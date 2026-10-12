//! Expect-style automation over a justelnet client.
//!
//! An [`Expect`] session owns a [`Client`] and the data it has received but no
//! wait has consumed yet. [`Expect::expect`] waits for a pattern in that data,
//! [`Expect::expect_any`] for any of several, and [`Expect::into_inner`] hands
//! the client back.
//!
//! A pattern is a literal or a byte [`Regex`], matched
//! against the received bytes in whatever encoding they arrive. Wait for a
//! prompt with a regex anchored to the end of the data, so that a `#` or `>`
//! inside a banner does not match first:
//!
//! ```no_run
//! use std::time::Duration;
//!
//! use justelnet::Client;
//! use justelnet::core::OptionPolicy;
//! use justelnet_expect::Expect;
//! use justelnet_expect::regex::bytes::Regex;
//!
//! # async fn login() -> Result<(), Box<dyn std::error::Error>> {
//! let secs = Duration::from_secs;
//! let client = Client::connect("192.0.2.1:23", OptionPolicy::default(), secs(5)).await?;
//! let mut s = Expect::new(client);
//! let prompt = Regex::new(r"\n\S+[#>] ?$")?;
//!
//! s.expect("Username: ", secs(10)).await?;
//! s.send_line("admin").await?;
//! s.expect("Password: ", secs(10)).await?;
//! s.send_line("secret").await?;
//! s.expect(&prompt, secs(10)).await?;
//!
//! s.send_line("show version").await?;
//! let output = s.expect(&prompt, secs(10)).await?;
//! println!("{}", output.before_lossy());
//! # Ok(())
//! # }
//! ```
//!
//! `\S` matches only UTF-8 text. Where the prompt itself is in another
//! encoding, such as a hostname in EUC-KR, use `(?-u:\S)`, which matches any
//! byte that is not ASCII whitespace.
//!
//! # Long output and `--More--`
//!
//! A device that pages its output stops at a marker such as `--More--` and
//! waits for a key. The session does not answer it for you, since markers and
//! keys differ by vendor; wait for the marker or the prompt, and send the key
//! on the marker:
//!
//! ```no_run
//! # use std::time::Duration;
//! # use justelnet_expect::{Expect, Pattern};
//! # use justelnet_expect::regex::bytes::Regex;
//! # async fn page(s: &mut Expect<tokio::net::TcpStream>) -> Result<(), Box<dyn std::error::Error>> {
//! # let secs = Duration::from_secs;
//! let prompt = Regex::new(r"\n\S+[#>] ?$")?;
//! s.send_line("show running-config").await?;
//! let mut output = Vec::new();
//! loop {
//!     let m = s
//!         .expect_any(&[Pattern::from(" --More-- "), Pattern::from(&prompt)], secs(10))
//!         .await?;
//!     output.extend_from_slice(&m.before);
//!     if m.index == 1 {
//!         break;
//!     }
//!     s.send_raw(b" ").await?;
//! }
//! # Ok(())
//! # }
//! ```
//!
//! Where the device allows it, turning paging off for the session is simpler:
//! `terminal length 0` on Cisco IOS, `set cli screen-length 0` on Junos.
//!
//! A session keeps at most [`DEFAULT_BUFFER_LIMIT`] (1 MiB) of unmatched data
//! and drops the oldest bytes past it, so one wait returns at most that much in
//! [`Match::before`]. Paging keeps each wait short;
//! [`Expect::set_buffer_limit`] raises the limit for one long output.
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/kihyun1998/justelnet/main/logo/icons/justelnet-icon-light-128.png"
)]
#![doc(
    html_favicon_url = "https://raw.githubusercontent.com/kihyun1998/justelnet/main/logo/favicon/favicon-32.png"
)]

use std::borrow::Cow;
use std::fmt;
use std::time::Duration;

use justelnet::core::{self, Command};
use justelnet::{Client, Event};
use regex::bytes::Regex;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::time::Instant;

/// The regex crate, re-exported for building byte regex patterns.
pub use regex;

