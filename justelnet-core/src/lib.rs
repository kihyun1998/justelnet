//! Sans-IO Telnet protocol core.
//!
//! A [`Core`] holds one connection's Telnet protocol state. The caller feeds it
//! received bytes with [`Core::receive`], then takes [`Event`]s out with
//! [`Core::poll_event`] and the bytes to send with [`Core::poll_transmit`].
//! The Core performs no I/O and has no notion of time.

use std::collections::VecDeque;

/// Something the Core reports to its caller.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Event {
    /// Bytes received from the peer, with `IAC IAC` turned into a single 255.
    /// Other Telnet commands are not parsed: only the byte after IAC is dropped.
    Data(Vec<u8>),
}

const IAC: u8 = 255;

/// Where the byte parser stands between two received bytes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Parse {
    #[default]
    Data,
    /// The previous byte was IAC.
    Iac,
}

/// One connection's Telnet protocol state.
#[derive(Debug, Default)]
pub struct Core {
    parse: Parse,
    events: VecDeque<Event>,
    transmit: Vec<u8>,
}

impl Core {
    /// A Core for a new connection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds bytes received from the peer. Never fails.
    pub fn receive(&mut self, bytes: &[u8]) {
        let mut data = Vec::new();
        for &b in bytes {
            self.parse = match (self.parse, b) {
                (Parse::Data, IAC) => Parse::Iac,
                (Parse::Data, b) => {
                    data.push(b);
                    Parse::Data
                }
                (Parse::Iac, IAC) => {
                    data.push(IAC);
                    Parse::Data
                }
                // IAC followed by any other byte: the command byte is dropped.
                (Parse::Iac, _) => Parse::Data,
            };
        }
        if !data.is_empty() {
            self.events.push_back(Event::Data(data));
        }
    }

    /// Takes the next Event, if any.
    pub fn poll_event(&mut self) -> Option<Event> {
        self.events.pop_front()
    }

    /// Appends every queued outgoing byte to `buf`.
    pub fn poll_transmit(&mut self, buf: &mut Vec<u8>) {
        buf.append(&mut self.transmit);
    }
}
