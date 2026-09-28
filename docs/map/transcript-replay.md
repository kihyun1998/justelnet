# Transcript replay

How the Core is proven: every behaviour is a Transcript replayed through the Core's public API, once as written and once under arbitrary chunking.

## Why adjacent Data Events are merged before comparing

The Core emits one Data Event per `receive` call. Chunking the same bytes differently therefore changes how many Data Events come out without changing what the peer said. The runner merges adjacent Data Events on both sides, so the chunking property compares meaning, not call boundaries.

## The trap the merge opens

The merge also hides a Core that emits **empty** Data Events: they fold into their neighbours and every comparison stays green. The runner rejects any empty Data Event before merging. This guard was added after a mutation (emitting Data for every `receive`, even with no data) survived both tests without it.

## Steps, not one stream

A Transcript is a sequence of steps, each opened by a `server:` line. Lines before the first `server:` are a step with no server bytes, which is where output the Core produces before any input (an active start) is expected. The chunking property splits each step's server bytes independently, so anything a later directive does between steps keeps its position.

## The option-state query is checked on every step

After every call and every chunk, the runner compares `is_enabled` for all 256 options on both sides with what the OptionChanged Events so far say. The acceptance criterion "the option-state query agrees with OptionChanged" is then asserted by every Transcript, including in states no Event marks, such as a request waiting for its answer (`WANTYES`).

The cost is measured: the chunking property went from 0.2 s to 4.75 s (debug build, Windows, seven Transcripts).
