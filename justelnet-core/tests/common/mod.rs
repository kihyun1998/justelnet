//! Transcript parsing and replay against the Core's public API.
//!
//! A Transcript is a text file, one directive per line:
//!
//! - `# ...` a comment; blank lines are ignored
//! - `support: <side> <option>` the Option policy accepts the peer enabling
//!   this option on this side; only before the first step
//! - `server: <hex bytes>` bytes the peer sends; starts a new step
//! - `call: enable|disable <side> <option>` a runtime request from the caller;
//!   starts a new step
//! - `client: <hex bytes>` bytes the Core must send during the current step
//! - `event: data "<text>"` an Event the Core must emit during the current step;
//!   the text accepts `\r`, `\n`, `\\`, `\"` and `\xNN` escapes
//! - `event: option <side> <option> on|off` an OptionChanged Event
//!
//! A side is `local` or `remote`. An option is a name (`BINARY`, `ECHO`, `SGA`,
//! `TM`, `TTYPE`, `NAWS`, `NEW-ENVIRON`, …) or a two-digit hex code.
//! Lines before the first `server:` or `call:` form a step with no server bytes.
//! Adjacent Data Events are merged before comparing, so how received bytes
//! were chunked never changes the result.

#![allow(dead_code)]

use justelnet_core::{Core, Event, OptionPolicy, Side, TelnetOption};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// A runtime request the caller makes at the start of a step.
#[derive(Debug, Clone, Copy)]
pub struct Call {
    pub enable: bool,
    pub side: Side,
    pub option: TelnetOption,
}

#[derive(Debug, Clone, Default)]
pub struct Step {
    pub call: Option<Call>,
    pub server: Vec<u8>,
    pub client: Vec<u8>,
    pub events: Vec<Event>,
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

#[derive(Debug, PartialEq, Eq)]
pub struct Outcome {
    pub client: Vec<u8>,
    pub events: Vec<Event>,
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
            "support" => {
                assert!(
                    steps.len() == 1,
                    "{}: `support:` after the first step",
                    at()
                );
                let (side, option) = side_option(rest, &at());
                policy = policy.support(option, side);
            }
            "call" => {
                let (verb, target) = rest.split_once(' ').unwrap_or_else(|| {
                    panic!("{}: expected `enable|disable <side> <option>`", at())
                });
                let enable = match verb {
                    "enable" => true,
                    "disable" => false,
                    other => panic!("{}: unknown call `{other}`", at()),
                };
                let (side, option) = side_option(target, &at());
                steps.push(Step {
                    call: Some(Call {
                        enable,
                        side,
                        option,
                    }),
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
    s.split_whitespace()
        .map(|b| u8::from_str_radix(b, 16).unwrap_or_else(|_| panic!("{at}: bad hex byte `{b}`")))
        .collect()
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
    if let Some(call) = step.call {
        if call.enable {
            replay.core.request_enable(call.option, call.side);
        } else {
            replay.core.request_disable(call.option, call.side);
        }
        replay.drain(&mut events);
        replay.core.poll_transmit(&mut client);
    }
    for chunk in chunks {
        replay.core.receive(chunk);
        replay.drain(&mut events);
        replay.core.poll_transmit(&mut client);
    }
    Outcome {
        client,
        events: coalesce(events),
    }
}

impl Replay {
    /// Takes every queued Event, then checks the option-state query for every
    /// option and side against what the OptionChanged Events so far say.
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
