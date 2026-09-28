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
    /// Bytes received from the peer, with `IAC IAC` turned into a single 255
    /// and option negotiation removed. Other Telnet commands are not parsed:
    /// only the byte after IAC is dropped.
    Data(Vec<u8>),
    /// An option turned on or off on one side.
    OptionChanged {
        option: TelnetOption,
        side: Side,
        enabled: bool,
    },
}

/// A Telnet option code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TelnetOption(u8);

impl TelnetOption {
    /// BINARY, RFC 856.
    pub const BINARY: Self = Self(0);
    /// ECHO, RFC 857.
    pub const ECHO: Self = Self(1);
    /// SUPPRESS-GO-AHEAD, RFC 858.
    pub const SGA: Self = Self(3);
    /// STATUS, RFC 859.
    pub const STATUS: Self = Self(5);
    /// TIMING-MARK, RFC 860.
    pub const TM: Self = Self(6);
    /// TERMINAL-TYPE, RFC 1091.
    pub const TTYPE: Self = Self(24);
    /// NAWS, RFC 1073.
    pub const NAWS: Self = Self(31);
    /// TERMINAL-SPEED, RFC 1079.
    pub const TSPEED: Self = Self(32);
    /// TOGGLE-FLOW-CONTROL, RFC 1372.
    pub const LFLOW: Self = Self(33);
    /// LINEMODE, RFC 1184.
    pub const LINEMODE: Self = Self(34);
    /// X-DISPLAY-LOCATION, RFC 1096.
    pub const XDISPLOC: Self = Self(35);
    /// OLD-ENVIRON, RFC 1408.
    pub const OLD_ENVIRON: Self = Self(36);
    /// NEW-ENVIRON, RFC 1572.
    pub const NEW_ENVIRON: Self = Self(39);
    /// COM-PORT-OPTION, RFC 2217.
    pub const COM_PORT: Self = Self(44);

    /// The option with this code.
    pub const fn new(code: u8) -> Self {
        Self(code)
    }

    /// This option's code.
    pub const fn code(self) -> u8 {
        self.0
    }
}

/// Which end of the connection an option is enabled on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Side {
    /// This end: the option we perform, negotiated with WILL/WONT from us.
    Local,
    /// The peer: the option it performs, negotiated with DO/DONT from us.
    Remote,
}

impl Side {
    fn index(self) -> usize {
        match self {
            Side::Local => 0,
            Side::Remote => 1,
        }
    }
}

/// The options the Core accepts when the peer asks for them.
///
/// The default policy supports no option, so every request is refused.
#[derive(Debug, Clone)]
pub struct OptionPolicy {
    supported: [[bool; 256]; 2],
}

impl Default for OptionPolicy {
    fn default() -> Self {
        Self {
            supported: [[false; 256]; 2],
        }
    }
}

impl OptionPolicy {
    /// A builder starting from the default policy.
    pub fn builder() -> OptionPolicyBuilder {
        OptionPolicyBuilder {
            policy: Self::default(),
        }
    }

    /// Whether the Core accepts the peer's request to enable `option` on `side`.
    pub fn supports(&self, option: TelnetOption, side: Side) -> bool {
        self.supported[side.index()][usize::from(option.0)]
    }
}

/// Builds an [`OptionPolicy`].
#[derive(Debug, Clone)]
pub struct OptionPolicyBuilder {
    policy: OptionPolicy,
}

impl OptionPolicyBuilder {
    /// Accept the peer's request to enable `option` on `side`.
    pub fn support(mut self, option: TelnetOption, side: Side) -> Self {
        self.policy.supported[side.index()][usize::from(option.0)] = true;
        self
    }

    /// The finished policy.
    pub fn build(self) -> OptionPolicy {
        self.policy
    }
}

const IAC: u8 = 255;
const WILL: u8 = 251;
const WONT: u8 = 252;
const DO: u8 = 253;
const DONT: u8 = 254;

/// Where the byte parser stands between two received bytes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Parse {
    #[default]
    Data,
    /// The previous byte was IAC.
    Iac,
    /// The previous bytes were IAC and this WILL, WONT, DO or DONT.
    Verb(u8),
}

/// An option's negotiation state on one side, RFC 1143's `us`/`him`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Q {
    #[default]
    No,
    Yes,
    WantNo,
    WantYes,
}

/// An option's state on one side: RFC 1143's state plus its one-entry queue,
/// where `opposite` is the queue's OPPOSITE.
#[derive(Debug, Default, Clone, Copy)]
struct OptionState {
    q: Q,
    opposite: bool,
}

/// Every option's state on both sides, indexed by [`Side::index`] then code.
#[derive(Debug, Clone)]
struct Options([[OptionState; 256]; 2]);

impl Default for Options {
    fn default() -> Self {
        Self([[OptionState::default(); 256]; 2])
    }
}

/// One connection's Telnet protocol state.
#[derive(Debug, Default)]
pub struct Core {
    policy: OptionPolicy,
    options: Options,
    parse: Parse,
    /// Received data not yet queued as a Data Event.
    data: Vec<u8>,
    events: VecDeque<Event>,
    transmit: Vec<u8>,
}

impl Core {
    /// A Core for a new connection, with the default [`OptionPolicy`].
    pub fn new() -> Self {
        Self::default()
    }

