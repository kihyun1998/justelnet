# Option negotiation

## What it is

How the Core agrees with the peer on which options are on, on each side: RFC 1143's Q method for all 256 option codes, on our side (`Side::Local`, WILL/WONT from us) and the peer's (`Side::Remote`, DO/DONT from us). It answers the peer from the [Option policy](option-policy.md) without asking the caller, takes the caller's runtime requests, opens the connection (active or passive start), and reports every change as an OptionChanged Event.

## Governing decisions

- [ADR 0001](../adr/0001-sans-io-core-with-tokio-driver.md): the Core answers negotiation itself from an Option policy, "so a caller cannot stall a login by forgetting to reply" — Unix telnetd blocks login until every request is answered.

The rules below were the maintainer's calls on issues (#6, #10, #16, #17, #19, #20), not decision records.

## Design model

From the source and the issues.

### What "on" means

An option counts as on only in RFC 1143's `YES` state. `Core::is_enabled` and OptionChanged both follow that, so an Event fires exactly when an option enters or leaves `YES`:

- A caller's disable request turns the option off at the call, not when the peer confirms (`Core::request_disable` calls `changed(…, false)` at once). The Core has already sent DONT/WONT and stops treating the option as on (RFC 854: the sender of a negative command stops using the option at once).
- A request that is answered and immediately undone by the queue (enable, then disable before the answer) never produces an Event, because the option never reaches `YES`.

**"On" and "may still be acting" differ.** `Core::live` is `YES` or `WANTNO`: on, or asked off by us with the peer's confirmation not yet in. Receiving uses `live` — the peer counts as binary until its WONT arrives ([receive parsing](receive-parsing.md)), and a Passthrough option's subnegotiations still reach the caller ([subnegotiation](subnegotiation.md)). Sending uses `is_enabled` — after our WILL BINARY and before the peer's DO, a CR is still translated ([sending](sending.md)). One primitive, two readers each way; a change to `live` moves both receive-side rules.

### The receive tables

`Core::received` is RFC 1143's receive table for both sides, one `match` over (enable, `Q`, `opposite`). In state `NO`, a WILL/DO is agreed to if the policy accepts the option and refused otherwise (RFC 1123 §3.2.1). A NAWS turning on on our side sends the window size right after the WILL ([subnegotiation](subnegotiation.md)).

### Passive start follows PuTTY, with the policy's list

An active start (`Start::Active`, the default) sends every requested option from `Core::with_policy`, before any input. A passive start sends nothing until the peer's negotiation first changes an option's state (an OptionChanged). Then, after answering the peer, `Core::start` asks for every requested option still off and not being negotiated (`NO`), so a request the caller already has in flight, with its queue, is left alone. Refusing an option the policy does not accept, or ignoring a DONT/WONT for one already off, changes no state and does not start it. It starts once: an option the peer refuses afterwards is not asked for again.

The trigger and the order match PuTTY (`option_side_effects` in `telnet.c`: the answer is sent, then `activated` sends the requests). PuTTY asks only for ECHO and SGA both ways at that point; the Core asks for everything the policy requests, NAWS, TTYPE and NEW-ENVIRON included. Reading "sends nothing until the peer negotiates" as "then sends" rather than "only ever answers" is the maintainer's call, made on 2026-09-29.

### The policy decides only what the peer may ask for

