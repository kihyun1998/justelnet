# Option negotiation

How the Core agrees with the peer on which options are on, on each side: RFC 1143's Q method for all 256 option codes, on our side (`Side::Local`, WILL/WONT from us) and the peer's (`Side::Remote`, DO/DONT from us).

## What "on" means

An option counts as on only in RFC 1143's `YES` state. `is_enabled` and OptionChanged both follow that, so an Event fires exactly when an option enters or leaves `YES`:

- A caller's disable request turns the option off at the call, not when the peer confirms. The Core has already sent DONT/WONT and stops treating the option as on (RFC 854: the sender of a negative command stops using the option at once).
- A request that is answered and immediately undone by the queue (enable, then disable before the answer) never produces an Event, because the option never reaches `YES`.

## The policy has three levels

Each option on each side is refused, accepted, or requested. Accepted and requested options are agreed to when the peer asks; requested ones are also asked for at the start, in the order the builder first requested them. The default policy's order is the spec's (`WILL NAWS, WILL TTYPE, WILL NEW-ENVIRON, DO ECHO, WILL SGA, DO SGA`), which is also PuTTY's active-start order minus TSPEED. `builder()` starts from the default policy, since adjusting it is the common case; `refuse_all()` gives a blank slate.

This shape is the maintainer's call, made on 2026-09-29 when shown the alternative of keeping `support()` and passing the start list separately (two settings that could disagree). It is theirs to reverse.

## Passive start follows PuTTY, with the policy's list

A passive start sends nothing until the peer's negotiation first changes an option's state (an OptionChanged). Then, after answering the peer, it asks for every requested option still off and not being negotiated (`NO`), so a request the caller already has in flight, with its queue, is left alone. Refusing an option the policy does not accept, or ignoring a DONT/WONT for one already off, changes no state and does not start it. It starts once: an option the peer refuses afterwards is not asked for again.

The trigger and the order match PuTTY (`option_side_effects` in `telnet.c`: the answer is sent, then `activated` sends the requests). PuTTY asks only for ECHO and SGA both ways at that point; the Core asks for everything the policy requests, NAWS, TTYPE and NEW-ENVIRON included. Reading "sends nothing until the peer negotiates" as "then sends" rather than "only ever answers" is the maintainer's call, made on 2026-09-29, and theirs to reverse.

## The default policy

A character-mode terminal client: ECHO refused on our side and requested from the peer, SGA requested both ways, BINARY accepted both ways but never requested, TTYPE, NAWS and NEW-ENVIRON requested on our side. Everything else is refused on both sides. What follows from the Q method with no special code:

- telnetd's 4.2BSD `DO ECHO` probe gets `WONT ECHO`, and the `DONT ECHO` telnetd sends afterwards asks for a state already in effect, so it gets no answer.
- Every `DO TM` gets `WONT TM`, since TM stays in `NO`; this keeps BSD and inetutils telnetd out of kludge linemode. A runtime request that turns TM on on our side ends that (see the comment on #16).
- OLD-ENVIRON is refused, so the two ENVIRONs are never both on, which hangs inetutils and BSD telnetd. A caller's policy accepting OLD-ENVIRON, or a runtime request for it, can still put both on: the Core does not guard it (PuTTY turns one off when the other activates), and it never answers an OLD-ENVIRON SEND. The maintainer chose to leave it unguarded on 2026-09-29, as with TM and ECHO at both ends, since only a caller leaving the defaults reaches it; it is theirs to reverse.

## A default Core reaches a telnetd login prompt alone

With TTYPE (#17), NAWS and NEW-ENVIRON (#18) answered, a Core on the default policy gets a Unix telnetd from connect to its login prompt: probed against a netkit telnetd on RHEL 9 on 2026-09-29, with no help from the probe (`telnetd-netkit-rhel9.txt` is that exchange).

## The policy decides only what the peer may ask for

`OptionPolicy` answers the peer's WILL/DO in state `NO`: accepted options are agreed to, the rest refused (RFC 1123 §3.2.1). A caller's runtime request is sent whether or not the policy accepts the option, and the peer's agreement to it is taken without consulting the policy (RFC 1143's `WANTYES` rows have no "if we agree" test).

This is the maintainer's call, made on 2026-09-29 when shown the alternative of ignoring runtime requests for options outside the policy (and turning them into an Error once #20 adds one). It matches the spec's BINARY row: "never requested by default; runtime request allowed". It is theirs to reverse.

## Noncompliant answers are absorbed with a Warning

RFC 1143 marks two rows as errors: the peer answering our DONT/WONT with WILL/DO. The Core applies the state the RFC gives (off; or on, when an enable was queued), sends nothing further, and emits a `NoncompliantAnswer` Warning before any OptionChanged the row causes, as libtelnet reports `EPROTOCOL` there. The maintainer chose to emit it on 2026-09-29, once the Warning Event existed (#17), over leaving the rows silent; it is theirs to reverse.

## Redundant requests send nothing

A runtime request that RFC 1143 marks as an error (already on, already asked for, already queued) is a no-op, not an `Err`: the option is already where the caller wants it, so it is not a misuse, and `Err` is kept for misuse (#10). The maintainer chose this on 2026-09-29, once the Core had an Error type, over returning `Result` with an AlreadyEnabled-style variant; it is theirs to reverse. RFC 854 rule b (never acknowledge a request for a state already in effect) and the receive tables together are what stop peers such as gensio, which acknowledge a WONT for an option already off, from starting a loop.

## Names

`TelnetOption` is a newtype over the option code rather than an enum, so an option the Core has no constant for is still an ordinary value, and one option never has two spellings. `Side` is `Local`/`Remote` rather than the spec table's Us/Them, so the names still read right for a future server role. Both names are the maintainer's call, made on 2026-09-29.

## A refusal does not stick

After `request_disable` completes, the option is back in `NO`, and a later WILL/DO from the peer is answered from the policy again, as RFC 1143 and libtelnet do. PuTTY instead keeps a refused option refused (its `REALLY_INACTIVE` state). The Core cannot change its policy after `with_policy`, so a caller has no way today to refuse an option the policy accepts for the rest of a session.

This is the maintainer's call, made on 2026-09-29 when shown PuTTY's sticky state as the alternative; it is theirs to reverse, and to revisit if a caller needs a lasting refusal.

## OptionChanged is not `#[non_exhaustive]`

`Event` is non-exhaustive, but its `OptionChanged` variant is not, so adding a field to it would break callers' patterns. Marking it would stop code outside the crate, including the Transcript runner, from constructing the Event it compares against. The maintainer chose to leave it unmarked on 2026-09-29, judging that option, side and on/off are a closed set; it is theirs to reverse.
