//! Transcript parsing and replay against the Core's public API.
//!
//! The format this reads is docs/transcripts.md.

#![allow(dead_code)]

use justelnet_core::{
    Command, Core, EndOfLine, Error, Event, OptionPolicy, Side, Start, TelnetOption, Warning,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// A runtime request the caller makes at the start of a step.
#[derive(Debug, Clone)]
pub enum Call {
    Enable(Side, TelnetOption),
    Disable(Side, TelnetOption),
    WindowSize(u16, u16),
    Data(Vec<u8>),
    Raw(Vec<u8>),
    Command(Command),
    Subnegotiation(TelnetOption, Vec<u8>),
}

#[derive(Debug, Clone, Default)]
pub struct Step {
    pub call: Option<Call>,
    pub server: Vec<u8>,
    pub client: Vec<u8>,
    pub events: Vec<Event>,
    pub terminal_type_sent: Option<Option<String>>,
    pub error: Option<Error>,
}

#[derive(Debug, Clone)]
pub struct Transcript {
    pub name: String,
    pub policy: OptionPolicy,
    pub steps: Vec<Step>,
}

impl Transcript {
    /// A replay starting from a Core built from this Transcript's Option policy.
    pub fn core(&self) -> Replay {
        Replay {
            core: Core::with_policy(self.policy.clone()),
            enabled: HashSet::new(),
        }
    }
}

/// A Core under replay, with the options its OptionChanged Events so far say
/// are on.
pub struct Replay {
    core: Core,
    enabled: HashSet<(TelnetOption, Side)>,
}

/// What one step made the Core do. Prints as the `client:` and `event:` lines
/// of a Transcript, one per line, or `# nothing`.
#[derive(PartialEq, Eq)]
pub struct Outcome {
    pub client: Vec<u8>,
    pub events: Vec<Event>,
}

impl std::fmt::Debug for Outcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.client.is_empty() && self.events.is_empty() {
            return write!(f, "# nothing");
        }
        if !self.client.is_empty() {
            write!(f, "\nclient: {}", hex_text(&self.client))?;
        }
        for e in &self.events {
            write!(f, "\nevent: {}", event_text(e))?;
        }
        Ok(())
    }
}

fn hex_text(bytes: &[u8]) -> String {
    let words: Vec<String> = bytes.iter().map(|b| format!("{b:02x}")).collect();
    words.join(" ")
}

fn quoted_text(bytes: &[u8]) -> String {
    let mut out = String::from("\"");
    for &b in bytes {
        match b {
            b'\r' => out.push_str("\\r"),
            b'\n' => out.push_str("\\n"),
            b'\\' => out.push_str("\\\\"),
            b'"' => out.push_str("\\\""),
            b' '..=b'~' => out.push(b as char),
            _ => out.push_str(&format!("\\x{b:02x}")),
        }
    }
    out.push('"');
    out
}

fn side_text(side: Side) -> String {
    match side {
        Side::Local => "local".to_owned(),
        Side::Remote => "remote".to_owned(),
        other => format!("{other:?}"),
    }
}

fn option_text(option: TelnetOption) -> String {
    let name = match option {
        TelnetOption::BINARY => "BINARY",
        TelnetOption::ECHO => "ECHO",
        TelnetOption::SGA => "SGA",
        TelnetOption::STATUS => "STATUS",
        TelnetOption::TM => "TM",
        TelnetOption::TTYPE => "TTYPE",
        TelnetOption::NAWS => "NAWS",
        TelnetOption::TSPEED => "TSPEED",
        TelnetOption::LFLOW => "LFLOW",
        TelnetOption::LINEMODE => "LINEMODE",
        TelnetOption::XDISPLOC => "XDISPLOC",
        TelnetOption::OLD_ENVIRON => "OLD-ENVIRON",
        TelnetOption::NEW_ENVIRON => "NEW-ENVIRON",
        TelnetOption::COM_PORT => "COM-PORT",
        other => return format!("{:02x}", other.code()),
    };
    name.to_owned()
}

fn command_text(command: Command) -> String {
    let name = match command {
        Command::NoOperation => "NOP",
        Command::DataMark => "DM",
        Command::Break => "BRK",
        Command::InterruptProcess => "IP",
        Command::AbortOutput => "AO",
        Command::AreYouThere => "AYT",
        Command::EraseCharacter => "EC",
        Command::EraseLine => "EL",
        Command::GoAhead => "GA",
        other => return format!("{other:?}"),
    };
    name.to_owned()
}

