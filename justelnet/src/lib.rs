//! Telnet client for tokio, built on `justelnet-core`.
//!
//! A [`Client`] owns one [`core::Core`] and one stream. The caller loops on
//! [`Client::next_event`], which writes the Core's answers to the peer before
//! it returns. There is no background task.
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/kihyun1998/justelnet/main/logo/icons/justelnet-icon-light-128.png"
)]
#![doc(
    html_favicon_url = "https://raw.githubusercontent.com/kihyun1998/justelnet/main/logo/favicon/favicon-32.png"
)]

use std::io;

use justelnet_core::{Command, Core, OptionPolicy, Side, TelnetOption};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// The sans-IO protocol Core, re-exported.
pub use justelnet_core as core;

/// Something the client reports to its caller.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Event {
    /// An Event from the Core.
    Core(core::Event),
    /// The peer closed the connection cleanly.
    Closed,
}

/// Why a client call failed.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// Reading from or writing to the stream failed. The connection has ended.
    Io(io::Error),
    /// The Core refused the call.
    Core(core::Error),
    /// The connection has ended, by a clean close or an earlier I/O error.
    Closed,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => e.fmt(f),
            Error::Core(e) => e.fmt(f),
            Error::Closed => f.write_str("the connection is closed"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => e.source(),
            Error::Core(e) => e.source(),
            Error::Closed => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<core::Error> for Error {
    fn from(e: core::Error) -> Self {
        Error::Core(e)
    }
}

/// A Telnet client over one stream.
#[derive(Debug)]
pub struct Client<S> {
    core: Core,
    stream: S,
    /// Where bytes read from the stream land before the Core takes them.
    read_buf: Box<[u8]>,
    /// Bytes taken from the Core for writing, of which `written` are written.
    outgoing: Vec<u8>,
    written: usize,
    /// Whether written bytes may still sit in the stream's own buffer.
    unflushed: bool,
    /// Whether the connection has ended, by a clean close or an I/O error.
    closed: bool,
}

impl<S> Client<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    /// A client over `stream`, answering the peer from `policy`. An active
    /// start's requests are written by the first [`Client::next_event`].
    ///
    /// A TCP stream should keep urgent data inline (`SO_OOBINLINE`, e.g.
    /// socket2's `set_out_of_band_inline`): otherwise a peer's Synch can lose
    /// bytes before they reach the Core.
    pub fn new(stream: S, policy: OptionPolicy) -> Self {
        Self {
            core: Core::with_policy(policy),
            stream,
            read_buf: vec![0; 4096].into_boxed_slice(),
            outgoing: Vec::new(),
            written: 0,
            unflushed: false,
            closed: false,
        }
    }

    /// Waits for the next Event. Every byte the Core has queued for the peer,
    /// its automatic answers included, is written first.
    ///
    /// A clean close by the peer is [`Event::Closed`]; a failed read or write
    /// is [`Error::Io`]. Either ends the connection, and every later call
    /// returns [`Error::Closed`].
    pub async fn next_event(&mut self) -> Result<Event, Error> {
        if self.closed {
            return Err(Error::Closed);
        }
        let result = self.drive().await;
        if matches!(result, Ok(Event::Closed) | Err(_)) {
            self.closed = true;
        }
        result.map_err(Error::Io)
    }

    /// Sends `data`: a CR means Enter and becomes the policy's end of line,
    /// unless BINARY is on on our side; IAC is doubled.
    pub async fn send_data(&mut self, data: &[u8]) -> Result<(), Error> {
        self.send_with(|core| {
            core.send_data(data);
            Ok(())
        })
        .await
    }

    /// Sends bytes as they are, with only IAC doubled.
    pub async fn send_raw(&mut self, data: &[u8]) -> Result<(), Error> {
        self.send_with(|core| {
            core.send_raw(data);
            Ok(())
        })
        .await
    }

    /// Sends `IAC <command>`.
    pub async fn send_command(&mut self, command: Command) -> Result<(), Error> {
        self.send_with(|core| {
            core.send_command(command);
            Ok(())
        })
        .await
    }

    /// Records a new window size, and sends it with NAWS if NAWS is on and
    /// the size changed.
    pub async fn set_window_size(&mut self, width: u16, height: u16) -> Result<(), Error> {
        self.send_with(|core| {
            core.set_window_size(width, height);
            Ok(())
        })
        .await
    }

    /// Asks the peer to enable `option` on `side`, whether or not the policy
    /// accepts it. Sends nothing if the option is on or already being asked for.
    pub async fn request_enable(&mut self, option: TelnetOption, side: Side) -> Result<(), Error> {
        self.send_with(|core| {
            core.request_enable(option, side);
            Ok(())
        })
        .await
    }

    /// Asks the peer to disable `option` on `side`. The option counts as off
    /// from this call on. Sends nothing if the option is off or already being
    /// asked off.
    pub async fn request_disable(&mut self, option: TelnetOption, side: Side) -> Result<(), Error> {
        self.send_with(|core| {
            core.request_disable(option, side);
            Ok(())
        })
        .await
    }

    /// Sends `IAC SB option data IAC SE` for a Passthrough option, doubling
    /// IAC in `data`. Fails with [`Error::Core`], sending nothing and leaving
    /// the connection usable, if `option` is not a Passthrough option or is
    /// off on both sides.
    pub async fn send_subnegotiation(
        &mut self,
        option: TelnetOption,
        data: &[u8],
    ) -> Result<(), Error> {
        self.send_with(|core| core.send_subnegotiation(option, data))
            .await
    }

    /// Whether `option` is currently on for `side`.
    pub fn is_enabled(&self, option: TelnetOption, side: Side) -> bool {
        self.core.is_enabled(option, side)
    }

    /// Hands the Core to `queue`, then writes what it queued. A failed write
    /// ends the connection.
    async fn send_with(
        &mut self,
        queue: impl FnOnce(&mut Core) -> Result<(), core::Error>,
    ) -> Result<(), Error> {
        if self.closed {
            return Err(Error::Closed);
        }
        queue(&mut self.core)?;
        let result = self.write_outgoing().await;
        if result.is_err() {
            self.closed = true;
        }
        result.map_err(Error::Io)
    }

    /// Writes what the Core has queued, then returns its next Event, reading
    /// from the stream until there is one.
    async fn drive(&mut self) -> io::Result<Event> {
        loop {
            self.write_outgoing().await?;
            if let Some(event) = self.core.poll_event() {
                return Ok(Event::Core(event));
            }
            let n = self.stream.read(&mut self.read_buf).await?;
            if n == 0 {
                return Ok(Event::Closed);
            }
            self.core.receive(&self.read_buf[..n]);
        }
    }

    /// Writes and flushes every byte the Core has queued.
    async fn write_outgoing(&mut self) -> io::Result<()> {
        self.core.poll_transmit(&mut self.outgoing);
        while self.written < self.outgoing.len() {
            let n = self.stream.write(&self.outgoing[self.written..]).await?;
            if n == 0 {
                return Err(io::ErrorKind::WriteZero.into());
            }
            self.written += n;
            self.unflushed = true;
        }
        self.outgoing.clear();
        self.written = 0;
        if self.unflushed {
            self.stream.flush().await?;
            self.unflushed = false;
        }
        Ok(())
    }
}
