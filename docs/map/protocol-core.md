# Aggregate — the protocol Core

**This note owns no detail.** The five territories that are `justelnet-core`: one connection's Telnet state behind one sans-IO API.

| Concept | Note |
|---|---|
| bytes in → Data, Command and Warning Events, in order | [receive parsing](receive-parsing.md) |
| RFC 1143 state per option and side, start, runtime requests | [option negotiation](option-negotiation.md) |
| what is accepted, requested and answered with, fixed at construction | [option policy](option-policy.md) |
| `IAC SB … IAC SE`: limits, TTYPE, NAWS, NEW-ENVIRON, Passthrough | [subnegotiation](subnegotiation.md) |
| caller's data and commands → bytes | [sending](sending.md) |

## Why they sit together

They share one struct (`justelnet-core/src/lib.rs — Core`) and one contract with every caller, set by [ADR 0001](../adr/0001-sans-io-core-with-tokio-driver.md): the caller pushes bytes in with `Core::receive` and calls, and pulls results out with `Core::poll_event` and `Core::poll_transmit`; nothing is pushed to the caller. Each member writes into the same two queues (`events`, `transmit`), so their outputs interleave in the order the input caused them, and none of them can wait, read a clock or touch I/O.

## The sans-IO boundary

What the members may not do, and where each part of that is checked:

- **No I/O and no clock.** Nothing in `justelnet-core` imports tokio or `std::time`; timeouts live in the Driver (`Client::connect`) and the Expect session (`Expect::wait`), per ADR 0001's consequence that expect-style automation "sits at or above the Driver, never in the Core". This is what makes [Transcript replay](transcript-replay.md) deterministic and [fuzzing](fuzzing.md) a pure function of its input. Checked only by reading.
- **No dependencies.** `justelnet-core/tests/no_dependencies.rs — manifest_lists_no_dependencies` fails if the manifest declares a normal or build dependency, for any target; `every_dependency_form_is_found` checks the scanner against every TOML form. A dev-dependency (proptest) is allowed.
- **`receive` never fails; misuse is the only `Error`.** Peer faults become Warning Events and the Core recovers; `Error` is returned only by `send_subnegotiation`, for caller misuse (#10). `Core::receive` returns `()`. The fuzz target is the check that it also never panics.
- **The pull API is the Driver's whole seam.** [Driving a stream](driving-a-stream.md) calls exactly `receive`, `poll_event`, `poll_transmit`, the send and request methods and `is_enabled`. A Core change that adds a method a caller must call at the right time (rather than an Event it may poll) changes the Driver's loop, not just its forwarding.

Changing one member: check its own note's blast radius; changing the boundary itself (an await, a clock, a dependency, a fallible `receive`) reopens ADR 0001.
