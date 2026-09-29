//! Sans-IO Telnet protocol core.
//!
//! A [`Core`] holds one connection's Telnet protocol state. The caller feeds it
//! received bytes with [`Core::receive`], then takes [`Event`]s out with
//! [`Core::poll_event`] and the bytes to send with [`Core::poll_transmit`].
//! The Core performs no I/O and has no notion of time.
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/kihyun1998/justelnet/main/logo/icons/justelnet-icon-light-128.png"
)]
#![doc(
    html_favicon_url = "https://raw.githubusercontent.com/kihyun1998/justelnet/main/logo/favicon/favicon-32.png"
)]

use std::collections::VecDeque;

/// Something the Core reports to its caller.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Event {
    /// Bytes received from the peer, with `IAC IAC` turned into a single 255
    /// and option negotiation and subnegotiation removed. Other Telnet
    /// commands are dropped without an Event.
    Data(Vec<u8>),
    /// An option turned on or off on one side.
    OptionChanged {
        option: TelnetOption,
        side: Side,
        enabled: bool,
    },
    /// The peer sent something malformed; the Core recovered and went on.
    Warning(Warning),
}

/// A peer fault the Core recovered from.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Warning {
    /// `IAC byte` inside a subnegotiation, where `byte` is neither SE nor IAC.
    MalformedSubnegotiation { option: TelnetOption, byte: u8 },
    /// A subnegotiation longer than the Core keeps; the rest was discarded.
    SubnegotiationTruncated { option: TelnetOption },
    /// The peer answered our DONT with WILL, or our WONT with DO.
    NoncompliantAnswer { option: TelnetOption, side: Side },
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

/// How the Core treats one option on one side.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Stance {
    #[default]
    Refuse,
    Accept,
    /// Accepted, and asked for at the start of the connection.
    Request,
}

/// How the Core opens a connection.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Start {
    /// Ask for every requested option before any input.
    #[default]
    Active,
    /// Send nothing until the peer's negotiation first changes an option's
    /// state, then ask for every requested option still off.
    Passive,
}

/// The options the Core accepts when the peer asks for them, the ones it asks
/// for itself, how it opens the connection, and the values it answers
/// subnegotiations with: terminal types, window size, environment variables.
///
/// The default policy is a character-mode terminal client with an active start:
///
/// | Option | Local (WILL) | Remote (DO) |
/// |---|---|---|
/// | BINARY | accept | accept |
/// | ECHO | refuse | request |
/// | SGA | request | request |
/// | TTYPE, NAWS, NEW-ENVIRON | request | refuse |
///
/// Every other option is refused on both sides, TM included. The terminal
/// types are `["UNKNOWN"]`, the window size 80x24, and there are no
/// environment variables.
#[derive(Debug, Clone)]
pub struct OptionPolicy {
    stances: [[Stance; 256]; 2],
    /// Requested options, in the order they are asked for.
    requests: Vec<(TelnetOption, Side)>,
    start: Start,
    terminal_types: Vec<String>,
    window_size: (u16, u16),
    /// Environment variables for NEW-ENVIRON, in the order they are sent.
    variables: Vec<Variable>,
}

/// One NEW-ENVIRON variable of an [`OptionPolicy`].
#[derive(Debug, Clone, PartialEq, Eq)]
struct Variable {
    /// Well known (VAR) rather than user defined (USERVAR).
    well_known: bool,
    name: String,
    value: String,
}

impl Default for OptionPolicy {
    fn default() -> Self {
        OptionPolicyBuilder::empty()
            .request(TelnetOption::NAWS, Side::Local)
            .request(TelnetOption::TTYPE, Side::Local)
            .request(TelnetOption::NEW_ENVIRON, Side::Local)
            .request(TelnetOption::ECHO, Side::Remote)
            .request(TelnetOption::SGA, Side::Local)
            .request(TelnetOption::SGA, Side::Remote)
            .accept(TelnetOption::BINARY, Side::Local)
            .accept(TelnetOption::BINARY, Side::Remote)
            .build()
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
    pub fn accepts(&self, option: TelnetOption, side: Side) -> bool {
        self.stance(option, side) != Stance::Refuse
    }

    /// Whether the Core asks for `option` on `side` at the start.
    pub fn requests(&self, option: TelnetOption, side: Side) -> bool {
        self.stance(option, side) == Stance::Request
    }

    /// How the Core opens the connection.
    pub fn start(&self) -> Start {
        self.start
    }

    /// The terminal types TTYPE answers with, most specific first.
    pub fn terminal_types(&self) -> &[String] {
        &self.terminal_types
    }

    /// The window size NAWS starts with, as (width, height).
    pub fn window_size(&self) -> (u16, u16) {
        self.window_size
    }

    fn stance(&self, option: TelnetOption, side: Side) -> Stance {
        self.stances[side.index()][usize::from(option.0)]
    }
}

/// Builds an [`OptionPolicy`].
#[derive(Debug, Clone)]
pub struct OptionPolicyBuilder {
    policy: OptionPolicy,
}

impl OptionPolicyBuilder {
    fn empty() -> Self {
        Self {
            policy: OptionPolicy {
                stances: [[Stance::Refuse; 256]; 2],
                requests: Vec::new(),
                start: Start::Active,
                terminal_types: vec!["UNKNOWN".to_owned()],
                window_size: (80, 24),
                variables: Vec::new(),
            },
        }
    }

