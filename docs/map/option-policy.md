# Option policy

## What it is

The Option policy (GLOSSARY.md): the options the Core accepts on each side, the ones it asks for and in what order, how it opens the connection, and the values it answers subnegotiations with — terminal types, window size, environment variables — plus the end of line `send_data` writes and which options are Passthrough. It is fixed at `Core::with_policy` and read by every other Core territory; the default is the policy every Transcript without policy lines, the fuzz target and the e2e login run against.

## Governing decisions

- [ADR 0001](../adr/0001-sans-io-core-with-tokio-driver.md): the Core answers from an Option policy, TTYPE SEND and NEW-ENVIRON SEND included, without asking the caller.

The shape and defaults were the maintainer's calls on 2026-09-29 (#6), not decision records.

## Design model

From the source and the issues.

### The policy has three levels

Each option on each side is refused, accepted, or requested (`Stance`). Accepted and requested options are agreed to when the peer asks; requested ones are also asked for at the start, in the order the builder first requested them (`OptionPolicyBuilder::set` keeps `requests`; an option refused or accepted in between moves to the end when requested again). `builder()` starts from the default policy, since adjusting it is the common case; `refuse_all()` gives a blank slate. This shape is the maintainer's call, made on 2026-09-29 when shown the alternative of keeping `support()` and passing the start list separately (two settings that could disagree).

What the policy decides is only what the peer may ask for: a caller's runtime request bypasses it ([option negotiation](option-negotiation.md)). It cannot be changed after `with_policy`.

### The default policy

A character-mode terminal client, `OptionPolicy::default`, with an active start:

| Option | Local (WILL) | Remote (DO) |
|---|---|---|
| BINARY | accept | accept |
| ECHO | refuse | request |
| SGA | request | request |
| TTYPE, NAWS, NEW-ENVIRON | request | refuse |

Everything else is refused on both sides. The request order is `WILL NAWS, WILL TTYPE, WILL NEW-ENVIRON, DO ECHO, WILL SGA, DO SGA`, the spec's order, which is also PuTTY's active-start order minus TSPEED. Terminal types `["UNKNOWN"]`, window 80x24, no environment variables, end of line CR LF, no Passthrough options (`OptionPolicyBuilder::empty`). The table is also written in `OptionPolicy`'s rustdoc — a second copy to keep in step.

What follows from the Q method with no special code:

- telnetd's 4.2BSD `DO ECHO` probe gets `WONT ECHO`, and the `DONT ECHO` telnetd sends afterwards asks for a state already in effect, so it gets no answer.
- Every `DO TM` gets `WONT TM`, since TM stays in `NO`; this keeps BSD and inetutils telnetd out of kludge linemode. A runtime request that turns TM on on our side ends that (see the comment on #16). Seen on a real server: inetutils telnetd 2.8 sends `DO TM` and `DO LINEMODE` and gets `WONT` to both ([logging in to a real server](logging-in-to-a-real-server.md)).
- OLD-ENVIRON is refused, so the two ENVIRONs are never both on, which hangs inetutils and BSD telnetd. A caller's policy accepting OLD-ENVIRON, or a runtime request for it, can still put both on: the Core does not guard it (PuTTY turns one off when the other activates), and it never answers an OLD-ENVIRON SEND (`Core::subnegotiated` answers only TTYPE and NEW-ENVIRON). The maintainer chose to leave it unguarded on 2026-09-29, as with TM and ECHO at both ends, since only a caller leaving the defaults reaches it.
- BINARY is accepted both ways but never requested; inetutils telnetd asks for it, so against that server lines go out with a bare CR ([sending](sending.md)).

### A default Core reaches a telnetd login prompt alone

With TTYPE (#17), NAWS and NEW-ENVIRON (#18) answered, a Core on the default policy gets a Unix telnetd from connect to its login prompt: probed against a netkit telnetd on RHEL 9 on 2026-09-29, with no help from the probe (`justelnet-core/tests/transcripts/telnetd-netkit-rhel9.txt` is that exchange).

The default terminal type `UNKNOWN`, NAWS 80x24 and the empty environment are explained where they are used: [subnegotiation](subnegotiation.md) (why `UNKNOWN`, why no variables) and [waiting for patterns](waiting-for-patterns.md) (why automation should keep `UNKNOWN`).

## Code

- `justelnet-core/src/lib.rs` — `OptionPolicy`, `OptionPolicy::default`, `OptionPolicy::accepts`, `OptionPolicy::requests`, `OptionPolicy::start`, `OptionPolicy::terminal_types`, `OptionPolicy::window_size`, `OptionPolicy::is_passthrough`, `OptionPolicy::end_of_line`, `OptionPolicyBuilder` (`empty`, `refuse`, `accept`, `request`, `refuse_all`, `start`, `passthrough`, `end_of_line`, `window_size`, `variable`, `user_variable`, `terminal_types`, `set`), `Stance`, `Variable`, `Start`, `EndOfLine`
- `justelnet-core/tests/common/mod.rs` — `parse`, the `policy:`/`accept:`/`request:`/… directives that build a Transcript's policy

## Reference behaviour

- [Target research](../research/target-options-quirks.md): what Unix telnetd, ser2net and network gear ask for on connect, the basis of the default.
- PuTTY's active-start order (`telnet.c`), read raw.

## Cross-cutting invariants

**None.**

## Blast radius

- [Option negotiation](option-negotiation.md) — every answer and the start's bytes come from `accepts` and `requests`.
- [Subnegotiation](subnegotiation.md) — terminal types, window size, variables and the Passthrough marks are read there.
- [Sending](sending.md) — `end_of_line` decides what a CR becomes.
- [Transcript replay](transcript-replay.md) — a Transcript with no policy lines runs on `OptionPolicy::default()`, so a change to the default changes the expected first step of most Transcripts, and the worked example in `docs/transcripts.md`; the runner's directive list must name every builder setting a Transcript needs.
- [Fuzzing](fuzzing.md) — the fuzz target runs only the default policy.
- [Logging in to a real server](logging-in-to-a-real-server.md) — the e2e login uses the default policy; its recorded server behaviour (DO BINARY, DO TM) is what the default meets.
- [Driving a stream](driving-a-stream.md) — `cancel_safety.rs` plays netkit's server bytes against the default policy.

## Known holes / open

- The public getters (`accepts`, `requests`, `start`, `terminal_types`, `window_size`, `is_passthrough`, `end_of_line`) were flagged as possible scope creep that no spec asked for; removing them is a semver question left to the maintainer (#85, out of scope there).
- OLD-ENVIRON with NEW-ENVIRON both on is unguarded (above).
- The default was chosen from source reading and one netkit and one inetutils server; Cisco IOS and Juniper bytes are not captured. Tracked: #9.