/// An Event as the text after `event: `; one the format has no line for
/// prints as its Debug.
fn event_text(e: &Event) -> String {
    match e {
        Event::Data(data) => format!("data {}", quoted_text(data)),
        Event::OptionChanged {
            option,
            side,
            enabled,
        } => format!(
            "option {} {} {}",
            side_text(*side),
            option_text(*option),
            if *enabled { "on" } else { "off" }
        ),
        Event::Command(command) => format!("command {}", command_text(*command)),
        Event::Subnegotiation { option, data } => {
            format!("subnegotiation {} {}", option_text(*option), hex_text(data))
        }
        Event::Warning(Warning::MalformedSubnegotiation { option, byte }) => {
            format!("warning malformed {} {byte:02x}", option_text(*option))
        }
        Event::Warning(Warning::SubnegotiationTruncated { option }) => {
            format!("warning truncated {}", option_text(*option))
        }
        Event::Warning(Warning::NoncompliantAnswer { option, side }) => format!(
            "warning noncompliant {} {}",
            side_text(*side),
            option_text(*option)
        ),
        Event::Warning(Warning::UnknownCommand { byte }) => {
            format!("warning unknown-command {byte:02x}")
        }
        other => format!("{other:?}"),
    }
}

pub fn transcripts_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("transcripts")
}

pub fn load_all() -> Vec<Transcript> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(transcripts_dir())
        .expect("transcripts directory")
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no Transcripts found");
    paths.iter().map(|p| load(p)).collect()
}

pub fn load(path: &Path) -> Transcript {
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    let text = std::fs::read_to_string(path).expect("readable Transcript");
    parse(&name, &text)
}

