//! The client against a fake device on the other end of an in-memory duplex.

use std::time::Duration;

use justelnet::core::{self, Command, OptionPolicy, Side, TelnetOption};
use justelnet::{Client, Event};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter, duplex};

mod common;

use common::received;

#[tokio::test(start_paused = true)]
async fn active_start_bytes_reach_the_device() {
    let (stream, mut device) = duplex(1024);
    let mut client = Client::new(stream, OptionPolicy::default());
    device.write_all(b"hi").await.unwrap();

    let event = client.next_event().await.unwrap();

    assert_eq!(event, Event::Core(core::Event::Data(b"hi".to_vec())));
    assert_eq!(received(&mut device).await, ACTIVE_START);
}

#[tokio::test(start_paused = true)]
async fn the_answer_is_written_before_the_next_data() {
    let policy = OptionPolicy::builder()
        .refuse_all()
        .accept(TelnetOption::TTYPE, Side::Local)
        .build();
    let (stream, mut device) = duplex(1024);
    let mut client = Client::new(stream, policy);
    assert_eq!(received(&mut device).await, []);
    device.write_all(&[0xff, 0xfd, 0x18]).await.unwrap();
    device.write_all(b"login: ").await.unwrap();

    let mut events = Vec::new();
    loop {
        let event = client.next_event().await.unwrap();
        let is_data = matches!(event, Event::Core(core::Event::Data(_)));
        events.push(event);
        if is_data {
            break;
        }
    }

    assert_eq!(received(&mut device).await, [0xff, 0xfb, 0x18]);
    assert_eq!(
        events,
        [
            Event::Core(core::Event::OptionChanged {
                option: TelnetOption::TTYPE,
                side: Side::Local,
                enabled: true,
            }),
            Event::Core(core::Event::Data(b"login: ".to_vec())),
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn data_reaches_the_caller_with_telnet_control_removed() {
    let (stream, mut device) = duplex(1024);
    let mut client = Client::new(stream, OptionPolicy::builder().refuse_all().build());
    device.write_all(b"a\xff\xffb\xff\xf1c\r\0d").await.unwrap();
    drop(device);

    let mut data = Vec::new();
    let mut commands = Vec::new();
    loop {
        match client.next_event().await.unwrap() {
            Event::Core(core::Event::Data(bytes)) => data.extend(bytes),
            Event::Core(core::Event::Command(command)) => commands.push(command),
            Event::Closed => break,
            event => panic!("unexpected {event:?}"),
        }
    }

    assert_eq!(data, b"a\xffbc\rd");
    assert_eq!(commands, [Command::NoOperation]);
}

const ACTIVE_START: [u8; 18] = [
    0xff, 0xfb, 0x1f, 0xff, 0xfb, 0x18, 0xff, 0xfb, 0x27, 0xff, 0xfd, 0x01, 0xff, 0xfb, 0x03, 0xff,
    0xfd, 0x03,
];

#[tokio::test(start_paused = true)]
async fn written_bytes_are_flushed_through_a_buffered_stream() {
    let (stream, mut device) = duplex(1024);
    let mut client = Client::new(BufWriter::new(stream), OptionPolicy::default());
    device.write_all(b"hi").await.unwrap();

    client.next_event().await.unwrap();

    assert_eq!(received(&mut device).await, ACTIVE_START);
}

#[tokio::test(start_paused = true)]
async fn output_larger_than_the_pipe_is_written_whole() {
    let (stream, mut device) = duplex(4);
    let mut client = Client::new(stream, OptionPolicy::default());

    let exchange = async {
        tokio::join!(client.next_event(), async {
            let mut sent = [0; 18];
            device.read_exact(&mut sent).await.unwrap();
            device.write_all(b"hi").await.unwrap();
            sent
        })
    };
    let (event, sent) = tokio::time::timeout(Duration::from_secs(1), exchange)
        .await
        .expect("the exchange stalled");

    assert_eq!(
        event.unwrap(),
        Event::Core(core::Event::Data(b"hi".to_vec()))
    );
    assert_eq!(sent, ACTIVE_START);
    assert_eq!(received(&mut device).await, []);
}
