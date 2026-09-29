//! What the caller sends through the client, as bytes on the device's side.

use std::io;

use justelnet::core::{self, Command, OptionPolicy, Side, TelnetOption};
use justelnet::{Client, Error, Event};
use tokio::io::{AsyncWriteExt, DuplexStream, duplex};

mod common;

use common::received;

fn quiet() -> OptionPolicy {
    OptionPolicy::builder().refuse_all().build()
}

/// A client on `policy` whose device has taken everything sent so far.
async fn connected(policy: OptionPolicy) -> (Client<DuplexStream>, DuplexStream) {
    let (stream, device) = duplex(1024);
    (Client::new(stream, policy), device)
}

/// The device sends `bytes` and the client handles them up to one Event.
async fn device_sends(
    client: &mut Client<DuplexStream>,
    device: &mut DuplexStream,
    bytes: &[u8],
) -> Event {
    device.write_all(bytes).await.unwrap();
    client.next_event().await.unwrap()
}

#[tokio::test(start_paused = true)]
async fn send_data_turns_cr_into_the_end_of_line_and_doubles_iac() {
    let (mut client, mut device) = connected(quiet()).await;

    client.send_data(b"ls\r").await.unwrap();
    assert_eq!(received(&mut device).await, b"ls\r\n");

    client.send_data(b"\xff").await.unwrap();
    assert_eq!(received(&mut device).await, [0xff, 0xff]);
}

#[tokio::test(start_paused = true)]
async fn send_raw_doubles_only_iac() {
    let (mut client, mut device) = connected(quiet()).await;

    client.send_raw(b"a\r\n\xff").await.unwrap();

    assert_eq!(
        received(&mut device).await,
        [b'a', b'\r', b'\n', 0xff, 0xff]
    );
}

