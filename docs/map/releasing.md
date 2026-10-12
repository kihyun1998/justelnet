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

The checklist covers the gates plus the two packaging checks, not version bumps or the publish order; that scope is the maintainer's call, made on 2026-10-07 over a gates-only list and a full publish procedure. On 2026-10-12 (#56) they widened it by one item, the changelog entry of each crate being published. Later that day, preparing 0.1, they widened it again to the publish procedure, with a tag per crate named `<crate>-v<version>`, chosen over one tag for all three, which would not fit once the versions part. Publishing is by tag through the `Release` workflow with crates.io Trusted Publishing, and 0.1.0 by hand, chosen over a long-lived `CARGO_REGISTRY_TOKEN` in a repository secret and over publishing by hand every time. Both are theirs to reverse. It lives in `RELEASING.md` at the root rather than in `CONTRIBUTING.md` or under `docs/`, also the maintainer's call that day. Both are theirs to reverse.

## Changelogs

#12's MSRV policy says a raised MSRV "is called out in the changelog"; #56 added one per crate, `<crate>/CHANGELOG.md`, so `cargo package` ships each with its crate. The format is Keep a Changelog 2.0.0 (read on keepachangelog.com and in its own `CHANGELOG.md`, 2026-10-12): the same format as 1.1.0, with guidance this project uses, a `**Breaking:**` marker kept inside its type, and a dependency named only by the effect its users see, which is how a `justelnet-core` break reaching a re-exporting crate is written.

2.0.0 advises a product split into separate libraries to keep one central changelog besides the per-component ones. There is none here: the three crates are versioned independently (#5), so no release spans them all, and most users read only `justelnet`'s. Per-crate files only and 2.0.0 over 1.1.0 are the maintainer's calls, made on 2026-10-12 with that advice in front of them, and theirs to reverse.

Each changelog's link references point at its crate's tags (`<crate>-v<version>`), which the publish procedure creates, so they resolve only once a version is tagged.

## The versioning policy

#12's MSRV, semver and 1.0 policy is stated in the README's `## Versioning`, the readme of all three crates, so crates.io shows it on each; #12 keeps the reasoning. Its text gives no MSRV number: `rust-version` and the badge hold the value, and the badge links to the section. It says the `#[non_exhaustive]` marking is on the enums, not their variants, so a new field in a variant is breaking; every public variant's fields are exhaustive (checked 2026-10-12), and `Event::OptionChanged` is named as the deliberate case (see `option-negotiation.md`). The README as its home, all of #12 including the 1.0 bar, and no number in the text are the maintainer's calls, made on 2026-10-12 in #68's triage; they are theirs to reverse.

In-page links such as `#versioning` work on crates.io as on GitHub: crates.io renders heading ids as `user-content-*` and keeps `href="#…"` links, as tokio's README shows (its 1.47.1 readme page, read 2026-10-12).

## Before 1.0

The policy in #12 speaks in 1.x terms; the README's `### Before 1.0` maps it onto `0.y.z` the way Cargo does. The Cargo book's SemVer chapter (read 2026-10-12) says `0.y.z` can treat y as major and z as minor, "only changes in the left-most non-zero component are considered incompatible", and that a raised MSRV is "possibly-breaking" but "generally recommended to treat this as a minor change". Breaking changes therefore bump `0.y`, additions and fixes bump `0.y.z`, and so does an MSRV raise, with #12's "never in a patch" taking effect at 1.0, since `0.y.z` has no separate patch level.

Raising the MSRV with `0.y.z` is the maintainer's call, made on 2026-10-12 in #71's triage and theirs to reverse. They chose it over bumping `0.y`, which is safer for a single crate but costs more here: through #12's cascade, and with one MSRV shared by all three crates, every raise would become a breaking release of all three. What softens a raise in `0.y.z` is the depending side's resolver, not this workspace's: with `resolver.incompatible-rust-versions = fallback`, the default from resolver 3 (edition 2024, Rust 1.84+), Cargo prefers a dependency version whose `rust-version` the depending package supports, so such a package stays on the previous `0.y.z` (Cargo book, resolver chapter, read 2026-10-12). It is a preference made when resolving, not a guarantee: an older resolver, or a lockfile update that asks for the new version, still picks it.

## Publishing by tag

crates.io's Trusted Publishing (its docs page source, `svelte/src/routes/docs/trusted-publishing/+page.svelte` in rust-lang/crates.io, read 2026-10-12) gives a GitHub workflow with `id-token: write` a 30-minute token through `rust-lang/crates-io-auth-action@v1`, and is configured per crate with the repository, the workflow filename and an optional environment. It requires the crate to exist already ("initial publish requires an API token"), which is why 0.1.0 goes out by hand.

`release.yml` publishes one crate per tag. Tags are pushed one at a time in dependency order rather than all at once: three tag runs in parallel could publish `justelnet` before `justelnet-core` is on crates.io, and a workflow `concurrency` group would not queue them, since it keeps one pending run and cancels the rest. A tag whose version is already on crates.io passes without publishing, so the 0.1.0 tags pushed after the manual publish exercise the workflow without uploading. The `release` environment is named in both the workflow and the crates.io configuration; protection rules on it, such as a required reviewer, are the maintainer's to add in the repository settings.

## Publishing 0.1

Measured on 2026-10-12 with cargo 1.96: `cargo publish --workspace --dry-run` packages, verifies and would upload all three crates in dependency order before any of them is on crates.io, as `cargo package --workspace` does. The names `justelnet`, `justelnet-core` and `justelnet-expect` were free on crates.io that day (its API answered 404 for each). The keywords and categories in the manifests are the maintainer's choice of that day; the category slugs were checked against crates.io's list.
