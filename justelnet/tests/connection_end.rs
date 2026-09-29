//! How a connection ends, and what the caller sees afterwards.

use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};

use justelnet::core::{self, OptionPolicy, TelnetOption, Warning};
use justelnet::{Client, Error, Event};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadBuf, duplex};

/// A stream that yields `hi`, then fails every read with a connection reset,
/// counting the reads it is asked for. Writes succeed.
struct ResetAfterHi {
    reads: Arc<AtomicUsize>,
}

impl AsyncRead for ResetAfterHi {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.reads.fetch_add(1, Ordering::SeqCst) == 0 {
            buf.put_slice(b"hi");
            Poll::Ready(Ok(()))
        } else {
            Poll::Ready(Err(io::ErrorKind::ConnectionReset.into()))
        }
    }
}

impl AsyncWrite for ResetAfterHi {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

fn quiet() -> OptionPolicy {
    OptionPolicy::builder().refuse_all().build()
}

#[tokio::test(start_paused = true)]
async fn a_clean_close_is_closed_then_closed_errors() {
    let (stream, device) = duplex(1024);
    let mut client = Client::new(stream, quiet());
    drop(device);

    assert!(matches!(client.next_event().await, Ok(Event::Closed)));
    assert!(matches!(client.next_event().await, Err(Error::Closed)));
    assert!(matches!(client.next_event().await, Err(Error::Closed)));
}

#[tokio::test(start_paused = true)]
async fn a_failed_write_is_returned_then_closed_errors() {
    let (stream, device) = duplex(1024);
    let mut client = Client::new(stream, OptionPolicy::default());
    drop(device);

    match client.next_event().await {
        Err(Error::Io(e)) => assert_eq!(e.kind(), io::ErrorKind::BrokenPipe),
        other => panic!("expected a broken pipe, got {other:?}"),
    }
    assert!(matches!(client.next_event().await, Err(Error::Closed)));
}

#[tokio::test(start_paused = true)]
async fn a_failed_read_is_returned_then_closed_errors_without_reading() {
    let reads = Arc::new(AtomicUsize::new(0));
    let stream = ResetAfterHi {
        reads: reads.clone(),
    };
    let mut client = Client::new(stream, quiet());

    assert_eq!(
        client.next_event().await.unwrap(),
        Event::Core(core::Event::Data(b"hi".to_vec()))
    );
    match client.next_event().await {
        Err(Error::Io(e)) => assert_eq!(e.kind(), io::ErrorKind::ConnectionReset),
        other => panic!("expected a connection reset, got {other:?}"),
    }
    assert!(matches!(client.next_event().await, Err(Error::Closed)));
    assert!(matches!(client.next_event().await, Err(Error::Closed)));
    assert_eq!(reads.load(Ordering::SeqCst), 2);
}

#[tokio::test(start_paused = true)]
async fn a_malformed_subnegotiation_is_a_warning_and_the_session_goes_on() {
    let (stream, mut device) = duplex(1024);
    let mut client = Client::new(stream, quiet());
    device
        .write_all(&[0xff, 0xfa, 0x18, 0xff, 0x01, 0xff, 0xf0, b'z'])
        .await
        .unwrap();

    assert_eq!(
        client.next_event().await.unwrap(),
        Event::Core(core::Event::Warning(Warning::MalformedSubnegotiation {
            option: TelnetOption::TTYPE,
            byte: 0x01,
        }))
    );
    assert_eq!(
        client.next_event().await.unwrap(),
        Event::Core(core::Event::Data(b"z".to_vec()))
    );
    device.write_all(b"more").await.unwrap();
    assert_eq!(
        client.next_event().await.unwrap(),
        Event::Core(core::Event::Data(b"more".to_vec()))
    );
}

#[test]
fn errors_describe_themselves() {
    let io = Error::from(io::Error::from(io::ErrorKind::ConnectionReset));
    let inner = io::Error::from(io::ErrorKind::ConnectionReset);
    assert_eq!(io.to_string(), inner.to_string());

    let core = Error::from(core::Error::NotPassthrough {
        option: TelnetOption::NAWS,
    });
    assert_eq!(core.to_string(), "option 31 is not a Passthrough option");

    assert_eq!(Error::Closed.to_string(), "the connection is closed");
}

/// An error whose source is a connection reset.
#[derive(Debug)]
struct Wrapped(io::Error);

impl std::fmt::Display for Wrapped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("wrapped")
    }
}

impl std::error::Error for Wrapped {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

#[test]
fn an_io_error_keeps_its_source() {
    use std::error::Error as _;
    let inner = io::Error::other(Wrapped(io::ErrorKind::ConnectionReset.into()));
    let expected = inner.source().map(|e| e.to_string());
    assert!(expected.is_some());

    let error = Error::from(inner);

    assert_eq!(error.source().map(|e| e.to_string()), expected);
    assert!(Error::Closed.source().is_none());
}