pub fn parse(name: &str, text: &str) -> Transcript {
    let mut policy = OptionPolicy::builder();
    let mut steps = vec![Step::default()];
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let at = || format!("{name}:{}", n + 1);
        let (kind, rest) = line
            .split_once(':')
            .unwrap_or_else(|| panic!("{}: expected `kind: value`", at()));
        let rest = rest.trim();
        match kind.trim() {
            "passthrough" => {
                assert!(
                    steps.len() == 1,
                    "{}: `passthrough:` after the first step",
                    at()
                );
                policy = policy.passthrough(option_named(rest, &at()));
            }
            "error" => {
                let error = match rest.split_once(' ') {
                    Some(("not-enabled", option)) => Error::OptionNotEnabled {
                        option: option_named(option.trim(), &at()),
                    },
                    Some(("not-passthrough", option)) => Error::NotPassthrough {
                        option: option_named(option.trim(), &at()),
                    },
                    _ => panic!(
                        "{}: expected `error: not-enabled|not-passthrough <option>`",
                        at()
                    ),
                };
                let step = steps.last_mut().unwrap();
                assert!(
                    matches!(step.call, Some(Call::Subnegotiation(..))),
                    "{}: `error:` only follows `call: subnegotiation`",
                    at()
                );
                step.error = Some(error);
            }
            "end-of-line" => {
                assert!(
                    steps.len() == 1,
                    "{}: `end-of-line:` after the first step",
                    at()
                );
                policy = policy.end_of_line(match rest {
                    "crlf" => EndOfLine::CrLf,
                    "crnul" => EndOfLine::CrNul,
                    "lf" => EndOfLine::Lf,
                    other => panic!("{}: unknown end of line `{other}`", at()),
                });
            }
            "window-size" => {
                assert!(
                    steps.len() == 1,
                    "{}: `window-size:` after the first step",
                    at()
                );
                let (w, h) = window(rest, &at());
                policy = policy.window_size(w, h);
            }
            kind @ ("variable" | "user-variable") => {
                assert!(steps.len() == 1, "{}: `{kind}:` after the first step", at());
                let (name, value) = rest
                    .split_once(' ')
                    .unwrap_or_else(|| panic!("{}: expected `{kind}: <name> \"<value>\"`", at()));
                let name = String::from_utf8(quoted(name, &at())).expect("UTF-8 name");
                let value = String::from_utf8(quoted(value.trim(), &at())).expect("UTF-8 value");
                policy = if kind == "variable" {
                    policy.variable(name, value)
                } else {
                    policy.user_variable(name, value)
                };
            }
            "terminal-types" => {
                assert!(
                    steps.len() == 1,
                    "{}: `terminal-types:` after the first step",
                    at()
                );
                policy = policy.terminal_types(rest.split_whitespace());
            }
            "terminal-type-sent" => {
                steps.last_mut().unwrap().terminal_type_sent =
                    Some((rest != "none").then(|| rest.to_owned()));
            }
            kind @ ("policy" | "accept" | "request" | "refuse") => {
                assert!(steps.len() == 1, "{}: `{kind}:` after the first step", at());
                policy = match (kind, rest) {
                    ("policy", "empty") => policy.refuse_all().start(Start::Passive),
                    ("policy", "passive") => policy.start(Start::Passive),
                    ("policy", other) => panic!("{}: unknown policy `{other}`", at()),
                    ("accept", target) => {
                        let (side, option) = side_option(target, &at());
                        policy.accept(option, side)
                    }
                    ("request", target) => {
                        let (side, option) = side_option(target, &at());
                        policy.request(option, side)
                    }
                    (_, target) => {
                        let (side, option) = side_option(target, &at());
                        policy.refuse(option, side)
                    }
                };
            }
            "call" => {
                let (verb, target) = rest.split_once(' ').unwrap_or_else(|| {
                    panic!(
                        "{}: expected `enable|disable <side> <option>` or `window <w> <h>`",
                        at()
                    )
                });
                let call = match verb {
                    "enable" => {
                        let (side, option) = side_option(target, &at());
                        Call::Enable(side, option)
                    }
                    "disable" => {
                        let (side, option) = side_option(target, &at());
                        Call::Disable(side, option)
                    }
                    "window" => {
                        let (w, h) = window(target, &at());
                        Call::WindowSize(w, h)
                    }
                    "data" => Call::Data(quoted(target.trim(), &at())),
                    "raw" => Call::Raw(quoted(target.trim(), &at())),
                    "command" => Call::Command(command_named(target.trim(), &at())),
                    "subnegotiation" => {
                        let (option, bytes) =
                            target.trim().split_once(' ').unwrap_or((target.trim(), ""));
                        Call::Subnegotiation(option_named(option, &at()), hex(bytes, &at()))
                    }
                    other => panic!("{}: unknown call `{other}`", at()),
                };
                steps.push(Step {
                    call: Some(call),
                    ..Step::default()
                });
            }
            "server" => steps.push(Step {
                server: hex(rest, &at()),
                ..Step::default()
            }),
            "client" => steps.last_mut().unwrap().client.extend(hex(rest, &at())),
            "event" => steps.last_mut().unwrap().events.push(event(rest, &at())),
            other => panic!("{}: unknown line kind `{other}`", at()),
        }
    }
    if steps[0].client.is_empty() && steps[0].events.is_empty() {
        steps.remove(0);
    }
    Transcript {
        name: name.to_owned(),
        policy: policy.build(),
        steps,
    }
}

fn hex(s: &str, at: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for word in s.split_whitespace() {
        let (b, n) = word.split_once('*').unwrap_or((word, "1"));
        let b = u8::from_str_radix(b, 16).unwrap_or_else(|_| panic!("{at}: bad hex byte `{b}`"));
        let n: usize = n
            .parse()
            .unwrap_or_else(|_| panic!("{at}: bad repeat `{word}`"));
        out.extend(std::iter::repeat_n(b, n));
    }
    out
}

fn command_named(s: &str, at: &str) -> Command {
    match s {
        "NOP" => Command::NoOperation,
        "DM" => Command::DataMark,
        "BRK" => Command::Break,
        "IP" => Command::InterruptProcess,
        "AO" => Command::AbortOutput,
        "AYT" => Command::AreYouThere,
        "EC" => Command::EraseCharacter,
        "EL" => Command::EraseLine,
        "GA" => Command::GoAhead,
        other => panic!("{at}: unknown command `{other}`"),
    }
}

fn window(s: &str, at: &str) -> (u16, u16) {
    let mut n = s.split_whitespace().map(|n| {
        n.parse::<u16>()
            .unwrap_or_else(|_| panic!("{at}: bad window dimension `{n}`"))
    });
    match (n.next(), n.next(), n.next()) {
        (Some(w), Some(h), None) => (w, h),
        _ => panic!("{at}: expected `<width> <height>`"),
    }
}

