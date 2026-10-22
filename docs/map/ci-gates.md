# CI gates

## What it is

What every push to `main` and every pull request must pass: the `CI` workflow's five jobs — formatting, clippy with warnings denied and the whole test suite on three OSes at stable, the test suite at the MSRV, and the real-telnetd login. These job names are also the release checklist's gates.

## Governing decisions

**None.** #24 named the gates as kinds of test; RELEASING.md (#36) lists the jobs.

## Design model

From `.github/workflows/ci.yml` and RELEASING.md.

| Job (exact name) | Runs |
|---|---|
| `test (ubuntu-latest, stable)`, `test (windows-latest, stable)`, `test (macos-latest, stable)` | `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --locked` |
| `test (ubuntu-latest, MSRV)` | `cargo test --workspace --locked` at the toolchain read from the workspace `rust-version` |
| `e2e (ubuntu-latest, inetutils telnetd)` | [logging in to a real server](logging-in-to-a-real-server.md) |

### The gates are jobs, not test suites

#24 names its gates as kinds of test (Transcripts, property tests, fake-device tests, the real-telnetd job), but only the real-telnetd job is a CI job of its own; the Transcripts and the property test run inside `cargo test --workspace` in every `test (…)` job. RELEASING.md lists the five job names exactly as the `CI` workflow reports them (checked against main's run of 2026-10-07) and maps each gate to the jobs that run it. A renamed job has to be renamed there too — the job names are a hand copy (the `name:` keys in `ci.yml` and the checkboxes in RELEASING.md).

### The MSRV is read, not written

The MSRV job takes its toolchain from `rust-version` in the root `Cargo.toml` (`sed` in the `msrv` step), so raising the MSRV needs no workflow edit; the README badge is a hand copy ([versioning](versioning.md)).

### What does not run here

`--locked` everywhere, so a stale `Cargo.lock` fails CI. Fuzzing runs only in its own nightly workflow ([fuzzing](fuzzing.md)). `cargo package --workspace` and the include grep are release checks, not CI ([releasing](releasing.md)). No link check runs on docs or the map.

## Code

- `.github/workflows/ci.yml` — jobs `test` (matrix `os`), `msrv`, `e2e`
- `RELEASING.md` — "Required: these CI jobs are green"

## Reference behaviour

**None.**

## Cross-cutting invariants

**None.**

## Blast radius

- [Releasing](releasing.md) — RELEASING.md names these jobs by their exact names; renaming one breaks the checklist silently.
- [Versioning](versioning.md) — the MSRV job reads `rust-version`; the badge does not.
- [Transcript replay](transcript-replay.md) — runs inside the `test (…)` jobs, at the MSRV too; a Transcript feature the MSRV toolchain lacks fails only there.
- [Logging in to a real server](logging-in-to-a-real-server.md) — the `e2e` job's steps (container, wrong password, artifact) live in this workflow.

## Known holes / open

- No link check covers docs or `docs/map/`; a broken note link is found only by reading.
- The job names are copied by hand into RELEASING.md (above).
