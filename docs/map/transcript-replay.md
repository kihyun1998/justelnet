# Transcript replay

## What it is

How the Core is proven: every behaviour is a Transcript (GLOSSARY.md) — a `.txt` file under `justelnet-core/tests/transcripts/` — replayed through the Core's public API whole, a byte at a time, and under arbitrary chunking. It covers the format and its runner, and the procedure that turns a capture from a real server, the e2e recording or a fuzz finding into a Transcript.

## Governing decisions

- [ADR 0001](../adr/0001-sans-io-core-with-tokio-driver.md): the Core has no clock, "so recorded server byte sequences can be replayed against it deterministically" — the reason Transcripts can exist.

The format's single home is the maintainer's call (below); the rest came with #34, #62, #63.

## Design model

From the source and `docs/transcripts.md`.

### Steps, not one stream

A Transcript is a sequence of steps, each opened by a `server:` or `call:` line. Lines before the first form a step with no server bytes, which is where output the Core produces before any input (an active start) is expected. `tests/transcripts.rs` replays each step's server bytes as one chunk and one byte at a time; `tests/chunking.rs` (proptest) splits each step's server bytes independently at arbitrary points (`common::split`), so anything a later directive does between steps keeps its position.

### Why adjacent Data Events are merged before comparing

The Core emits the data of one `receive` call as one Data Event, split only where another Event falls between. Chunking the same bytes differently therefore changes how many Data Events come out without changing what the peer said. The runner merges adjacent Data Events on both sides (`common::coalesce`), so the chunking property compares meaning, not call boundaries. The merge opens a trap — it would hide empty Data Events — held by [no empty Data Event](no-empty-data-event.md).

### The option-state query is checked on every step

After every call and at the end of every step, the runner compares `is_enabled` for all 256 options on both sides with what the OptionChanged Events so far say (`Replay::check_query`), and an OptionChanged without a state change fails (`Replay::drain`). The acceptance criterion "the option-state query agrees with OptionChanged" is then asserted by every Transcript, including in states no Event marks, such as a request waiting for its answer (`WANTYES`), which is visible right after the call that made it.

It is not checked after every chunk: that cost grew with each Transcript (the chunking property took 9.9 s with fourteen Transcripts, 2.2 s with the check per step; debug build, Windows, measured when the check was placed), and the states it would add are the ones between two bytes of one step, which no mutation tried so far needed to reach.

### Transcripts start from the default Option policy

A Transcript without policy lines runs against `OptionPolicy::default()` (`common::parse` starts from `OptionPolicy::builder()`), active start included, so its first step expects the start's bytes. `policy: empty` gives the blank slate the option-engine Transcripts are written against: nothing accepted, a passive start, so only what the Transcript lists happens.

### Where the format is written down

`docs/transcripts.md` is the one description of the format; the doc comment atop `tests/common/mod.rs` only points to it. Keeping the directive list in one place, rather than in both, is the maintainer's call (2026-10-08, chosen over a section in CONTRIBUTING.md that pointed at the list in `mod.rs`, and over a copy in each). A new directive in the parser has to be added to that page — see [every Event has a Transcript spelling](every-event-has-a-transcript-spelling.md) for the vocabulary the two must share.

### A failure prints as Transcript lines

A step's `Outcome` prints (its `Debug`) as the `client:` and `event:` lines it would take in a Transcript, or `# nothing`, so a failure can be read against the file line by line rather than converted from decimal (#63). `tests/transcript_format.rs` holds the printing to the parser. The printer escapes `"` inside text, but the parser also accepts a bare one there, so dropping that escape is a change no test sees (2026-10-08).

### What a capture can and cannot supply

A client that answers nothing records only the server's first burst: the lab netkit telnetd sent twelve bytes in ten seconds and then waited (2026-10-08). A PuTTY log of a telnet session holds no bytes at all: with "SSH packets and raw data" set, `otherbackends/telnet.c` reports negotiation only through `logevent` as text (`server negotiation: DO TTYPE`), names an option it does not know `<unknown>`, and `log_packet` is never called for telnet (PuTTY source, 2026-08-26 head). So the procedure in `docs/transcripts.md` takes bytes from a silent capture or the end-to-end recording, and uses a PuTTY log for order only. Later server bytes are only as good as the answers they followed: a capture from a client that answered differently from the Core is usable only up to the first differing answer.

