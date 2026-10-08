# Transcript replay

How the Core is proven: every behaviour is a Transcript replayed through the Core's public API, once as written and once under arbitrary chunking.

## Why adjacent Data Events are merged before comparing

The Core emits the data of one `receive` call as one Data Event, split only where another Event (an OptionChanged, a Command, a Warning) falls between. Chunking the same bytes differently therefore changes how many Data Events come out without changing what the peer said. The runner merges adjacent Data Events on both sides, so the chunking property compares meaning, not call boundaries.

## The trap the merge opens

The merge also hides a Core that emits **empty** Data Events: they fold into their neighbours and every comparison stays green. The runner rejects any empty Data Event before merging. This guard was added after a mutation (emitting Data for every `receive`, even with no data) survived both tests without it.

## Steps, not one stream

A Transcript is a sequence of steps, each opened by a `server:` line. Lines before the first `server:` are a step with no server bytes, which is where output the Core produces before any input (an active start) is expected. The chunking property splits each step's server bytes independently, so anything a later directive does between steps keeps its position.

## The option-state query is checked on every step

After every call and at the end of every step, the runner compares `is_enabled` for all 256 options on both sides with what the OptionChanged Events so far say. The acceptance criterion "the option-state query agrees with OptionChanged" is then asserted by every Transcript, including in states no Event marks, such as a request waiting for its answer (`WANTYES`), which is visible right after the call that made it.

It is not checked after every chunk: that cost grew with each Transcript (the chunking property took 9.9 s with fourteen Transcripts, 2.2 s with the check per step; debug build, Windows), and the states it would add are the ones between two bytes of one step, which no mutation tried so far needed to reach.

## Transcripts start from the default Option policy

A Transcript without policy lines runs against `OptionPolicy::default()`, active start included, so its first step expects the start's bytes. `policy: empty` gives the blank slate the option-engine Transcripts are written against: nothing accepted, a passive start, so only what the Transcript lists happens.

## Where the format is written down

`docs/transcripts.md` is the one description of the format; the doc comment atop `tests/common/mod.rs` only points to it. Keeping the directive list in one place, rather than in both, is the maintainer's call (2026-10-08, chosen over a section in CONTRIBUTING.md that pointed at the list in `mod.rs`, and over a copy in each); it is theirs to reverse. A new directive in the parser has to be added to that page.

## What a capture can and cannot supply

A client that answers nothing records only the server's first burst: the lab netkit telnetd sent twelve bytes in ten seconds and then waited (2026-10-08). A PuTTY log of a telnet session holds no bytes at all: with "SSH packets and raw data" set, `otherbackends/telnet.c` reports negotiation only through `logevent` as text (`server negotiation: DO TTYPE`), names an option it does not know `<unknown>`, and `log_packet` is never called for telnet (PuTTY source, 2026-08-26 head). So the procedure takes bytes from a silent capture or the end-to-end recording, and uses a PuTTY log for order only.

## A failure prints as Transcript lines

A step's Outcome prints as the `client:` and `event:` lines it would take in a Transcript, so a failure can be read against the file line by line rather than converted from decimal. `tests/transcript_format.rs` holds the printing to the parser: every expected Outcome, and one holding every byte, option code and command, must parse back to itself, so a new Event variant the printer does not know turns it red as soon as a Transcript expects one (the printer falls back to its Debug, which the parser rejects). The printer escapes `"` inside text, but the parser also accepts a bare one there, so dropping that escape is a change no test sees (2026-10-08).
