# Receive parsing

## What it is

How the Core turns received bytes into Events: `Core::receive` walks the bytes through a small parser, strips every Telnet control byte out of the data, hands negotiation to [option negotiation](option-negotiation.md) and `IAC SB … IAC SE` to [subnegotiation](subnegotiation.md), and queues Data, Command and Warning Events in the order the bytes arrived.

## Governing decisions

- [ADR 0001](../adr/0001-sans-io-core-with-tokio-driver.md): the Core takes bytes in and hands Events out, pulled by the caller; it has no clock and no I/O, so it cannot see TCP urgent data (below).

The per-byte rules were settled on issues (#6, #10) and by the maintainer on 2026-09-29; they are not decision records.

## Design model

From the source and the issues; no record states these rules.

### What comes out of the bytes after IAC

`IAC IAC` becomes a single 255 data byte, `IAC WILL/WONT/DO/DONT x` goes to option negotiation, and `IAC SB` to subnegotiation (`Core::command`). The nine commands NOP, DM, BRK, IP, AO, AYT, EC, EL and GA (241–249, `Command::from_code`) each become a Command Event in place among the data. Any other byte after IAC (SE outside a subnegotiation, or anything below 240, EOR, ABORT, SUSP and EOF included, whose options the Core refuses) is dropped with an `UnknownCommand` Warning. No Telnet control byte reaches Data.

Warning on those bytes is the maintainer's call, made on 2026-09-29 over dropping them silently or warning on a stray SE only; it is theirs to reverse (maintainer, 2026-09-29).

### DM is reported, and no data is discarded

RFC 854's Synch has the receiver discard data up to the DM once TCP urgent data signals it, and RFC 1123 §3.2.4 makes that a MUST. The Core cannot know where discarding should start: it sees a byte stream, and a generic stream carries no urgent signal. So `IAC DM` is a Command Event like the others and the data around it is kept (#6). PuTTY discards while `in_synch` is set and stops at the next 0xF2 byte, a heuristic its own comment calls hoping for the best.

The urgent mark also damages the bytes before they reach the Core: the Core cannot repair this from the bytes it is given, so the socket has to keep urgent data inline — see [socket setup matches connect](socket-setup-matches-connect.md), which holds the probe.

### CR NUL

While the peer's BINARY may be on — on, or asked off with the peer's WONT not yet in (`Core::peer_binary`, through `live`, see [option negotiation](option-negotiation.md)) — nothing is dropped. Otherwise a NUL right after a data CR is dropped, so CR NUL arrives as CR (RFC 854). CR LF and a bare LF pass through as data. The CR is not held back: it goes out at once (`cr_seen` is kept across `receive` calls), and a NUL at the start of the next `receive` is still dropped. Anything between the two, a command included, keeps the NUL, since `IAC` clears `cr_seen`; as in PuTTY (`SEENCR` lasts one byte).

### Data and other Events keep their order

Received data is held in `Core::data` until the Core queues another Event or the `receive` call ends, then queued as one Data Event ahead of it (`Core::flush_data`, called from `changed`, `warn`, `command` and `subnegotiated`). Data before a negotiation therefore comes out before its OptionChanged, and data after it comes out after. How many Data Events one stream makes depends on how it was chunked; see [no empty Data Event](no-empty-data-event.md) for what the caller can rely on.

### Input bounds

`receive` takes any slice and never fails; data is queued per call, so one call's Data Event is at most the size of that call's input. Nothing here bounds how many Events pile up if the caller never polls. A subnegotiation body is bounded separately (4096 bytes, [subnegotiation](subnegotiation.md)).

## Code

- `justelnet-core/src/lib.rs` — `Core::receive`, `Parse`, `Core::command`, `Command::from_code`, `Core::flush_data`, `Core::peer_binary`, `Core::warn`, `Warning::UnknownCommand`, `Event::Data`, `Event::Command`

## Reference behaviour

- [Target research](../research/target-options-quirks.md): Synch/DM sent as TCP urgent data is one of the quirks a client must tolerate.
- PuTTY `telnet.c` (`in_synch`, `SEENCR`) and libtelnet, read raw per [thegraph](../agents/thegraph.md); RFC 854, RFC 1123 §3.2.4.

## Cross-cutting invariants

- [No empty Data Event](no-empty-data-event.md) — `flush_data` is the site that keeps it.
- [Every Event has a Transcript spelling](every-event-has-a-transcript-spelling.md) — Command and Warning variants made here.
- [Socket setup matches connect](socket-setup-matches-connect.md) — why the Synch reaches `receive` whole.

## Blast radius

- [Option negotiation](option-negotiation.md) — the parser hands it `IAC <verb> x` and relies on its `live` for BINARY; a change to what counts as "peer binary" changes which NULs are dropped here.
- [Subnegotiation](subnegotiation.md) — the parser's `Sb*` states decide where a subnegotiation ends; recovery from `IAC x` inside one is shared code (`Core::command` is re-entered from `SbIac`).
- [Transcript replay](transcript-replay.md) — every parse rule is pinned by a Transcript (`receive-*.txt`, `plain-text-and-escaped-iac.txt`); a rule change is a Transcript change.
- [Waiting for patterns](waiting-for-patterns.md) — Expect matches on Data as this parser cuts it; a byte that leaks into or out of Data changes what a prompt regex sees.
- [Fuzzing](fuzzing.md) — the fuzz target drives only `receive`; a new parser state is new ground for it.

## Known holes / open

- On BSD-semantics stacks the DM byte is the one urgent data takes, leaving a bare IAC to swallow the next byte; not observed (maintainer probe notes, 2026-09-29).
- Nothing bounds the Event queue if a caller feeds bytes and never polls; the Driver always polls, so only a direct Core caller can reach it.