/// What a wait looks for in the received data: a literal `&str` or a byte
/// [`Regex`].
#[derive(Debug, Clone)]
pub struct Pattern(Kind);

#[derive(Debug, Clone)]
enum Kind {
    Literal(Vec<u8>),
    Regex(Regex),
}

impl Pattern {
    /// Where the pattern first matches in `haystack`, as a start and end.
    /// A literal is looked for from `from` on; a regex always searches the
    /// whole of `haystack`.
    fn find(&self, haystack: &[u8], from: usize) -> Option<(usize, usize)> {
        match &self.0 {
            Kind::Literal(literal) if literal.is_empty() => Some((0, 0)),
            Kind::Literal(literal) => memchr::memmem::find(&haystack[from..], literal)
                .map(|start| (from + start, from + start + literal.len())),
            Kind::Regex(regex) => regex.find(haystack).map(|m| (m.start(), m.end())),
        }
    }

    /// Where the next search can start, after a `haystack` of `searched`
    /// bytes held no match: a literal's start in its last `len - 1` bytes,
    /// the beginning for a regex.
    fn resume_from(&self, searched: usize) -> usize {
        match &self.0 {
            Kind::Literal(literal) => searched.saturating_sub(literal.len().saturating_sub(1)),
            Kind::Regex(_) => 0,
        }
    }
}

/// A literal, matched as its UTF-8 bytes.
impl From<&str> for Pattern {
    fn from(literal: &str) -> Self {
        Pattern(Kind::Literal(literal.as_bytes().to_vec()))
    }
}

/// A byte regex, matched against the received bytes as they are.
impl From<Regex> for Pattern {
    fn from(regex: Regex) -> Self {
        Pattern(Kind::Regex(regex))
    }
}

/// A byte regex, for waiting on the same regex more than once.
impl From<&Regex> for Pattern {
    fn from(regex: &Regex) -> Self {
        Pattern(Kind::Regex(regex.clone()))
    }
}

/// A successful wait.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Match {
    /// Which pattern matched, counting from 0.
    pub index: usize,
    /// The data ahead of the match.
    pub before: Vec<u8>,
    /// The matched bytes.
    pub matched: Vec<u8>,
}

impl Match {
    /// `before` as text, with each invalid UTF-8 sequence shown as U+FFFD.
    pub fn before_lossy(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.before)
    }

    /// `matched` as text, with each invalid UTF-8 sequence shown as U+FFFD.
    pub fn matched_lossy(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.matched)
    }
}

/// Why a wait or a call failed.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// No pattern matched within the timeout. `buffer` is the unmatched data,
    /// which the session keeps for the next wait.
    Timeout { buffer: Vec<u8> },
    /// The peer closed the connection during the wait. `buffer` is the
    /// unmatched data.
    Closed { buffer: Vec<u8> },
    /// The client failed.
    Client(justelnet::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Timeout { buffer } => write!(
                f,
                "timed out waiting for a pattern ({} bytes buffered)",
                buffer.len()
            ),
            Error::Closed { buffer } => write!(
                f,
                "the connection closed while waiting for a pattern ({} bytes buffered)",
                buffer.len()
            ),
            Error::Client(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Client(e) => e.source(),
            Error::Timeout { .. } | Error::Closed { .. } => None,
        }
    }
}

impl From<justelnet::Error> for Error {
    fn from(e: justelnet::Error) -> Self {
        Error::Client(e)
    }
}

/// How many unmatched bytes a session keeps unless told otherwise: 1 MiB.
pub const DEFAULT_BUFFER_LIMIT: usize = 1 << 20;

/// A client wrapped for automation, with the data no wait has consumed yet.
pub struct Expect<S> {
    client: Client<S>,
    /// The unmatched data is `buffer[head..]`; the bytes ahead of `head`
    /// were dropped past the limit and are not yet reclaimed.
    buffer: Vec<u8>,
    head: usize,
    limit: usize,
}

