# Releasing

## What it is

What gates a publish to crates.io and how it happens: `RELEASING.md`'s checklist (the CI jobs green, the crates package, the changelogs written) and the publish procedure — a tag per crate, `<crate>-v<version>`, pushed in dependency order, published by the `Release` workflow through crates.io Trusted Publishing.

## Governing decisions

**None** in `docs/adr/`. The checklist was asked for by #36 under #24; its widening (#56, the 0.1 preparation) is the maintainer's, dated below. What a version number means is [versioning](versioning.md).

## Design model

From RELEASING.md, `.github/workflows/release.yml` and measurements before 0.1.

### What the checklist covers

The gates ([CI gates](ci-gates.md) by job name), the two packaging checks, each crate's changelog entry, and the publish procedure. On 2026-10-07 the maintainer scoped it to the gates plus the two packaging checks, over a gates-only list and a full publish procedure. On 2026-10-12 (#56) they widened it by one item, the changelog entry of each crate being published. Later that day, preparing 0.1, they widened it again to the publish procedure, with a tag per crate named `<crate>-v<version>`, chosen over one tag for all three, which would not fit once the versions part. Publishing is by tag through the `Release` workflow with crates.io Trusted Publishing, and 0.1.0 by hand, chosen over a long-lived `CARGO_REGISTRY_TOKEN` in a repository secret and over publishing by hand every time. It lives in `RELEASING.md` at the root rather than in `CONTRIBUTING.md` or under `docs/`, also the maintainer's call that day. All are theirs to reverse.

### Packaging, and what it does not catch

Measured on 2026-10-07 with cargo 1.96, before anything was published:

- `cargo package -p justelnet` fails with `no matching package named justelnet-core found`: a crate's path dependencies must already be on crates.io. `cargo package --workspace` packages all three and verifies each against a temporary local registry, and passes.
- That verification builds only the libraries. It passed while `justelnet/tests/cancel_safety.rs` included `../../justelnet-core/tests/transcripts/telnetd-netkit-rhel9.txt`, a file the published `justelnet` crate does not contain (#26's finding, carried on #36). So the checklist adds a grep for includes that reach outside a crate (`include_(str|bytes)!("../..`), which found exactly that line.

The checklist failed on main on 2026-10-07 until #55 gave that test its own copy of the bytes ([driving a stream](driving-a-stream.md)); after it, the grep prints nothing (re-run at `9a09008`, 2026-10-22).

### Publishing by tag

crates.io's Trusted Publishing (its docs page source, `svelte/src/routes/docs/trusted-publishing/+page.svelte` in rust-lang/crates.io, read 2026-10-12) gives a GitHub workflow with `id-token: write` a 30-minute token through `rust-lang/crates-io-auth-action@v1`, and is configured per crate with the repository, the workflow filename and an optional environment. It requires the crate to exist already ("initial publish requires an API token"), which is why 0.1.0 went out by hand.

`release.yml` publishes one crate per tag. It reads the crate and version from the tag, fails unless they match the crate's `Cargo.toml` (`cargo metadata`) and the tagged commit is on `origin/main`, and skips the publish when crates.io already has that version (HTTP 200 from its API). Tags are pushed one at a time in dependency order rather than all at once: three tag runs in parallel could publish `justelnet` before `justelnet-core` is on crates.io, and a workflow `concurrency` group would not queue them, since it keeps one pending run and cancels the rest. A tag whose version is already on crates.io passes without publishing, so the 0.1.0 tags pushed after the manual publish exercised the workflow without uploading. The `release` environment is named in both the workflow and the crates.io configuration; protection rules on it, such as a required reviewer, are the maintainer's to add in the repository settings.

### Publishing 0.1

Measured on 2026-10-12 with cargo 1.96: `cargo publish --workspace --dry-run` packages, verifies and would upload all three crates in dependency order before any of them is on crates.io, as `cargo package --workspace` does. The names `justelnet`, `justelnet-core` and `justelnet-expect` were free on crates.io that day (its API answered 404 for each). The keywords and categories in the manifests are the maintainer's choice of that day; the category slugs were checked against crates.io's list. 0.1.0 of all three and `justelnet-expect` 0.1.1 are published (README status, changelogs).

### What the published crates point back at

The rustdoc of all three crates sets `html_logo_url` and `html_favicon_url` to raw GitHub URLs under `logo/` on `main`, and the README's images point there too; moving or renaming those files breaks docs.rs and crates.io pages of versions already published.

## Code

- `RELEASING.md`
- `.github/workflows/release.yml` — job `publish` (steps `tag`, `published`, `auth`, `cargo publish`)
- `justelnet-core/src/lib.rs`, `justelnet/src/lib.rs`, `justelnet-expect/src/lib.rs` — `#![doc(html_logo_url, html_favicon_url)]`
- each crate's `Cargo.toml` — `version`, `keywords`, `categories`

## Reference behaviour

- crates.io Trusted Publishing docs source (read 2026-10-12).

## Cross-cutting invariants

**None.**

## Blast radius

- [Versioning](versioning.md) — the bump rules and the changelog format the checklist checks.
- [CI gates](ci-gates.md) — the checklist names the CI jobs by exact name.
- [Driving a stream](driving-a-stream.md) and every crate's tests — a test that reads a file outside its own crate passes `cargo package` and breaks the published crate's tests; the include grep is the only check.
- [Transcript replay](transcript-replay.md) — Transcripts ship inside `justelnet-core`'s package; another crate's tests cannot include them.

## Known holes / open

- RELEASING.md "Publishing" step 4 still says to take "not yet published to crates.io" out of the README status line, which was done by commit `61a56a0` (#74); the step is now a no-op for every release.
- No reviewer protection on the `release` environment unless the maintainer adds it (above).
