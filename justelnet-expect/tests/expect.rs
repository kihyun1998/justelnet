//! The Expect session against a fake device on an in-memory duplex.
//!
//! Expected matches and buffers are the prototype's (`Expect` module of
//! `prototypes/expect-api.PROTOTYPE.html` on `prototype/expect-api`), run on
//! the same scenario steps.

use std::time::Duration;

use justelnet::core::{self, OptionPolicy, Side, TelnetOption};
use justelnet::{Client, Event};
use justelnet_expect::{Error, Expect, Match};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream, duplex};

const SECS_5: Duration = Duration::from_secs(5);

/// An Expect session on a client whose policy sends nothing unasked.
fn session() -> (Expect<DuplexStream>, DuplexStream) {
    let (stream, device) = duplex(1024);
    let client = Client::new(stream, OptionPolicy::builder().refuse_all().build());
    (Expect::new(client), device)
}

/// Every byte the device can read without waiting.
async fn received(device: &mut DuplexStream) -> Vec<u8> {
    let mut received = Vec::new();
    let mut buf = [0; 1024];
    while let Ok(Ok(n)) =
        tokio::time::timeout(Duration::from_millis(10), device.read(&mut buf)).await
    {
        if n == 0 {
            break;
        }
        received.extend_from_slice(&buf[..n]);
    }
    received
}

fn assert_match(m: &Match, index: usize, before: &[u8], matched: &[u8]) {
    assert_eq!(m.index, index);
    assert_eq!(m.before, before, "before");
    assert_eq!(m.matched, matched, "matched");
}

/// Scenario 1, a normal login, with its last wait for `>` alone.
#[tokio::test(start_paused = true)]
async fn a_normal_login() {
    let (mut s, mut device) = session();

    device
        .write_all(b"\r\nUser Access Verification\r\n\r\nUsername: ")
        .await
        .unwrap();
    let m = s.expect("Username: ", SECS_5).await.unwrap();
    assert_match(
        &m,
        0,
        b"\r\nUser Access Verification\r\n\r\n",
        b"Username: ",
    );

    s.send_line("admin").await.unwrap();
    assert_eq!(received(&mut device).await, b"admin\r\n");
    device.write_all(b"admin\r\nPassword: ").await.unwrap();
    let m = s.expect("Password: ", SECS_5).await.unwrap();
    assert_match(&m, 0, b"admin\r\n", b"Password: ");

    s.send_line("********").await.unwrap();
    assert_eq!(received(&mut device).await, b"********\r\n");
    device.write_all(b"\r\nR1>").await.unwrap();
    let m = s.expect(">", SECS_5).await.unwrap();
    assert_match(&m, 0, b"\r\nR1", b">");
}

/// Scenario 2: a prompt split across reads still matches, and not before
/// its second half arrives.
#[tokio::test(start_paused = true)]
async fn a_prompt_split_across_reads() {
    let (mut s, mut device) = session();
    device.write_all(b"Pass").await.unwrap();
    let start = tokio::time::Instant::now();

    let (m, ()) = tokio::join!(s.expect("Password: ", SECS_5), async {
        tokio::time::sleep(Duration::from_secs(1)).await;
        device.write_all(b"word: ").await.unwrap();
    });

    let m = m.unwrap();
    assert_match(&m, 0, b"", b"Password: ");
    assert_eq!(start.elapsed(), Duration::from_secs(1));
}

/// Scenario 5: a timeout carries what had arrived, and keeps it buffered.
#[tokio::test(start_paused = true)]
async fn a_timeout_carries_the_confirm_buffer() {
    let (mut s, mut device) = session();
    s.send_line("reload").await.unwrap();
    device
        .write_all(b"reload\r\nProceed with reload? [confirm]")
        .await
        .unwrap();
    let start = tokio::time::Instant::now();

    let result = s.expect("#", SECS_5).await;

    assert_eq!(start.elapsed(), SECS_5);
    match result {
        Err(Error::Timeout { buffer }) => {
            assert_eq!(buffer, b"reload\r\nProceed with reload? [confirm]")
        }
        other => panic!("expected a timeout, got {other:?}"),
    }
    let m = s.expect("[confirm]", SECS_5).await.unwrap();
    assert_match(&m, 0, b"reload\r\nProceed with reload? ", b"[confirm]");
}

