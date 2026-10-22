# Versioning

## What it is

How the three crates are versioned: Semantic Versioning per crate, read as Cargo reads `0.y.z` before 1.0; one shared MSRV; the cascade by which a break in one crate is a break in those that re-export or expose it; `#[non_exhaustive]` enums so additions are minor; the bar for 1.0; and the per-crate changelogs that record all of it.

## Governing decisions

**None** in `docs/adr/`. The policy was decided on #12 (the reasoning is there) and stated in the README's `## Versioning`, which is its home; #68 and #71 refined it, #56 added the changelogs.

## Design model

From the README, CONTRIBUTING.md, the manifests and the issues.

### The policy and its home

#12's MSRV, semver and 1.0 policy is stated in the README's `## Versioning`, the readme of all three crates (`readme.workspace`), so crates.io shows it on each; #12 keeps the reasoning. Its text gives no MSRV number: `rust-version` (root `Cargo.toml`, `workspace.package`, 1.94 at `9a09008`) and the badge hold the value, and the badge links to the section. The badge is a hand copy; CI reads `rust-version` ([CI gates](ci-gates.md)). The README as its home, all of #12 including the 1.0 bar, and no number in the text are the maintainer's calls, made on 2026-10-12 in #68's triage.

In-page links such as `#versioning` work on crates.io as on GitHub: crates.io renders heading ids as `user-content-*` and keeps `href="#…"` links, as tokio's README shows (its 1.47.1 readme page, read 2026-10-12).

### The cascade

`justelnet` re-exports `justelnet-core` (`pub use justelnet_core as core`), so a breaking Core release is a breaking `justelnet` release; `justelnet-expect` exposes `justelnet` types (`Client`, `justelnet::Error` inside `Error::Client`), so a breaking `justelnet` release is a breaking `justelnet-expect` release. Non-breaking releases do not cascade. The manifests carry it: `justelnet` requires `justelnet-core = "0.1.0"`, `justelnet-expect` requires `justelnet = "0.1.0"` (path plus version).

### What is breaking

Public enums are `#[non_exhaustive]`, and configuration such as the Option policy has private fields and is built with a builder or `Default`. Adding an Event, a Command, an error kind or a setting is therefore a minor release, and a `match` on these needs a `_` arm. The marking is on the enums, not on their variants, so a new field in a variant is breaking; every public variant's fields are exhaustive (checked 2026-10-12, and again at `9a09008`), and `Event::OptionChanged` is named as the deliberate case ([option negotiation](option-negotiation.md)). Auto traits are surface too: `Expect` keeps `Send + Sync` ([session log](session-log.md)).

### Before 1.0

The policy in #12 speaks in 1.x terms; the README's `### Before 1.0` maps it onto `0.y.z` the way Cargo does. The Cargo book's SemVer chapter (read 2026-10-12) says `0.y.z` can treat y as major and z as minor, "only changes in the left-most non-zero component are considered incompatible", and that a raised MSRV is "possibly-breaking" but "generally recommended to treat this as a minor change". Breaking changes therefore bump `0.y`, additions and fixes bump `0.y.z`, and so does an MSRV raise, with #12's "never in a patch" taking effect at 1.0, since `0.y.z` has no separate patch level.

Raising the MSRV with `0.y.z` is the maintainer's call, made on 2026-10-12 in #71's triage. They chose it over bumping `0.y`, which is safer for a single crate but costs more here: through the cascade, and with one MSRV shared by all three crates, every raise would become a breaking release of all three. What softens a raise in `0.y.z` is the depending side's resolver, not this workspace's: with `resolver.incompatible-rust-versions = fallback`, the default from resolver 3 (edition 2024, Rust 1.84+), Cargo prefers a dependency version whose `rust-version` the depending package supports, so such a package stays on the previous `0.y.z` (Cargo book, resolver chapter, read 2026-10-12). It is a preference made when resolving, not a guarantee: an older resolver, or a lockfile update that asks for the new version, still picks it.

### 1.0

`justelnet-core` reaches 1.0 first, once the Terminal client that is justelnet's first consumer has shipped on `justelnet` and the Core has gone about three months without a breaking change; `justelnet` and `justelnet-expect` separately, by the same bar (README).

### Changelogs

#12's MSRV policy says a raised MSRV "is called out in the changelog"; #56 added one per crate, `<crate>/CHANGELOG.md`, so `cargo package` ships each with its crate. The format is Keep a Changelog 2.0.0 (read on keepachangelog.com and in its own `CHANGELOG.md`, 2026-10-12): the same format as 1.1.0, with guidance this project uses, a `**Breaking:**` marker kept inside its type, and a dependency named only by the effect its users see, which is how a `justelnet-core` break reaching a re-exporting crate is written. CONTRIBUTING.md "Changelogs" is the rule a pull request follows: a line under `## [Unreleased]` for what users notice, a cascaded break written in each affected crate's changelog, a raised MSRV under `Changed` in all three. Entries are hand-written; nothing checks them in CI.

2.0.0 advises a product split into separate libraries to keep one central changelog besides the per-component ones. There is none here: the three crates are versioned independently (#5), so no release spans them all, and most users read only `justelnet`'s. Per-crate files only and 2.0.0 over 1.1.0 are the maintainer's calls, made on 2026-10-12 with that advice in front of them.

Each changelog's link references point at its crate's tags (`<crate>-v<version>`), which the publish procedure creates ([releasing](releasing.md)), so they resolve only once a version is tagged.

## Code

- `README.md` — `## Versioning` (`### Minimum supported Rust version`, `### Breaking changes`, `### Before 1.0`, `### 1.0`), the MSRV badge
- `Cargo.toml` — `workspace.package.rust-version`; each crate's `version` and its dependency's version requirement
- `justelnet/src/lib.rs` — `pub use justelnet_core as core`
- `justelnet-core/CHANGELOG.md`, `justelnet/CHANGELOG.md`, `justelnet-expect/CHANGELOG.md`
- `CONTRIBUTING.md` — "Changelogs"

## Reference behaviour

- Cargo book, SemVer and resolver chapters (read 2026-10-12); Keep a Changelog 2.0.0 (read 2026-10-12).

## Cross-cutting invariants

**None.**

## Blast radius

- [Releasing](releasing.md) — a release bumps each crate's version and its dependents' requirements by this policy, and checks the changelogs.
- [CI gates](ci-gates.md) — raising `rust-version` changes the MSRV job's toolchain; the badge must be edited by hand.
- [Protocol core](protocol-core.md), [driving a stream](driving-a-stream.md), [waiting for patterns](waiting-for-patterns.md) — any public type change there is classified here (a variant field, an auto trait, a re-export).

## Known holes / open

- The MSRV badge is a hand copy of `rust-version`; nothing checks they agree.
- Changelog entries are not checked in CI (by design, CONTRIBUTING.md).