#[tokio::test(start_paused = true)]
async fn each_command_is_iac_and_its_code() {
    let (mut client, mut device) = connected(quiet()).await;
    let commands = [
        (Command::Break, 0xf3),
        (Command::InterruptProcess, 0xf4),
        (Command::AbortOutput, 0xf5),
        (Command::AreYouThere, 0xf6),
        (Command::EraseCharacter, 0xf7),
        (Command::EraseLine, 0xf8),
        (Command::NoOperation, 0xf1),
        (Command::GoAhead, 0xf9),
        (Command::DataMark, 0xf2),
    ];

    for (command, code) in commands {
        client.send_command(command).await.unwrap();
        assert_eq!(received(&mut device).await, [0xff, code], "{command:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn a_window_size_change_sends_naws() {
    let policy = OptionPolicy::builder()
        .refuse_all()
        .accept(TelnetOption::NAWS, Side::Local)
        .build();
    let (mut client, mut device) = connected(policy).await;
    device_sends(&mut client, &mut device, &[0xff, 0xfd, 0x1f]).await;
    assert_eq!(
        received(&mut device).await,
        [
            0xff, 0xfb, 0x1f, 0xff, 0xfa, 0x1f, 0x00, 0x50, 0x00, 0x18, 0xff, 0xf0
        ]
    );

    client.set_window_size(100, 40).await.unwrap();

    assert_eq!(
        received(&mut device).await,
        [0xff, 0xfa, 0x1f, 0x00, 0x64, 0x00, 0x28, 0xff, 0xf0]
    );
}

#[tokio::test(start_paused = true)]
async fn binary_is_requested_and_refused_at_runtime() {
    let (mut client, mut device) = connected(quiet()).await;

    client
        .request_enable(TelnetOption::BINARY, Side::Local)
        .await
        .unwrap();
    assert_eq!(received(&mut device).await, [0xff, 0xfb, 0x00]);
    client
        .request_enable(TelnetOption::BINARY, Side::Remote)
        .await
        .unwrap();
    assert_eq!(received(&mut device).await, [0xff, 0xfd, 0x00]);

    device_sends(&mut client, &mut device, &[0xff, 0xfd, 0x00]).await;
    client
        .request_disable(TelnetOption::BINARY, Side::Local)
        .await
        .unwrap();
    assert_eq!(received(&mut device).await, [0xff, 0xfc, 0x00]);
}

#[tokio::test(start_paused = true)]
async fn a_passthrough_subnegotiation_goes_out_once_the_option_is_on() {
    let policy = OptionPolicy::builder()
        .refuse_all()
        .request(TelnetOption::COM_PORT, Side::Local)
        .passthrough(TelnetOption::COM_PORT)
        .build();
    let (mut client, mut device) = connected(policy).await;

    let refused = client
        .send_subnegotiation(TelnetOption::COM_PORT, &[0x05])
        .await;
    assert!(matches!(
        refused,
        Err(Error::Core(core::Error::OptionNotEnabled {
            option: TelnetOption::COM_PORT
        }))
    ));
    assert_eq!(received(&mut device).await, []);

    device_sends(&mut client, &mut device, &[0xff, 0xfd, 0x2c]).await;
    assert_eq!(received(&mut device).await, [0xff, 0xfb, 0x2c]);
    client
        .send_subnegotiation(TelnetOption::COM_PORT, &[0x05, 0xff])
        .await
        .unwrap();

    assert_eq!(
        received(&mut device).await,
        [0xff, 0xfa, 0x2c, 0x05, 0xff, 0xff, 0xff, 0xf0]
    );
}

#[tokio::test(start_paused = true)]
async fn a_core_error_leaves_the_session_usable() {
    let (mut client, mut device) = connected(quiet()).await;

    let refused = client.send_subnegotiation(TelnetOption::TTYPE, b"x").await;
    assert!(matches!(
        refused,
        Err(Error::Core(core::Error::NotPassthrough {
            option: TelnetOption::TTYPE
        }))
    ));

    client.send_data(b"ok").await.unwrap();
    assert_eq!(received(&mut device).await, b"ok");
    assert_eq!(
        device_sends(&mut client, &mut device, b"hi").await,
        Event::Core(core::Event::Data(b"hi".to_vec()))
    );
}

#[tokio::test(start_paused = true)]
async fn option_state_follows_the_device() {
    let (mut client, mut device) = connected(OptionPolicy::default()).await;
    assert!(!client.is_enabled(TelnetOption::ECHO, Side::Remote));

    device_sends(&mut client, &mut device, &[0xff, 0xfb, 0x01]).await;

    assert!(client.is_enabled(TelnetOption::ECHO, Side::Remote));
    assert!(!client.is_enabled(TelnetOption::ECHO, Side::Local));

    device_sends(&mut client, &mut device, &[0xff, 0xfd, 0x03]).await;

    assert!(client.is_enabled(TelnetOption::SGA, Side::Local));
    assert!(!client.is_enabled(TelnetOption::SGA, Side::Remote));
}

#[tokio::test(start_paused = true)]
async fn nothing_is_sent_after_a_clean_close() {
    let (writer, mut device) = duplex(1024);
    let mut client = Client::new(tokio::io::join(tokio::io::empty(), writer), quiet());
    assert_eq!(client.next_event().await.unwrap(), Event::Closed);

    assert!(matches!(client.send_data(b"x").await, Err(Error::Closed)));
    assert!(matches!(
        client.send_command(Command::AreYouThere).await,
        Err(Error::Closed)
    ));
    assert!(matches!(
        client
            .request_enable(TelnetOption::BINARY, Side::Local)
            .await,
        Err(Error::Closed)
    ));

    assert_eq!(received(&mut device).await, []);
}

#[tokio::test(start_paused = true)]
async fn a_failed_send_ends_the_connection() {
    let (mut client, device) = connected(quiet()).await;
    drop(device);

    match client.send_data(b"x").await {
        Err(Error::Io(e)) => assert_eq!(e.kind(), io::ErrorKind::BrokenPipe),
        other => panic!("expected a broken pipe, got {other:?}"),
    }
    assert!(matches!(client.next_event().await, Err(Error::Closed)));
    assert!(matches!(client.send_raw(b"x").await, Err(Error::Closed)));
}
