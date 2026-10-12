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

The checklist covers the gates plus the two packaging checks, not version bumps or the publish order; that scope is the maintainer's call, made on 2026-10-07 over a gates-only list and a full publish procedure. On 2026-10-12 (#56) they widened it by one item, the changelog entry of each crate being published; version bumps and the publish order stay out. It lives in `RELEASING.md` at the root rather than in `CONTRIBUTING.md` or under `docs/`, also the maintainer's call that day. Both are theirs to reverse.

## Changelogs

#12's MSRV policy says a raised MSRV "is called out in the changelog"; #56 added one per crate, `<crate>/CHANGELOG.md`, so `cargo package` ships each with its crate. The format is Keep a Changelog 2.0.0 (read on keepachangelog.com and in its own `CHANGELOG.md`, 2026-10-12): the same format as 1.1.0, with guidance this project uses, a `**Breaking:**` marker kept inside its type, and a dependency named only by the effect its users see, which is how a `justelnet-core` break reaching a re-exporting crate is written.

2.0.0 advises a product split into separate libraries to keep one central changelog besides the per-component ones. There is none here: the three crates are versioned independently (#5), so no release spans them all, and most users read only `justelnet`'s. Per-crate files only and 2.0.0 over 1.1.0 are the maintainer's calls, made on 2026-10-12 with that advice in front of them, and theirs to reverse.

The `[Unreleased]` heading has no link reference: nothing is tagged yet, and how per-crate tags are named belongs with the publish procedure, which the checklist leaves out.

## The versioning policy

#12's MSRV, semver and 1.0 policy is stated in the README's `## Versioning`, the readme of all three crates, so crates.io shows it on each; #12 keeps the reasoning. Its text gives no MSRV number: `rust-version` and the badge hold the value, and the badge links to the section. It says the `#[non_exhaustive]` marking is on the enums, not their variants, so a new field in a variant is breaking; every public variant's fields are exhaustive (checked 2026-10-12), and `Event::OptionChanged` is named as the deliberate case (see `option-negotiation.md`). The README as its home, all of #12 including the 1.0 bar, and no number in the text are the maintainer's calls, made on 2026-10-12 in #68's triage; they are theirs to reverse.

In-page links such as `#versioning` work on crates.io as on GitHub: crates.io renders heading ids as `user-content-*` and keeps `href="#…"` links, as tokio's README shows (its 1.47.1 readme page, read 2026-10-12).
