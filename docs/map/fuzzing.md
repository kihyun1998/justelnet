# Fuzzing

## What it is

How the Core's promise that receiving bytes never fails and never panics is backed: the `receive` cargo-fuzz target in `justelnet-core/fuzz`, run by the nightly `Fuzz` workflow, and the rule that a crash it finds lands as a Transcript.

## Governing decisions

**None.** #21 asked for the target; where it runs is the maintainer's call (below). [ADR 0001](../adr/0001-sans-io-core-with-tokio-driver.md)'s sans-IO Core is what makes a byte-only target possible, but decides nothing about fuzzing.

## Design model

From the source, CONTRIBUTING.md and the workflow.

### What it feeds

A Core on the default Option policy gets the input's stream split at lengths the input itself chooses (its first byte, mod 16, is a count; the next bytes the lengths; the rest is fed as one last chunk), so chunk boundaries are fuzzed along with the bytes. After each chunk every Event and outgoing byte is drained, and an empty Data Event fails the run, as it fails a Transcript ([no empty Data Event](no-empty-data-event.md)). Caller calls (`send_*`, requests) and other policies are not fuzzed.

### Where it runs

Only in CI, in its own workflow (`.github/workflows/fuzz.yml`): every night (`17 3 * * *`) for five minutes and on demand (`gh workflow run fuzz.yml --ref <branch>`), on Linux with nightly Rust. A crash uploads `justelnet-core/fuzz/artifacts` as `fuzz-artifacts`. Pull-request CI stays on stable and does not fuzz, since five minutes a run is too long to sit in front of every merge; nor does the fuzz run gate a release (RELEASING.md "Not gates").

The fuzz crate is its own workspace (`[workspace]` in `justelnet-core/fuzz/Cargo.toml`) so the stable workspace never builds libFuzzer, and has its own `Cargo.lock`.

The fuzz target does not run natively on Windows as this repository's machine is set up (probed 2026-09-29): with AddressSanitizer the runtime DLL is not found, with Visual Studio 2019's ASan DLL on the path it fails to initialise, and without a sanitizer the link fails. It does build there (`cargo +nightly fuzz build receive`). #21 asked for the target to build and run locally; the maintainer chose on 2026-09-29 to have the on-demand CI run stand for "locally", over installing a toolchain in WSL.

### A crash becomes a Transcript

CONTRIBUTING.md "A crashing input becomes a Transcript": shrink with `cargo fuzz tmin`, drop the leading count and length bytes (the replay splits every Transcript at arbitrary points already), write the rest as `server:` lines with the `client:` and `event:` lines the fixed Core must produce, and watch it fail on the unfixed Core before the fix goes in.

## Code

- `justelnet-core/fuzz/fuzz_targets/receive.rs` — `fuzz_target!`, `feed`
- `justelnet-core/fuzz/Cargo.toml` — its own workspace, `libfuzzer-sys`
- `.github/workflows/fuzz.yml` — job `receive`
- `CONTRIBUTING.md` — "Fuzzing the Core"

## Reference behaviour

**None.**

## Cross-cutting invariants

- [No empty Data Event](no-empty-data-event.md) — `feed` checks it.

## Blast radius

- [Receive parsing](receive-parsing.md) and [subnegotiation](subnegotiation.md) — the only code the target reaches is `receive`'s parser and the answers it queues; a crash is theirs.
- [Option policy](option-policy.md) — the target runs only the default; a policy-dependent path (Passthrough, a passive start) is not fuzzed.
- [Transcript replay](transcript-replay.md) — a crash fix lands as a Transcript; the input layout's leading bytes must be stripped, which depends on the layout in `receive.rs`'s doc comment.

## Known holes / open

- Caller calls and non-default policies are not fuzzed (above).
- No local run on Windows (above).