fn side_option(s: &str, at: &str) -> (Side, TelnetOption) {
    let (side, option) = s
        .trim()
        .split_once(' ')
        .unwrap_or_else(|| panic!("{at}: expected `<side> <option>`"));
    let side = match side {
        "local" => Side::Local,
        "remote" => Side::Remote,
        other => panic!("{at}: unknown side `{other}`"),
    };
    (side, option_named(option.trim(), at))
}

fn option_named(s: &str, at: &str) -> TelnetOption {
    match s {
        "BINARY" => TelnetOption::BINARY,
        "ECHO" => TelnetOption::ECHO,
        "SGA" => TelnetOption::SGA,
        "STATUS" => TelnetOption::STATUS,
        "TM" => TelnetOption::TM,
        "TTYPE" => TelnetOption::TTYPE,
        "NAWS" => TelnetOption::NAWS,
        "TSPEED" => TelnetOption::TSPEED,
        "LFLOW" => TelnetOption::LFLOW,
        "LINEMODE" => TelnetOption::LINEMODE,
        "XDISPLOC" => TelnetOption::XDISPLOC,
        "OLD-ENVIRON" => TelnetOption::OLD_ENVIRON,
        "NEW-ENVIRON" => TelnetOption::NEW_ENVIRON,
        "COM-PORT" => TelnetOption::COM_PORT,
        hex => TelnetOption::new(
            u8::from_str_radix(hex, 16).unwrap_or_else(|_| panic!("{at}: unknown option `{hex}`")),
        ),
    }
}

fn event(s: &str, at: &str) -> Event {
    let (name, arg) = s.split_once(' ').unwrap_or((s, ""));
    match name {
        "data" => Event::Data(quoted(arg.trim(), at)),
        "option" => {
            let (target, state) = arg
                .trim()
                .rsplit_once(' ')
                .unwrap_or_else(|| panic!("{at}: expected `option <side> <option> on|off`"));
            let (side, option) = side_option(target, at);
            let enabled = match state {
                "on" => true,
                "off" => false,
                other => panic!("{at}: expected `on` or `off`, got `{other}`"),
            };
            Event::OptionChanged {
                option,
                side,
                enabled,
            }
        }
        "command" => Event::Command(command_named(arg.trim(), at)),
        "subnegotiation" => {
            let (option, bytes) = arg.trim().split_once(' ').unwrap_or((arg.trim(), ""));
            Event::Subnegotiation {
                option: option_named(option, at),
                data: hex(bytes, at),
            }
        }
        "warning" => {
            let words: Vec<&str> = arg.split_whitespace().collect();
            match words.as_slice() {
                ["malformed", option, byte] => Event::Warning(Warning::MalformedSubnegotiation {
                    option: option_named(option, at),
                    byte: u8::from_str_radix(byte, 16)
                        .unwrap_or_else(|_| panic!("{at}: bad hex byte `{byte}`")),
                }),
                ["unknown-command", byte] => Event::Warning(Warning::UnknownCommand {
                    byte: u8::from_str_radix(byte, 16)
                        .unwrap_or_else(|_| panic!("{at}: bad hex byte `{byte}`")),
                }),
                ["truncated", option] => Event::Warning(Warning::SubnegotiationTruncated {
                    option: option_named(option, at),
                }),
                ["noncompliant", side, option] => {
                    let (side, option) = side_option(&format!("{side} {option}"), at);
                    Event::Warning(Warning::NoncompliantAnswer { option, side })
                }
                _ => panic!(
                    "{at}: expected `warning malformed <option> <byte>` or `warning truncated <option>`"
                ),
            }
        }
        other => panic!("{at}: unknown event `{other}`"),
    }
}

fn quoted(s: &str, at: &str) -> Vec<u8> {
    let inner = s
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or_else(|| panic!("{at}: expected a quoted string"));
    let mut out = Vec::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            let mut buf = [0; 4];
            out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            continue;
        }
        match chars.next() {
            Some('r') => out.push(b'\r'),
            Some('n') => out.push(b'\n'),
            Some('\\') => out.push(b'\\'),
            Some('"') => out.push(b'"'),
            Some('x') => {
                let h: String = chars.by_ref().take(2).collect();
                out.push(
                    u8::from_str_radix(&h, 16).unwrap_or_else(|_| panic!("{at}: bad \\x escape")),
                );
            }
            other => panic!("{at}: bad escape `\\{other:?}`"),
        }
    }
    out
}