    /// Refuse the peer's request to enable `option` on `side`, and never ask for it.
    pub fn refuse(self, option: TelnetOption, side: Side) -> Self {
        self.set(option, side, Stance::Refuse)
    }

    /// Accept the peer's request to enable `option` on `side`, without asking for it.
    pub fn accept(self, option: TelnetOption, side: Side) -> Self {
        self.set(option, side, Stance::Accept)
    }

    /// Accept `option` on `side`, and ask for it at the start. Requests are
    /// sent in the order they were made here; an option refused or accepted
    /// in between moves to the end when requested again.
    pub fn request(self, option: TelnetOption, side: Side) -> Self {
        self.set(option, side, Stance::Request)
    }

    /// Refuse every option on both sides.
    pub fn refuse_all(mut self) -> Self {
        self.policy.stances = [[Stance::Refuse; 256]; 2];
        self.policy.requests.clear();
        self
    }

    /// Open the connection this way.
    pub fn start(mut self, start: Start) -> Self {
        self.policy.start = start;
        self
    }

    /// Start NAWS with this window size.
    pub fn window_size(mut self, width: u16, height: u16) -> Self {
        self.policy.window_size = (width, height);
        self
    }

    /// Send the well-known (VAR) environment variable `name` with `value`
    /// when NEW-ENVIRON asks for it, replacing any earlier value.
    pub fn variable(self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.set_variable(true, name.into(), value.into())
    }

    /// Send the user-defined (USERVAR) environment variable `name` with
    /// `value` when NEW-ENVIRON asks for it, replacing any earlier value.
    pub fn user_variable(self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.set_variable(false, name.into(), value.into())
    }

    fn set_variable(mut self, well_known: bool, name: String, value: String) -> Self {
        let variables = &mut self.policy.variables;
        match variables
            .iter_mut()
            .find(|v| v.well_known == well_known && v.name == name)
        {
            Some(v) => v.value = value,
            None => variables.push(Variable {
                well_known,
                name,
                value,
            }),
        }
        self
    }

    /// Answer TTYPE with these terminal types, most specific first
    /// (RFC 1091). An empty list answers `UNKNOWN`.
    pub fn terminal_types<I, T>(mut self, types: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<String>,
    {
        self.policy.terminal_types = types.into_iter().map(Into::into).collect();
        self
    }

    /// The finished policy.
    pub fn build(self) -> OptionPolicy {
        self.policy
    }

    fn set(mut self, option: TelnetOption, side: Side, stance: Stance) -> Self {
        self.policy.stances[side.index()][usize::from(option.0)] = stance;
        let requests = &mut self.policy.requests;
        let listed = requests.contains(&(option, side));
        if stance == Stance::Request && !listed {
            requests.push((option, side));
        } else if stance != Stance::Request && listed {
            requests.retain(|&r| r != (option, side));
        }
        self
    }
}

const IAC: u8 = 255;
const SE: u8 = 240;
const SB: u8 = 250;
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
    /// The previous bytes were IAC SB; the next is the option.
    Sb,
    /// Inside a subnegotiation for this option.
    SbData(TelnetOption),
    /// Inside a subnegotiation for this option, just after IAC.
    SbIac(TelnetOption),
}

/// The most subnegotiation bytes the Core keeps; the rest are discarded.
const SUBNEGOTIATION_LIMIT: usize = 4096;

/// TTYPE and NEW-ENVIRON: IS and SEND.
const IS: u8 = 0;
const SEND: u8 = 1;

/// NEW-ENVIRON: VAR, VALUE, ESC, USERVAR (RFC 1572).
const VAR: u8 = 0;
const VALUE: u8 = 1;
const ESC: u8 = 2;
const USERVAR: u8 = 3;

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
#[derive(Debug)]
pub struct Core {
    policy: OptionPolicy,
    options: Options,
    /// Whether the policy's requests have been sent; false only while a
    /// passive start waits for the peer.
    started: bool,
    parse: Parse,
    /// The body of the subnegotiation being received, up to the limit.
    subnegotiation: Vec<u8>,
    /// Whether the subnegotiation being received passed the limit.
    truncated: bool,
    /// How many TTYPE SENDs have been answered.
    ttype_answers: usize,
    /// The terminal type last sent in answer to TTYPE SEND.
    ttype_sent: Option<String>,
    /// The window size NAWS reports, as (width, height).
    window_size: (u16, u16),
    /// Received data not yet queued as a Data Event.
    data: Vec<u8>,
    events: VecDeque<Event>,
    transmit: Vec<u8>,
}

impl Default for Core {
    fn default() -> Self {
        Self::new()
    }
}

impl Core {
    /// A Core for a new connection, with the default [`OptionPolicy`].
    pub fn new() -> Self {
        Self::with_policy(OptionPolicy::default())
    }

