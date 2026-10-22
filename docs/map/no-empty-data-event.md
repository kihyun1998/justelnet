# No empty Data Event

## The fact

The Core never emits `Event::Data` with an empty body. Checkable: every site that drains Core Events asserts `event != Event::Data(Vec::new())`.

```sh
rg -n "empty Data Event" justelnet-core
```

## Why it is cross-cutting

The Core keeps it in one place (`Core::flush_data` queues Data only when `data` is non-empty), but it is checked at two sites that do not call each other and that drive the Core differently — the Transcript runner under chosen and arbitrary chunking, and the fuzz target under fuzzed bytes and chunking — and `docs/transcripts.md` states it as part of the format's contract ("the Core must never emit an empty one"). Each check exists because the comparison around it would hide a violation: the runner merges adjacent Data Events before comparing, so an empty one folds into its neighbour and every comparison stays green.

## Territories it holds in

- [Receive parsing](receive-parsing.md) — `justelnet-core/src/lib.rs — Core::flush_data` keeps it; every Event-queuing path (`changed`, `warn`, `command`, `subnegotiated`, the end of `receive`) goes through it.
- [Transcript replay](transcript-replay.md) — `justelnet-core/tests/common/mod.rs — Replay::drain` asserts it before `coalesce` merges.
- [Fuzzing](fuzzing.md) — `justelnet-core/fuzz/fuzz_targets/receive.rs — feed` asserts it after each chunk.

## What a violation looks like

A caller's `Event::Data` handler sees zero bytes; the Driver returns it from `next_event` as an Event like any other, so a terminal loop wakes for nothing and an Expect wait appends nothing. In the Transcript runner, without the guard, nothing goes red at all.

## Discovery history

- The runner's guard was added after a mutation — emitting Data for every `receive`, even with no data — survived both the replay and the chunking tests without it (maintainer's mutation run, recorded in the hand-written transcript-replay note); the string is in the runner from the walking skeleton on (commit `57eced1`, 2026-09-28).
- The fuzz target carried the same assertion from its first version (commit `23fa06c`, 2026-09-29, #21).

## Where it will recur

If a change adds a new place that queues an Event (a new Event variant, a new flush point, a Data split for a new reason), it is subject to this: queue Data only through `flush_data`, or check emptiness yourself. If a new harness drains Core Events and merges Data, it needs the same assertion before merging.
