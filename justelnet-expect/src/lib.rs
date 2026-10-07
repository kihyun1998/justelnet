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
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/kihyun1998/justelnet/main/logo/icons/justelnet-icon-light-128.png"
)]
#![doc(
    html_favicon_url = "https://raw.githubusercontent.com/kihyun1998/justelnet/main/logo/favicon/favicon-32.png"
)]

use std::borrow::Cow;
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
    fn find(&self, haystack: &[u8]) -> Option<(usize, usize)> {
        match &self.0 {
            Kind::Literal(literal) if literal.is_empty() => Some((0, 0)),
            Kind::Literal(literal) => haystack
                .windows(literal.len())
                .position(|window| window == literal.as_slice())
                .map(|start| (start, start + literal.len())),
            Kind::Regex(regex) => regex.find(haystack).map(|m| (m.start(), m.end())),
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

/// A client wrapped for automation, with the data no wait has consumed yet.
#[derive(Debug)]
pub struct Expect<S> {
    client: Client<S>,
    buffer: Vec<u8>,
}

impl<S> Expect<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    /// Wraps `client`, with nothing buffered.
    pub fn new(client: Client<S>) -> Self {
        Self {
            client,
            buffer: Vec::new(),
        }
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
    pub fn into_inner(self) -> (Client<S>, Vec<u8>) {
        (self.client, self.buffer)
    }

    async fn wait(&mut self, patterns: &[Pattern], timeout: Duration) -> Result<Match, Error> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(m) = self.take_match(patterns) {
                return Ok(m);
            }
            match tokio::time::timeout_at(deadline, self.client.next_event()).await {
                Err(_) => {
                    return Err(Error::Timeout {
                        buffer: self.buffer.clone(),
                    });
                }
                Ok(Ok(Event::Core(core::Event::Data(data)))) => self.buffer.extend(data),
                Ok(Ok(Event::Closed)) => {
                    return Err(Error::Closed {
                        buffer: self.buffer.clone(),
                    });
                }
                Ok(Ok(_)) => {}
                Ok(Err(e)) => return Err(e.into()),
            }
        }
    }

    /// The earliest match in the buffer, ties going to the pattern listed
    /// first, taken out of the buffer with the data ahead of it.
    fn take_match(&mut self, patterns: &[Pattern]) -> Option<Match> {
        let (index, start, end) = patterns
            .iter()
            .enumerate()
            .filter_map(|(i, p)| p.find(&self.buffer).map(|(start, end)| (i, start, end)))
            .min_by_key(|&(i, start, _)| (start, i))?;
        let rest = self.buffer.split_off(end);
        let matched = self.buffer.split_off(start);
        let before = std::mem::replace(&mut self.buffer, rest);
        Some(Match {
            index,
            before,
            matched,
        })
    }
}