    /// A Core for a new connection, answering the peer from `policy`.
    pub fn with_policy(policy: OptionPolicy) -> Self {
        let window_size = policy.window_size();
        let mut core = Self {
            policy,
            options: Options::default(),
            started: false,
            parse: Parse::default(),
            subnegotiation: Vec::new(),
            truncated: false,
            ttype_answers: 0,
            ttype_sent: None,
            window_size,
            data: Vec::new(),
            events: VecDeque::new(),
            transmit: Vec::new(),
        };
        if core.policy.start() == Start::Active {
            core.start();
        }
        core
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
                (Parse::Iac, b) => self.command(b),
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
                (Parse::Sb, code) => {
                    self.subnegotiation.clear();
                    self.truncated = false;
                    Parse::SbData(TelnetOption(code))
                }
                (Parse::SbData(option), IAC) => Parse::SbIac(option),
                (Parse::SbData(option), b) => {
                    self.subnegotiation_byte(b);
                    Parse::SbData(option)
                }
                (Parse::SbIac(option), SE) => {
                    self.subnegotiated(option);
                    Parse::Data
                }
                (Parse::SbIac(option), IAC) => {
                    self.subnegotiation_byte(IAC);
                    Parse::SbData(option)
                }
                (Parse::SbIac(option), byte @ 241..=254) => {
                    self.warn(Warning::MalformedSubnegotiation { option, byte });
                    self.subnegotiated(option);
                    self.command(byte)
                }
                (Parse::SbIac(option), byte) => {
                    self.warn(Warning::MalformedSubnegotiation { option, byte });
                    self.subnegotiation_byte(IAC);
                    self.subnegotiation_byte(byte);
                    Parse::SbData(option)
                }
            };
        }
        self.flush_data();
    }

    /// Asks the peer to enable `option` on `side`, whether or not the policy
    /// accepts it. Sends nothing if the option is on or already being asked for.
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

    /// Records a new window size, and sends it with NAWS if NAWS is on and
    /// the size changed.
    pub fn set_window_size(&mut self, width: u16, height: u16) {
        if self.window_size == (width, height) {
            return;
        }
        self.window_size = (width, height);
        if self.is_enabled(TelnetOption::NAWS, Side::Local) {
            self.send_naws();
        }
    }

    /// The terminal type the Core last sent in answer to TTYPE SEND.
    pub fn terminal_type_sent(&self) -> Option<&str> {
        self.ttype_sent.as_deref()
    }

    /// Whether `option` is currently on for `side`.
    pub fn is_enabled(&self, option: TelnetOption, side: Side) -> bool {
        self.options.0[side.index()][usize::from(option.0)].q == Q::Yes
    }

    /// Handles the peer's WILL/DO (`enable`) or WONT/DONT for `option` on
    /// `side`, by RFC 1143's receive tables.
    fn received(&mut self, option: TelnetOption, side: Side, enable: bool) {
        let st = *self.state(option, side);
        if enable && st.q == Q::WantNo {
            self.warn(Warning::NoncompliantAnswer { option, side });
        }
        let (q, send, changed) = match (enable, st.q, st.opposite) {
            (true, Q::No, _) if self.policy.accepts(option, side) => {
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
        if changed == Some(true) && option == TelnetOption::NAWS && side == Side::Local {
            self.send_naws();
        }
        if let Some(enabled) = changed {
            self.changed(option, side, enabled);
            if !self.started {
                self.start();
            }
        }
    }

    /// Asks for every option the policy requests that is still off and not
    /// being negotiated, in the policy's order.
    fn start(&mut self) {
        self.started = true;
        for (option, side) in self.policy.requests.clone() {
            if self.state(option, side).q == Q::No {
                self.request_enable(option, side);
            }
        }
    }

    /// Handles the byte after IAC outside a subnegotiation.
    fn command(&mut self, b: u8) -> Parse {
        match b {
            IAC => {
                self.data.push(IAC);
                Parse::Data
            }
            WILL..=DONT => Parse::Verb(b),
            SB => Parse::Sb,
            // Any other command: the command byte is dropped.
            _ => Parse::Data,
        }
    }

    fn subnegotiation_byte(&mut self, b: u8) {
        if self.subnegotiation.len() < SUBNEGOTIATION_LIMIT {
            self.subnegotiation.push(b);
        } else {
            self.truncated = true;
        }
    }

    /// Handles a complete subnegotiation for `option`. TTYPE SEND and
    /// NEW-ENVIRON SEND are answered while the option is on on our side;
    /// every other subnegotiation is discarded.
    fn subnegotiated(&mut self, option: TelnetOption) {
        if self.truncated {
            self.warn(Warning::SubnegotiationTruncated { option });
        }
        if !self.is_enabled(option, Side::Local) || self.subnegotiation.first() != Some(&SEND) {
            return;
        }
        if option == TelnetOption::TTYPE && self.subnegotiation.len() == 1 {
            self.answer_ttype();
        } else if option == TelnetOption::NEW_ENVIRON {
            let request = std::mem::take(&mut self.subnegotiation);
            self.answer_environ(&request[1..]);
            self.subnegotiation = request;
        }
    }

    /// Sends NEW-ENVIRON IS for a SEND whose list is `request` (RFC 1572):
    /// with no list, every VAR then every USERVAR; otherwise each entry in
    /// order, a type with no name standing for every variable of that type,
    /// and a variable the policy lacks sent as its name alone.
    fn answer_environ(&mut self, request: &[u8]) {
        let mut entries: Vec<(bool, Vec<u8>)> = Vec::new();
        let mut escaped = false;
        for &b in request {
            match (escaped, entries.last_mut()) {
                (false, _) if b == VAR || b == USERVAR => entries.push((b == VAR, Vec::new())),
                (false, _) if b == ESC => escaped = true,
                (_, Some((_, name))) => {
                    name.push(b);
                    escaped = false;
                }
                (_, None) => escaped = false,
            }
        }
        if request.is_empty() {
            entries = vec![(true, Vec::new()), (false, Vec::new())];
        }
        let mut body = vec![IS];
        for (well_known, name) in entries {
            let kind = if well_known { VAR } else { USERVAR };
            let mut found = false;
            for v in &self.policy.variables {
                if v.well_known == well_known && (name.is_empty() || name == v.name.as_bytes()) {
                    body.push(kind);
                    push_escaped(&mut body, v.name.as_bytes());
                    body.push(VALUE);
                    push_escaped(&mut body, v.value.as_bytes());
                    found = true;
                }
            }
            if !found && !name.is_empty() {
                body.push(kind);
                push_escaped(&mut body, &name);
            }
        }
        self.send_subnegotiation(TelnetOption::NEW_ENVIRON, &body);
    }

    /// Sends the window size with NAWS (RFC 1073).
    fn send_naws(&mut self) {
        let (width, height) = self.window_size;
        let [w1, w0] = width.to_be_bytes();
        let [h1, h0] = height.to_be_bytes();
        self.send_subnegotiation(TelnetOption::NAWS, &[w1, w0, h1, h0]);
    }

    /// Queues `IAC SB option body IAC SE`, doubling every 255 in `body`.
    fn send_subnegotiation(&mut self, option: TelnetOption, body: &[u8]) {
        self.transmit.extend([IAC, SB, option.0]);
        for &b in body {
            if b == IAC {
                self.transmit.push(IAC);
            }
            self.transmit.push(b);
        }
        self.transmit.extend([IAC, SE]);
    }

    /// Sends TTYPE IS with the next name in RFC 1091's cycle: each name in
    /// turn, the last one again, then from the top.
    fn answer_ttype(&mut self) {
        let types = self.policy.terminal_types();
        let name = if types.is_empty() {
            "UNKNOWN".to_owned()
        } else {
            let at = self.ttype_answers % (types.len() + 1);
            types[at.min(types.len() - 1)].clone()
        };
        self.ttype_answers += 1;
        let mut body = vec![IS];
        body.extend(name.as_bytes());
        self.send_subnegotiation(TelnetOption::TTYPE, &body);
        self.ttype_sent = Some(name);
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

    /// Queues a Warning Event after any data received before it.
    fn warn(&mut self, warning: Warning) {
        self.flush_data();
        self.events.push_back(Event::Warning(warning));
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

/// Appends `bytes` to a NEW-ENVIRON body, escaping VAR, VALUE, ESC and
/// USERVAR with ESC.
fn push_escaped(body: &mut Vec<u8>, bytes: &[u8]) {
    for &b in bytes {
        if matches!(b, VAR | VALUE | ESC | USERVAR) {
            body.push(ESC);
        }
        body.push(b);
    }
}