#[tokio::test(start_paused = true)]
async fn data_after_the_match_stays_buffered() {
    let (mut s, mut device) = session();
    device
        .write_all(b"\r\n% Login invalid\r\n\r\nUsername: ")
        .await
        .unwrap();

    let m = s.expect("% Login invalid", SECS_5).await.unwrap();
    assert_match(&m, 0, b"\r\n", b"% Login invalid");
    drop(device);

    let m = s.expect("Username: ", SECS_5).await.unwrap();
    assert_match(&m, 0, b"\r\n\r\n", b"Username: ");
}

#[tokio::test(start_paused = true)]
async fn non_data_events_are_skipped() {
    let (mut s, mut device) = session();
    device
        .write_all(b"lo\xff\xfb\x01gi\xff\xf1n: ")
        .await
        .unwrap();

    let m = s.expect("login: ", SECS_5).await.unwrap();

    assert_match(&m, 0, b"", b"login: ");
}

#[tokio::test(start_paused = true)]
async fn a_close_during_a_wait_carries_the_buffer() {
    let (mut s, mut device) = session();
    device.write_all(b"Connection closed by ").await.unwrap();
    drop(device);

    match s.expect("#", SECS_5).await {
        Err(Error::Closed { buffer }) => assert_eq!(buffer, b"Connection closed by "),
        other => panic!("expected a close, got {other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn the_client_comes_back_working_with_the_rest_of_the_buffer() {
    let (mut s, mut device) = session();
    device.write_all(b"\r\nR1>show ver").await.unwrap();
    s.expect(">", SECS_5).await.unwrap();
    s.send_command(core::Command::AreYouThere).await.unwrap();
    s.send_raw(b"\xff").await.unwrap();
    assert_eq!(received(&mut device).await, [0xff, 0xf6, 0xff, 0xff]);

    let (mut client, rest) = s.into_inner();

    assert_eq!(rest, b"show ver");
    assert!(!client.is_enabled(TelnetOption::ECHO, Side::Remote));
    client.send_data(b"x").await.unwrap();
    assert_eq!(received(&mut device).await, b"x");
    device.write_all(b"ion").await.unwrap();
    assert_eq!(
        client.next_event().await.unwrap(),
        Event::Core(core::Event::Data(b"ion".to_vec()))
    );
}

#[test]
fn errors_describe_themselves() {
    let timeout = Error::Timeout {
        buffer: b"[confirm]".to_vec(),
    };
    assert_eq!(
        timeout.to_string(),
        "timed out waiting for a pattern (9 bytes buffered)"
    );
    let closed = Error::Closed { buffer: Vec::new() };
    assert_eq!(
        closed.to_string(),
        "the connection closed while waiting for a pattern (0 bytes buffered)"
    );
    let client = Error::from(justelnet::Error::Closed);
    assert_eq!(client.to_string(), "the connection is closed");
}

#[tokio::test(start_paused = true)]
async fn the_timeout_counts_from_the_start_of_the_wait() {
    let (mut s, mut device) = session();
    let start = tokio::time::Instant::now();

    let ((result, waited), ()) = tokio::join!(
        async { (s.expect("#", SECS_5).await, start.elapsed()) },
        async {
            for _ in 0..5 {
                tokio::time::sleep(Duration::from_secs(2)).await;
                device.write_all(b".").await.unwrap();
            }
        }
    );

    assert!(matches!(result, Err(Error::Timeout { .. })), "{result:?}");
    assert_eq!(waited, SECS_5);
    assert!(matches!(
        s.expect("#", Duration::ZERO).await,
        Err(Error::Timeout { buffer }) if buffer == b"....."
    ));
}

#[tokio::test(start_paused = true)]
async fn the_first_occurrence_matches() {
    let (mut s, mut device) = session();
    device.write_all(b"R1>R2>").await.unwrap();

    let m = s.expect(">", SECS_5).await.unwrap();
    assert_match(&m, 0, b"R1", b">");
    let m = s.expect("", SECS_5).await.unwrap();
    assert_match(&m, 0, b"", b"");
    let m = s.expect(">", SECS_5).await.unwrap();
    assert_match(&m, 0, b"R2", b">");
}

#[tokio::test(start_paused = true)]
async fn option_state_is_read_through_the_session() {
    let (stream, mut device) = duplex(1024);
    let policy = OptionPolicy::builder()
        .refuse_all()
        .accept(TelnetOption::ECHO, Side::Remote)
        .build();
    let mut s = Expect::new(Client::new(stream, policy));
    assert!(!s.client().is_enabled(TelnetOption::ECHO, Side::Remote));
    device.write_all(b"\xff\xfb\x01login: ").await.unwrap();

    s.expect("login: ", SECS_5).await.unwrap();

    assert!(s.client().is_enabled(TelnetOption::ECHO, Side::Remote));
}
