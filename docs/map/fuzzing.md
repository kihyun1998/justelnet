# Fuzzing

How the Core's promise that receiving bytes never panics is backed: the `receive` cargo-fuzz target in `justelnet-core/fuzz`, run by the `Fuzz` workflow.

## Where it runs

Only in CI, in its own workflow: every night for five minutes and on demand, on Linux with nightly Rust. Pull-request CI stays on stable and does not fuzz, since five minutes a run is too long to sit in front of every merge.

The fuzz target does not run natively on Windows as this repository's machine is set up (probed 2026-09-29): with AddressSanitizer the runtime DLL is not found, with Visual Studio 2019's ASan DLL on the path it fails to initialise, and without a sanitizer the link fails. It does build there (`cargo fuzz build`). #21 asked for the target to build and run locally; the maintainer chose on 2026-09-29 to have the on-demand CI run stand for "locally", over installing a toolchain in WSL, and it is theirs to reverse.

## What it feeds

A Core on the default Option policy gets the input's stream split at lengths the input itself chooses (its first byte is a count, the next bytes the lengths), so chunk boundaries are fuzzed along with the bytes. After each chunk every Event and outgoing byte is drained, and an empty Data Event fails the run, as it fails a Transcript. Caller calls (`send_*`, requests) and other policies are not fuzzed.