/// Shows the client, the unmatched data and the limit.
impl<S: fmt::Debug> fmt::Debug for Expect<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Expect")
            .field("client", &self.client)
            .field("buffer", &self.unmatched())
            .field("limit", &self.limit)
            .finish()
    }
}

impl<S> Expect<S> {
    /// The data no wait has consumed yet.
    fn unmatched(&self) -> &[u8] {
        &self.buffer[self.head..]
    }
}

impl<S> Expect<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    /// Wraps `client`, with nothing buffered and the buffer limited to
    /// [`DEFAULT_BUFFER_LIMIT`].
    pub fn new(client: Client<S>) -> Self {
        Self {
            client,
            buffer: Vec::new(),
            head: 0,
            limit: DEFAULT_BUFFER_LIMIT,
        }
    }

    /// Keeps at most `bytes` of unmatched data, dropping the oldest first,
    /// now and as data arrives.
    ///
    /// A prompt arrives last, so it still matches, but a wait whose output
    /// runs past the limit returns only the newest `bytes` of it in
    /// [`Match::before`]. Raise the limit for one long output, or take it
    /// page by page (see the crate documentation). The buffer carried by
    /// [`Error::Timeout`] and [`Error::Closed`] is limited the same way.
    pub fn set_buffer_limit(&mut self, bytes: usize) {
        self.limit = bytes;
        self.trim();
    }

    /// Waits up to `timeout` for `pattern` in the buffered and incoming data.
    ///
    /// Data after the match stays buffered for the next wait. Events other
    /// than Data are skipped. On a timeout the buffer is kept.
    pub async fn expect(
        &mut self,
        pattern: impl Into<Pattern>,
        timeout: Duration,
    ) -> Result<Match, Error> {
        self.wait(&[pattern.into()], timeout).await
    }

    /// Waits up to `timeout` for any of `patterns`; [`Match::index`] says
    /// which one matched.
    ///
    /// The earliest match in the data wins, and of patterns matching at the
    /// same position, the one listed first. Literals and regexes mix as
    /// [`Pattern`]s: `&[Pattern::from("% Login invalid"), Pattern::from(&prompt)]`.
    /// With no patterns, nothing matches and the wait ends at the timeout.
    /// Otherwise as [`Expect::expect`].
    pub async fn expect_any<P>(&mut self, patterns: &[P], timeout: Duration) -> Result<Match, Error>
    where
        P: Into<Pattern> + Clone,
    {
        let patterns: Vec<Pattern> = patterns.iter().cloned().map(Into::into).collect();
        self.wait(&patterns, timeout).await
    }

    /// Sends `line` followed by Enter, which the client turns into the
    /// policy's end of line.
    pub async fn send_line(&mut self, line: impl AsRef<[u8]>) -> Result<(), Error> {
        let mut data = line.as_ref().to_vec();
        data.push(b'\r');
        Ok(self.client.send_data(&data).await?)
    }

    /// Sends bytes as they are, with only IAC doubled.
    pub async fn send_raw(&mut self, data: &[u8]) -> Result<(), Error> {
        Ok(self.client.send_raw(data).await?)
    }

    /// Sends `IAC <command>`.
    pub async fn send_command(&mut self, command: Command) -> Result<(), Error> {
        Ok(self.client.send_command(command).await?)
    }

    /// The wrapped client, for queries such as option state.
    pub fn client(&self) -> &Client<S> {
        &self.client
    }

    /// Hands back the client and the data no wait has consumed.
    pub fn into_inner(mut self) -> (Client<S>, Vec<u8>) {
        self.reclaim();
        (self.client, self.buffer)
    }

    async fn wait(&mut self, patterns: &[Pattern], timeout: Duration) -> Result<Match, Error> {
        let deadline = Instant::now() + timeout;
        let mut from = vec![0; patterns.len()];
        loop {
            if let Some(m) = self.take_match(patterns, &mut from) {
                return Ok(m);
            }
            match tokio::time::timeout_at(deadline, self.client.next_event()).await {
                Err(_) => {
                    return Err(Error::Timeout {
                        buffer: self.unmatched().to_vec(),
                    });
                }
                Ok(Ok(Event::Core(core::Event::Data(data)))) => {
                    self.append(&data);
                    let dropped = self.trim();
                    for start in &mut from {
                        *start = start.saturating_sub(dropped);
                    }
                }
                Ok(Ok(Event::Closed)) => {
                    return Err(Error::Closed {
                        buffer: self.unmatched().to_vec(),
                    });
                }
                Ok(Ok(_)) => {}
                Ok(Err(e)) => return Err(e.into()),
            }
        }
    }

    /// Appends `data`, reclaiming the dropped bytes first where the buffer
    /// would otherwise grow to hold them, with room left for as much again.
    fn append(&mut self, data: &[u8]) {
        if self.head > 0 && self.buffer.len() + data.len() > self.buffer.capacity() {
            self.reclaim();
            self.buffer.reserve_exact(self.buffer.len() + data.len());
        }
        self.buffer.extend_from_slice(data);
    }

    /// Moves the unmatched data to the front, freeing the dropped bytes.
    fn reclaim(&mut self) {
        self.buffer.drain(..self.head);
        self.head = 0;
    }

    /// Drops the oldest unmatched bytes past the limit and returns how many.
    /// The dropped bytes are reclaimed once they reach the limit.
    fn trim(&mut self) -> usize {
        let excess = self.unmatched().len().saturating_sub(self.limit);
        self.head += excess;
        if self.head >= self.limit {
            self.reclaim();
        }
        excess
    }

    /// The earliest match in the unmatched data, ties going to the pattern
    /// listed first, taken out of the buffer with the data ahead of it.
    /// `from` holds where each pattern's search starts, and is moved on for
    /// each pattern that does not match.
    fn take_match(&mut self, patterns: &[Pattern], from: &mut [usize]) -> Option<Match> {
        let unmatched = self.unmatched();
        let mut earliest: Option<(usize, usize, usize)> = None;
        for (i, pattern) in patterns.iter().enumerate() {
            match pattern.find(unmatched, from[i]) {
                Some((start, end)) => {
                    if earliest.is_none_or(|(_, first, _)| start < first) {
                        earliest = Some((i, start, end));
                    }
                }
                None => from[i] = pattern.resume_from(unmatched.len()),
            }
        }
        let (index, start, end) = earliest?;
        let rest = self.buffer.split_off(self.head + end);
        let matched = self.buffer.split_off(self.head + start);
        let before = self.buffer.split_off(self.head);
        self.buffer = rest;
        self.head = 0;
        Some(Match {
            index,
            before,
            matched,
        })
    }
}

#[cfg(test)]
mod tests {
    use justelnet::core::OptionPolicy;
    use tokio::io::{AsyncWriteExt, duplex};

    use super::*;

    /// The allocation stays within about twice the limit plus one read, also
    /// when a one-byte read shifts where the dropped bytes add up to the limit.
    #[tokio::test(start_paused = true)]
    async fn the_buffer_allocation_stays_near_twice_the_limit() {
        const LIMIT: usize = 1024;
        const READ: usize = 256;
        let (stream, mut device) = duplex(4096);
        let mut s = Expect::new(Client::new(
            stream,
            OptionPolicy::builder().refuse_all().build(),
        ));
        s.set_buffer_limit(LIMIT);

        let (result, ()) = tokio::join!(s.expect("#", Duration::from_secs(60)), async {
            for i in 0..64 {
                if i == 8 {
                    device.write_all(b"x").await.unwrap();
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                device.write_all(&[b'x'; READ]).await.unwrap();
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        });

        assert!(matches!(result, Err(Error::Timeout { .. })), "{result:?}");
        assert!(
            s.buffer.capacity() <= 2 * LIMIT + READ,
            "capacity {} for a limit of {LIMIT}",
            s.buffer.capacity()
        );
    }
}