A caller's runtime request (`request_enable`) is sent whether or not the policy accepts the option, and the peer's agreement to it is taken without consulting the policy (RFC 1143's `WANTYES` rows have no "if we agree" test). This is the maintainer's call, made on 2026-09-29 when shown the alternative of ignoring runtime requests for options outside the policy (and turning them into an Error once #20 adds one). It matches the spec's BINARY row: "never requested by default; runtime request allowed".

### Noncompliant answers are absorbed with a Warning

RFC 1143 marks two rows as errors: the peer answering our DONT/WONT with WILL/DO. The Core applies the state the RFC gives (off; or on, when an enable was queued), sends nothing further, and emits a `NoncompliantAnswer` Warning before any OptionChanged the row causes (`received` warns first), as libtelnet reports `EPROTOCOL` there. The maintainer chose to emit it on 2026-09-29, once the Warning Event existed (#17), over leaving the rows silent.

### Redundant requests send nothing

A runtime request that RFC 1143 marks as an error (already on, already asked for, already queued) is a no-op, not an `Err`: the option is already where the caller wants it, so it is not a misuse, and `Err` is kept for misuse (#10). The maintainer chose this on 2026-09-29, once the Core had an Error type, over returning `Result` with an AlreadyEnabled-style variant. RFC 854 rule b (never acknowledge a request for a state already in effect) and the receive tables together are what stop peers such as gensio, which acknowledge a WONT for an option already off, from starting a loop.

### A refusal does not stick

After `request_disable` completes, the option is back in `NO`, and a later WILL/DO from the peer is answered from the policy again, as RFC 1143 and libtelnet do. PuTTY instead keeps a refused option refused (its `REALLY_INACTIVE` state). The Core cannot change its policy after `with_policy`, so a caller has no way today to refuse an option the policy accepts for the rest of a session. This is the maintainer's call, made on 2026-09-29 when shown PuTTY's sticky state as the alternative; to be revisited if a caller needs a lasting refusal.

### Names

`TelnetOption` is a newtype over the option code rather than an enum, so an option the Core has no constant for is still an ordinary value, and one option never has two spellings. `Side` is `Local`/`Remote` rather than the spec table's Us/Them, so the names still read right for a future server role. Both names are the maintainer's call, made on 2026-09-29.

### OptionChanged is not `#[non_exhaustive]`

`Event` is non-exhaustive, but its `OptionChanged` variant is not, so adding a field to it would break callers' patterns. Marking it would stop code outside the crate, including the Transcript runner, from constructing the Event it compares against. The maintainer chose to leave it unmarked on 2026-09-29, judging that option, side and on/off are a closed set. The README's Versioning section names it as the deliberate case ([versioning](versioning.md)).

### Input bounds

State is two fixed arrays of 256 entries (`Options`); a peer cannot grow it. Each received verb sends at most one answer.

## Code

- `justelnet-core/src/lib.rs` — `Q`, `OptionState`, `Options`, `Core::received`, `Core::request_enable`, `Core::request_disable`, `Core::start`, `Core::with_policy`, `Core::is_enabled`, `Core::live`, `Core::send`, `Core::changed`, `Start`, `Side`, `TelnetOption`, `Warning::NoncompliantAnswer`, `Event::OptionChanged`

## Reference behaviour

- RFC 1143 (Q method), RFC 854, RFC 1123 §3.2.1 — binding per [thegraph](../agents/thegraph.md).
- PuTTY `telnet.c` (`option_side_effects`, `activated`, `REALLY_INACTIVE`) and libtelnet (`EPROTOCOL`), read raw.
- [Target research](../research/target-options-quirks.md): Unix telnetd's two blocking phases, gensio acknowledging a WONT already in effect.

## Cross-cutting invariants

- [Every Event has a Transcript spelling](every-event-has-a-transcript-spelling.md) — OptionChanged and NoncompliantAnswer.

## Blast radius

- [Option policy](option-policy.md) — `received` consults `accepts`, `start` walks `requests`; a new stance or a change to the request order changes the bytes of every start.
- [Receive parsing](receive-parsing.md) — reads `live(BINARY, Remote)` to decide whether CR NUL is stripped.
- [Subnegotiation](subnegotiation.md) — answers TTYPE/NEW-ENVIRON only while `is_enabled(…, Local)`, and delivers Passthrough subnegotiations while `live`; NAWS is sent from `received`.
- [Sending](sending.md) — `send_data` translates nothing while `is_enabled(BINARY, Local)`; `send_subnegotiation` refuses an option off on both sides.
- [Driving a stream](driving-a-stream.md) — `Client::request_enable/disable/is_enabled` mirror these methods and their docs.
- [Transcript replay](transcript-replay.md) — the runner checks `is_enabled` for all 256 options against the OptionChanged Events after every call and step; an Event that fires without a state change fails it.

## Known holes / open

- A caller's policy accepting OLD-ENVIRON, or a runtime request for it, can put both ENVIRONs on, which hangs inetutils and BSD telnetd; the Core does not guard it (see [option policy](option-policy.md)).
- No lasting refusal (above).