    /// A Core for a new connection, answering the peer from `policy`.
    pub fn with_policy(policy: OptionPolicy) -> Self {
        Self {
            policy,
            ..Self::default()
        }
    }

    /// Feeds bytes received from the peer. Never fails.
    pub fn receive(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.parse = match (self.parse, b) {
                (Parse::Data, IAC) => Parse::Iac,
                (Parse::Data, b) => {
                    self.data.push(b);
                    Parse::Data
                }
                (Parse::Iac, IAC) => {
                    self.data.push(IAC);
                    Parse::Data
                }
                (Parse::Iac, verb @ WILL..=DONT) => Parse::Verb(verb),
                // IAC followed by any other byte: the command byte is dropped.
                (Parse::Iac, _) => Parse::Data,
                (Parse::Verb(verb), code) => {
                    let option = TelnetOption(code);
                    match verb {
                        WILL => self.received(option, Side::Remote, true),
                        WONT => self.received(option, Side::Remote, false),
                        DO => self.received(option, Side::Local, true),
                        _ => self.received(option, Side::Local, false),
                    }
                    Parse::Data
                }
            };
        }
        self.flush_data();
    }

    /// Asks the peer to enable `option` on `side`, whether or not the policy
    /// supports it. Sends nothing if the option is on or already being asked for.
    pub fn request_enable(&mut self, option: TelnetOption, side: Side) {
        let st = *self.state(option, side);
        match (st.q, st.opposite) {
            (Q::No, _) => {
                self.state(option, side).q = Q::WantYes;
                self.send(option, side, true);
            }
            (Q::WantNo, false) => self.state(option, side).opposite = true,
            (Q::WantYes, true) => self.state(option, side).opposite = false,
            (Q::Yes, _) | (Q::WantNo, true) | (Q::WantYes, false) => {}
        }
    }

    /// Asks the peer to disable `option` on `side`. The option counts as off
    /// from this call on. Sends nothing if the option is off or already being
    /// asked off.
    pub fn request_disable(&mut self, option: TelnetOption, side: Side) {
        let st = *self.state(option, side);
        match (st.q, st.opposite) {
            (Q::Yes, _) => {
                self.state(option, side).q = Q::WantNo;
                self.send(option, side, false);
                self.changed(option, side, false);
            }
            (Q::WantNo, true) => self.state(option, side).opposite = false,
            (Q::WantYes, false) => self.state(option, side).opposite = true,
            (Q::No, _) | (Q::WantNo, false) | (Q::WantYes, true) => {}
        }
    }

    /// Whether `option` is currently on for `side`.
    pub fn is_enabled(&self, option: TelnetOption, side: Side) -> bool {
        self.options.0[side.index()][usize::from(option.0)].q == Q::Yes
    }

    /// Handles the peer's WILL/DO (`enable`) or WONT/DONT for `option` on
    /// `side`, by RFC 1143's receive tables.
    fn received(&mut self, option: TelnetOption, side: Side, enable: bool) {
        let st = *self.state(option, side);
        let (q, send, changed) = match (enable, st.q, st.opposite) {
            (true, Q::No, _) if self.policy.supports(option, side) => {
                (Q::Yes, Some(true), Some(true))
            }
            (true, Q::No, _) => (Q::No, Some(false), None),
            (true, Q::Yes, _) => (Q::Yes, None, None),
            (true, Q::WantNo, false) => (Q::No, None, None),
            (true, Q::WantNo, true) => (Q::Yes, None, Some(true)),
            (true, Q::WantYes, false) => (Q::Yes, None, Some(true)),
            (true, Q::WantYes, true) => (Q::WantNo, Some(false), None),
            (false, Q::No, _) => (Q::No, None, None),
            (false, Q::Yes, _) => (Q::No, Some(false), Some(false)),
            (false, Q::WantNo, false) => (Q::No, None, None),
            (false, Q::WantNo, true) => (Q::WantYes, Some(true), None),
            (false, Q::WantYes, _) => (Q::No, None, None),
        };
        *self.state(option, side) = OptionState { q, opposite: false };
        if let Some(enable) = send {
            self.send(option, side, enable);
        }
        if let Some(enabled) = changed {
            self.changed(option, side, enabled);
        }
    }

    fn state(&mut self, option: TelnetOption, side: Side) -> &mut OptionState {
        &mut self.options.0[side.index()][usize::from(option.0)]
    }

    /// Queues WILL/WONT (our side) or DO/DONT (the peer's side) for `option`.
    fn send(&mut self, option: TelnetOption, side: Side, enable: bool) {
        let verb = match (side, enable) {
            (Side::Local, true) => WILL,
            (Side::Local, false) => WONT,
            (Side::Remote, true) => DO,
            (Side::Remote, false) => DONT,
        };
        self.transmit.extend([IAC, verb, option.0]);
    }

    /// Queues an OptionChanged Event after any data received before it.
    fn changed(&mut self, option: TelnetOption, side: Side, enabled: bool) {
        self.flush_data();
        self.events.push_back(Event::OptionChanged {
            option,
            side,
            enabled,
        });
    }

    fn flush_data(&mut self) {
        if !self.data.is_empty() {
            self.events
                .push_back(Event::Data(std::mem::take(&mut self.data)));
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
