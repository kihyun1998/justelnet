//! A login to a real telnetd, the one in `ci/inetutils-telnetd`.
//!
//! Ignored by default; CI runs it against the container with `--ignored`.
//! `JUSTELNET_E2E_ADDR` is the server's address. `JUSTELNET_E2E_PASSWORD`
//! overrides the test account's password. `JUSTELNET_E2E_RECORDING` is where
//! the bytes each side sent are written, as `server:` and `client:` lines.

use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use justelnet::core::OptionPolicy;
use justelnet::{Client, Event};
use justelnet_expect::regex::bytes::Regex;
use justelnet_expect::{Error, Expect, Match, Pattern};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;

const USER: &str = "e2e";
const PASSWORD: &str = "justelnet-e2e";
const WAIT: Duration = Duration::from_secs(10);

#[tokio::test]
#[ignore = "needs the telnetd container; set JUSTELNET_E2E_ADDR"]
async fn a_login_to_inetutils_telnetd() {
    let addr = std::env::var("JUSTELNET_E2E_ADDR").expect("JUSTELNET_E2E_ADDR is not set");
    let password = std::env::var("JUSTELNET_E2E_PASSWORD").unwrap_or_else(|_| PASSWORD.into());
    let stream = TcpStream::connect(&addr).await.expect("connect");
    let (stream, _recording) = Recorder::new(stream);
    let mut s = Expect::new(Client::new(stream, OptionPolicy::default()));
    let shell = Regex::new(r"\ne2e\$ $").unwrap();

    wait(&mut s, Regex::new(r"login: $").unwrap(), "the login prompt").await;
    s.send_line(USER).await.unwrap();
    wait(
        &mut s,
        Regex::new(r"Password: $").unwrap(),
        "the password prompt",
    )
    .await;
    s.send_line(&password).await.unwrap();
    wait(&mut s, &shell, "the shell prompt").await;

    s.send_line("echo justelnet-$((6*7))").await.unwrap();
    let output = wait(&mut s, &shell, "the command's output").await;
    let lines: Vec<&[u8]> = output.before.split(|&b| b == b'\n').collect();
    assert!(
        lines.contains(&&b"justelnet-42\r"[..]),
        "no line `justelnet-42` in {:?}",
        output.before_lossy()
    );

    s.send_line("exit").await.unwrap();
    let (mut client, _) = s.into_inner();
    loop {
        let event = tokio::time::timeout(WAIT, client.next_event())
            .await
            .expect("no Closed Event after logout")
            .expect("the session ended with an error");
        if event == Event::Closed {
            break;
        }
    }
}

/// Waits for `pattern`, and on failure panics with what had arrived.
async fn wait<S>(s: &mut Expect<S>, pattern: impl Into<Pattern>, what: &str) -> Match
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    match s.expect(pattern, WAIT).await {
        Ok(m) => m,
        Err(Error::Timeout { buffer }) => panic!(
            "timed out waiting for {what}; received:\n{}",
            String::from_utf8_lossy(&buffer)
        ),
        Err(Error::Closed { buffer }) => panic!(
            "the connection closed waiting for {what}; received:\n{}",
            String::from_utf8_lossy(&buffer)
        ),
        Err(e) => panic!("waiting for {what}: {e}"),
    }
}

/// The bytes each side sent, in order, as runs tagged `server` or `client`.
type Log = Arc<Mutex<Vec<(&'static str, Vec<u8>)>>>;

/// A stream that logs the bytes read from and written to it.
struct Recorder<S> {
    inner: S,
    log: Log,
}

impl<S> Recorder<S> {
    /// The stream, and a guard that writes the log out when dropped.
    fn new(inner: S) -> (Self, Recording) {
        let log = Log::default();
        let recording = Recording(log.clone());
        (Self { inner, log }, recording)
    }

    fn record(&self, side: &'static str, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let mut log = self.log.lock().unwrap();
        match log.last_mut() {
            Some((last, run)) if *last == side => run.extend_from_slice(bytes),
            _ => log.push((side, bytes.to_vec())),
        }
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for Recorder<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let poll = Pin::new(&mut self.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = poll {
            self.record("server", &buf.filled()[before..]);
        }
        poll
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for Recorder<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let poll = Pin::new(&mut self.inner).poll_write(cx, buf);
        if let Poll::Ready(Ok(n)) = poll {
            self.record("client", &buf[..n]);
        }
        poll
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// Writes the recorded bytes to `JUSTELNET_E2E_RECORDING` when dropped.
struct Recording(Log);

impl Drop for Recording {
    fn drop(&mut self) {
        let Ok(path) = std::env::var("JUSTELNET_E2E_RECORDING") else {
            return;
        };
        let mut text = String::from(
            "# Recorded from GNU inetutils telnetd 2.8 (ci/inetutils-telnetd) by the\n\
             # end-to-end test, default Option policy. Bytes only: the `event:` lines\n\
             # a replayable Transcript needs are not recorded.\n",
        );
        for (side, bytes) in self.0.lock().unwrap().iter() {
            let hex: Vec<String> = bytes.iter().map(|b| format!("{b:02x}")).collect();
            text.push_str(&format!("{side}: {}\n", hex.join(" ")));
        }
        std::fs::write(&path, text).expect("write the recording");
    }
}
