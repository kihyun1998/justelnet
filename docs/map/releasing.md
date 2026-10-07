# Releasing

What gates a publish to crates.io: `RELEASING.md`, the checklist #36 asked for under #24.

## The gates are jobs, not test suites

#24 names its gates as kinds of test (Transcripts, property tests, fake-device tests, the real-telnetd job), but only the real-telnetd job is a CI job of its own; the Transcripts and the property test run inside `cargo test --workspace` in every `test (…)` job. The checklist lists the five job names exactly as the `CI` workflow reports them (checked against main's run of 2026-10-07) and maps each gate to the jobs that run it. A renamed job has to be renamed there too.

## Packaging, and what it does not catch

Measured on 2026-10-07 with cargo 1.96, before anything was published:

- `cargo package -p justelnet` fails with `no matching package named justelnet-core found`: a crate's path dependencies must already be on crates.io. `cargo package --workspace` packages all three and verifies each against a temporary local registry, and passes.
- That verification builds only the libraries. It passed while `justelnet/tests/cancel_safety.rs` includes `../../justelnet-core/tests/transcripts/telnetd-netkit-rhel9.txt`, a file the published `justelnet` crate will not contain (#26's finding, carried on #36). So the checklist adds a grep for includes that reach outside a crate, which finds exactly that line.

The checklist failed on main on 2026-10-07 until #55 gave that test its own copy of the bytes; after it, the grep prints nothing.

## Calls

The checklist covers the gates plus the two packaging checks, not version bumps or the publish order; that scope is the maintainer's call, made on 2026-10-07 over a gates-only list and a full publish procedure. It lives in `RELEASING.md` at the root rather than in `CONTRIBUTING.md` or under `docs/`, also the maintainer's call that day. Both are theirs to reverse.
