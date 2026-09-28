# Option negotiation

How the Core agrees with the peer on which options are on, on each side: RFC 1143's Q method for all 256 option codes, on our side (`Side::Local`, WILL/WONT from us) and the peer's (`Side::Remote`, DO/DONT from us).

## What "on" means

An option counts as on only in RFC 1143's `YES` state. `is_enabled` and OptionChanged both follow that, so an Event fires exactly when an option enters or leaves `YES`:

- A caller's disable request turns the option off at the call, not when the peer confirms. The Core has already sent DONT/WONT and stops treating the option as on (RFC 854: the sender of a negative command stops using the option at once).
- A request that is answered and immediately undone by the queue (enable, then disable before the answer) never produces an Event, because the option never reaches `YES`.

## The policy decides only what the peer may ask for

`OptionPolicy` answers the peer's WILL/DO in state `NO`: supported options are accepted, the rest refused (RFC 1123 §3.2.1). A caller's runtime request is sent whether or not the policy supports the option, and the peer's agreement to it is taken without consulting the policy (RFC 1143's `WANTYES` rows have no "if we agree" test).

This is the maintainer's call, made on 2026-09-29 when shown the alternative of ignoring runtime requests for options outside the policy (and turning them into an Error once #20 adds one). It matches the spec's BINARY row: "never requested by default; runtime request allowed". It is theirs to reverse.

## Noncompliant answers are absorbed silently

RFC 1143 marks two rows as errors: the peer answering our DONT/WONT with WILL/DO. The Core applies the state the RFC gives (off; or on, when an enable was queued) and sends nothing further. There is no Warning Event yet, because Warning arrives with subnegotiation (#17). Whether these rows should then emit one is open.

## Redundant requests send nothing

A runtime request that RFC 1143 marks as an error (already on, already asked for, already queued) is a no-op: there is no Core Error type until #20. RFC 854 rule b (never acknowledge a request for a state already in effect) and the receive tables together are what stop peers such as gensio, which acknowledge a WONT for an option already off, from starting a loop.

## Names

`TelnetOption` is a newtype over the option code rather than an enum, so an option the Core has no constant for is still an ordinary value, and one option never has two spellings. `Side` is `Local`/`Remote` rather than the spec table's Us/Them, so the names still read right for a future server role. Both names are the maintainer's call, made on 2026-09-29.

## A refusal does not stick

After `request_disable` completes, the option is back in `NO`, and a later WILL/DO from the peer is answered from the policy again, as RFC 1143 and libtelnet do. PuTTY instead keeps a refused option refused (its `REALLY_INACTIVE` state). The Core cannot change its policy after `with_policy`, so a caller has no way today to refuse an option the policy supports for the rest of a session.

This is the maintainer's call, made on 2026-09-29 when shown PuTTY's sticky state as the alternative; it is theirs to reverse, and to revisit if a caller needs a lasting refusal.

## OptionChanged is not `#[non_exhaustive]`

`Event` is non-exhaustive, but its `OptionChanged` variant is not, so adding a field to it would break callers' patterns. Marking it would stop code outside the crate, including the Transcript runner, from constructing the Event it compares against. The maintainer chose to leave it unmarked on 2026-09-29, judging that option, side and on/off are a closed set; it is theirs to reverse.