**A Transcript is written from the RFCs, not from the Core.** `docs/transcripts.md` step 3 tells the author to check each expected byte and Event against the RFCs and the policy; copying the runner's `left` into the file makes a Transcript that can never fail. A Transcript is committed and public, so it is cut at the login prompt (no account or password).

### Captured, not written

Three Transcripts are captured from real servers (each header says so): `telnetd-netkit-rhel9.txt` (lab netkit, 2026-09-29), `telnetd-netkit-rhel9-silent-capture.txt` (the same server answering nothing, 2026-10-08, the worked example in `docs/transcripts.md`) and `telnetd-inetutils-2.8-recorded.txt` (the e2e recording, six CI runs, #62). Three follow a server's source, not a capture: `telnetd-inetutils-negotiation.txt`, `telnetd-netbsd-negotiation.txt` and `ser2net-start.txt`. Where the inetutils recording differs from the source-read one (a second TTYPE SEND), its header says so. The rest are written from the RFCs.

### Input bounds

The runner reads every `.txt` in the directory (`common::load_all`, which asserts there is at least one); a Transcript's size is bounded by what is committed.

## Code

- `justelnet-core/tests/common/mod.rs` — `Transcript`, `Step`, `Call`, `Replay`, `Replay::drain`, `Replay::check_query`, `Outcome`, `parse`, `run_step`, `coalesce`, `expected`, `parse_outcome`, `split`, `load_all`, `transcripts_dir`, the printers (`event_text`, `option_text`, `command_text`, `hex_text`, `quoted_text`) and parsers (`event`, `option_named`, `command_named`, `hex`, `quoted`, `window`, `side_option`)
- `justelnet-core/tests/transcripts.rs` — `every_transcript_replays`, `every_transcript_replays_one_byte_at_a_time`
- `justelnet-core/tests/chunking.rs` — `every_transcript_survives_arbitrary_chunking`; `chunking.proptest-regressions`
- `justelnet-core/tests/transcript_format.rs` — `every_expected_outcome_round_trips`, `every_byte_option_and_command_round_trips`, `nothing_round_trips`
- `justelnet-core/tests/transcripts/*.txt`
- `docs/transcripts.md` — the format and the capture procedures

## Reference behaviour

- PuTTY `otherbackends/telnet.c` (`logevent`, `log_packet`), 2026-08-26 head: what a PuTTY log holds.
- The captured Transcripts above are the comparison with real servers; [target research](../research/target-options-quirks.md) is the source-read side.

## Cross-cutting invariants

- [No empty Data Event](no-empty-data-event.md) — `Replay::drain` checks it.
- [Every Event has a Transcript spelling](every-event-has-a-transcript-spelling.md) — the printer, the parser and `docs/transcripts.md`.

## Blast radius

- [Option policy](option-policy.md) — Transcripts default to `OptionPolicy::default()`; the runner's policy directives mirror the builder.
- [Protocol core](protocol-core.md) — every Core territory is pinned here; a Core behaviour change is a Transcript change, written from the RFC first.
- [Driving a stream](driving-a-stream.md) — `cancel_safety.rs` copies `telnetd-netkit-rhel9.txt`'s server lines; editing that Transcript leaves the copy behind.
- [Fuzzing](fuzzing.md) — a fuzz crash lands as a Transcript (CONTRIBUTING.md); the fuzz target and the runner check the same empty-Data rule.
- [Logging in to a real server](logging-in-to-a-real-server.md) — the e2e recording's `server:`/`client:` lines are this format without `event:` lines; changing the line syntax changes the `Recording` writer.
- [Sending](sending.md) and [waiting for patterns](waiting-for-patterns.md) — a capture from network gear tests their device assumptions (reverse-telnet end of line; `--More--` backspaces), which no captured Transcript backs yet.
- [CI gates](ci-gates.md) — the replay runs inside `cargo test --workspace` in every `test (…)` job.

## Known holes / open

- No Transcript from network gear (Cisco IOS, Juniper). Tracked: #9.
- The parser accepts a bare `"` inside quoted text, so the printer's escape is untested (above).
- The option-state query is not checked between chunks of one step (above).