/// Merges adjacent Data Events, so chunking never changes the comparison.
pub fn coalesce(events: Vec<Event>) -> Vec<Event> {
    let mut out: Vec<Event> = Vec::new();
    for e in events {
        match (out.last_mut(), e) {
            (Some(Event::Data(prev)), Event::Data(next)) => prev.extend(next),
            (_, e) => out.push(e),
        }
    }
    out
}

/// Makes the step's call, then feeds its server bytes in the given chunks, and
/// collects what comes out.
pub fn run_step<'a>(
    replay: &mut Replay,
    step: &Step,
    chunks: impl IntoIterator<Item = &'a [u8]>,
) -> Outcome {
    let mut client = Vec::new();
    let mut events = Vec::new();
    replay.core.poll_transmit(&mut client);
    if let Some(call) = &step.call {
        match call {
            Call::Enable(side, option) => replay.core.request_enable(*option, *side),
            Call::Disable(side, option) => replay.core.request_disable(*option, *side),
            Call::WindowSize(w, h) => replay.core.set_window_size(*w, *h),
            Call::Data(data) => replay.core.send_data(data),
            Call::Raw(data) => replay.core.send_raw(data),
            Call::Command(command) => replay.core.send_command(*command),
            Call::Subnegotiation(option, data) => {
                let result = replay.core.send_subnegotiation(*option, data);
                assert_eq!(
                    result.err(),
                    step.error,
                    "the result of send_subnegotiation"
                );
            }
        }
        replay.drain(&mut events);
        replay.check_query();
        replay.core.poll_transmit(&mut client);
    }
    for chunk in chunks {
        replay.core.receive(chunk);
        replay.drain(&mut events);
        replay.core.poll_transmit(&mut client);
    }
    replay.check_query();
    if let Some(name) = &step.terminal_type_sent {
        assert_eq!(
            replay.core.terminal_type_sent(),
            name.as_deref(),
            "the terminal type the Core reports having sent"
        );
    }
    Outcome {
        client,
        events: coalesce(events),
    }
}

impl Replay {
    /// Takes every queued Event, tracking which options they say are on.
    fn drain(&mut self, events: &mut Vec<Event>) {
        while let Some(e) = self.core.poll_event() {
            assert!(
                e != Event::Data(Vec::new()),
                "the Core emitted an empty Data Event"
            );
            if let Event::OptionChanged {
                option,
                side,
                enabled,
            } = e
            {
                let flipped = if enabled {
                    self.enabled.insert((option, side))
                } else {
                    self.enabled.remove(&(option, side))
                };
                assert!(
                    flipped,
                    "OptionChanged for {option:?} {side:?} to {enabled} without a change"
                );
            }
            events.push(e);
        }
    }

    /// Checks the option-state query for every option and side against what
    /// the OptionChanged Events so far say.
    fn check_query(&self) {
        for code in 0..=u8::MAX {
            let option = TelnetOption::new(code);
            for side in [Side::Local, Side::Remote] {
                assert_eq!(
                    self.core.is_enabled(option, side),
                    self.enabled.contains(&(option, side)),
                    "the option-state query disagrees with the OptionChanged Events for {option:?} {side:?}"
                );
            }
        }
    }
}

/// Reads `client:` and `event:` lines, as an Outcome prints them, back into one.
pub fn parse_outcome(text: &str) -> Outcome {
    let t = parse("outcome", text);
    match t.steps.as_slice() {
        [] => Outcome {
            client: Vec::new(),
            events: Vec::new(),
        },
        [step] if step.call.is_none() && step.server.is_empty() => expected(step),
        _ => panic!("expected only `client:` and `event:` lines"),
    }
}

pub fn expected(step: &Step) -> Outcome {
    Outcome {
        client: step.client.clone(),
        events: coalesce(step.events.clone()),
    }
}

/// Splits `bytes` at the given cut points (each taken modulo `len + 1`).
pub fn split<'a>(bytes: &'a [u8], cuts: &[usize]) -> Vec<&'a [u8]> {
    let mut at: Vec<usize> = cuts.iter().map(|c| c % (bytes.len() + 1)).collect();
    at.sort_unstable();
    let mut parts = Vec::new();
    let mut from = 0;
    for to in at {
        parts.push(&bytes[from..to]);
        from = to;
    }
    parts.push(&bytes[from..]);
    parts
}
