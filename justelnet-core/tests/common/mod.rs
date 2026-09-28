//! Transcript parsing and replay against the Core's public API.
//!
//! A Transcript is a text file, one directive per line:
//!
//! - `# ...` a comment; blank lines are ignored
//! - `server: <hex bytes>` bytes the peer sends; each `server:` line starts a new step
//! - `client: <hex bytes>` bytes the Core must send during the current step
//! - `event: data "<text>"` an Event the Core must emit during the current step;
//!   the text accepts `\r`, `\n`, `\\`, `\"` and `\xNN` escapes
//!
//! Lines before the first `server:` form a step with no server bytes.
//! Adjacent Data Events are merged before comparing, so how received bytes
//! were chunked never changes the result.

#![allow(dead_code)]

use justelnet_core::{Core, Event};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct Step {
    pub server: Vec<u8>,
    pub client: Vec<u8>,
    pub events: Vec<Event>,
}

#[derive(Debug, Clone)]
pub struct Transcript {
    pub name: String,
    pub steps: Vec<Step>,
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
        steps,
    }
}

fn hex(s: &str, at: &str) -> Vec<u8> {
    s.split_whitespace()
        .map(|b| u8::from_str_radix(b, 16).unwrap_or_else(|_| panic!("{at}: bad hex byte `{b}`")))
        .collect()
}

fn event(s: &str, at: &str) -> Event {
    let (name, arg) = s.split_once(' ').unwrap_or((s, ""));
    match name {
        "data" => Event::Data(quoted(arg.trim(), at)),
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

/// Feeds one step's server bytes in the given chunks and collects what comes out.
pub fn run_step<'a>(core: &mut Core, chunks: impl IntoIterator<Item = &'a [u8]>) -> Outcome {
    let mut client = Vec::new();
    let mut events = Vec::new();
    core.poll_transmit(&mut client);
    for chunk in chunks {
        core.receive(chunk);
        while let Some(e) = core.poll_event() {
            assert!(
                e != Event::Data(Vec::new()),
                "the Core emitted an empty Data Event"
            );
            events.push(e);
        }
        core.poll_transmit(&mut client);
    }
    Outcome {
        client,
        events: coalesce(events),
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
