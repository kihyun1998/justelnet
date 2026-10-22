# Sending

## What it is

How the caller's bytes and commands become bytes on the wire: `Core::send_data` (text, where CR means Enter), `Core::send_raw` (bytes as they are), `Core::send_command` (`IAC <command>`), all queued for `poll_transmit`. The Driver and the Expect session forward to these and restate their rules in their own docs.

## Governing decisions

**None.** [ADR 0001](../adr/0001-sans-io-core-with-tokio-driver.md) places sending in the Core (queued, pulled by the caller) but decides nothing about what the bytes are; the end-of-line and command rules are the maintainer's calls on #6.

## Design model

From the source and the issues.

### A CR means Enter

`send_data` turns every CR into the policy's end of line (`EndOfLine`): CR LF by default (RFC 1123 §3.3.1's SHOULD), CR NUL, or LF, the setting per device that the Cisco reverse-telnet knobs show is needed. LF and every other byte pass through; IAC is doubled. So a caller that sends CR LF for Enter sends CR LF LF under the default: the caller sends CR alone for Enter, the way a terminal's Enter key does. `Expect::send_line` adds that CR (`with_enter`).

PuTTY splits the two: its Enter is a special (`SS_EOL`, CR LF, or CR alone under BINARY), and a CR inside data is sent as CR NUL, RFC 854's bare CR. The Core has one data path, so a caller that needs a CR without Enter on the wire uses `send_raw`, which sends it bare: RFC 854 wants CR NUL there while BINARY is off, so the caller writes the NUL itself (`send_raw(b"\r\0")`). PuTTY and libtelnet stuff the NUL for it; `send_raw` translates nothing on purpose.

While BINARY is on on our side (`is_enabled(BINARY, Local)`), `send_data` translates nothing and only doubles IAC, as `send_raw` always does. "On" is agreed, not asked for: after our WILL BINARY and before the peer's DO, a CR is still translated (RFC 856). Receiving is the mirror: the peer counts as binary until its WONT arrives, even after our DONT ([option negotiation](option-negotiation.md), `live`). The default policy accepts BINARY and inetutils telnetd asks for it, so against that server a line goes out as a bare CR ([logging in to a real server](logging-in-to-a-real-server.md)).

**The rule is written in three crates.** The logic is only `Core::send_data`; the BINARY exception is restated in the docs of `Core::send_data`, `Client::send_data`, `with_enter` and `Expect::send_line` (and `Expect::send_secret_line` by reference). The Expect copy once lacked the exception while the Driver's had it (#85, fixed by #87 with a test, `a_line_ends_in_a_bare_cr_while_binary_is_on_on_our_side`). A change to the rule changes all four. The sites, by tool: `rg -n "BINARY is on" justelnet*/src`.

### Commands

`send_command` sends IAC and the command's own code, nothing else: Break is never sent as IP or the other way round (some devices treat them differently; Cisco has a knob to turn IP into a line BREAK). One `Command` type serves both directions, so GA and DM can be sent too. A DM sent this way carries no TCP urgent mark, which only the Driver could add, and does not. The single type is the maintainer's call, made on 2026-09-29 over a separate send-only type without GA and DM; it is theirs to reverse.

### Input bounds

The transmit queue (`Core::transmit`) grows with what the caller sends and is emptied by `poll_transmit`; nothing bounds it if the caller never polls. The Driver polls on every send.

## Code

- `justelnet-core/src/lib.rs` — `Core::send_data`, `Core::send_raw`, `Core::send_command`, `Command::code`, `EndOfLine`, `Core::poll_transmit`
- `justelnet/src/lib.rs` — `Client::send_data`, `Client::send_raw`, `Client::send_command` (forwarders)
- `justelnet-expect/src/lib.rs` — `with_enter`, `Expect::send_line`, `Expect::send_secret_line`, `Expect::send_raw`, `Expect::send_command`

## Reference behaviour

- RFC 854, RFC 856 (BINARY), RFC 1123 §3.3.1.
- PuTTY `telnet.c` (`SS_EOL`), libtelnet `telnet_send_text`: see [libtelnet vs russh](../research/libtelnet-vs-russh.md) for libtelnet's text translation.
- [Target research](../research/target-options-quirks.md): Cisco's `telnet transparent` and BREAK/IP knobs, the reason end of line is a setting.

## Cross-cutting invariants

- [Every Event has a Transcript spelling](every-event-has-a-transcript-spelling.md) — `Command`'s names are the format's `call: command` and `event: command`.

## Blast radius

- [Option policy](option-policy.md) — `end_of_line` is a policy setting.
- [Option negotiation](option-negotiation.md) — the BINARY test reads `is_enabled`.
- [Driving a stream](driving-a-stream.md) — `Client::send_*` docs restate the end-of-line rule.
- [Waiting for patterns](waiting-for-patterns.md) — `send_line`'s docs restate it; a device's echo of what was sent arrives as Data a wait sees.
- [Session log](session-log.md) — `Logged::Sent` carries the line and its CR before translation, so a log shows CR where the wire had CR LF.
- [Receive parsing](receive-parsing.md) — `Command` is shared by both directions; a new command code is new on both.
- [Transcript replay](transcript-replay.md) — `send-*.txt` Transcripts pin each end of line and BINARY.

## Known holes / open

- No way to send a Synch (DM with a TCP urgent mark); the Driver does not expose urgent sends.
- That network gear needs an end of line other than CR LF, and treats BRK and IP differently, rests on Cisco's documented reverse-telnet knobs, not on captured bytes. A reverse-telnet console capture would confirm or overturn it. Tracked: #9.
