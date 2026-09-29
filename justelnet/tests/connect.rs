//! Connecting by address, against local TCP listeners.

use std::io;
use std::time::Duration;

use justelnet::core::{self, OptionPolicy};
use justelnet::{Client, Error, Event};
use socket2::SockRef;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Generous, so that a slow CI machine never trips it.
const TIMEOUT: Duration = Duration::from_secs(30);

#[tokio::test]
async fn the_listener_receives_the_active_start() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let (client, accepted) = tokio::join!(
        Client::connect(addr, OptionPolicy::default(), TIMEOUT),
        listener.accept()
    );
    let mut client = client.unwrap();
    let (mut server, _) = accepted.unwrap();
    server.write_all(b"hi").await.unwrap();

    assert_eq!(
        client.next_event().await.unwrap(),
        Event::Core(core::Event::Data(b"hi".to_vec()))
    );
    let mut start = [0; 18];
    server.read_exact(&mut start).await.unwrap();
    assert_eq!(
        start,
        [
            0xff, 0xfb, 0x1f, 0xff, 0xfb, 0x18, 0xff, 0xfb, 0x27, 0xff, 0xfd, 0x01, 0xff, 0xfb,
            0x03, 0xff, 0xfd, 0x03
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn an_unanswered_connect_times_out() {
    let timeout = Duration::from_secs(5);
    let start = tokio::time::Instant::now();

    // TEST-NET-1 (RFC 5737): nothing answers there.
    let result = Client::connect("192.0.2.1:23", OptionPolicy::default(), timeout).await;

    assert!(
        matches!(result, Err(Error::ConnectTimeout)),
        "{:?}",
        result.err()
    );
    let waited = start.elapsed();
    assert!(
        waited >= timeout && waited < timeout + Duration::from_secs(1),
        "{waited:?}"
    );
    assert_eq!(Error::ConnectTimeout.to_string(), "connecting timed out");
}

#[tokio::test]
async fn a_refused_connect_is_an_io_error() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    match Client::connect(addr, OptionPolicy::default(), TIMEOUT).await {
        Err(Error::Io(e)) => assert_eq!(e.kind(), io::ErrorKind::ConnectionRefused),
        other => panic!("expected a refused connection, got {:?}", other.err()),
    }
}

#[tokio::test]
async fn urgent_data_arrives_inline() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let policy = OptionPolicy::builder().refuse_all().build();

    let (client, accepted) =
        tokio::join!(Client::connect(addr, policy, TIMEOUT), listener.accept());
    let mut client = client.unwrap();
    let (mut server, _) = accepted.unwrap();
    server.write_all(b"a").await.unwrap();
    SockRef::from(&server).send_out_of_band(b"b").unwrap();
    server.write_all(b"c").await.unwrap();
    drop(server);

    let mut data = Vec::new();
    loop {
        match client.next_event().await.unwrap() {
            Event::Core(core::Event::Data(bytes)) => data.extend(bytes),
            Event::Closed => break,
            event => panic!("unexpected {event:?}"),
        }
    }
    assert_eq!(data, b"abc");
}
