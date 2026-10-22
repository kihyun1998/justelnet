# Releasing

What must be true before a release of justelnet is published to crates.io, starting with 0.1. Every box is checked on the commit being released.

## Required: these CI jobs are green

In the `CI` workflow, on the commit being released:

- [ ] `test (ubuntu-latest, stable)`
- [ ] `test (windows-latest, stable)`
- [ ] `test (macos-latest, stable)`
- [ ] `test (ubuntu-latest, MSRV)`
- [ ] `e2e (ubuntu-latest, inetutils telnetd)`

What each gate is, and the job that runs it:

| Gate | Where it runs |
|---|---|
| Transcripts (`justelnet-core/tests/transcripts`) and the chunking property test (`justelnet-core/tests/chunking.rs`) | `cargo test --workspace` in every `test (…)` job |
| Fake-device tests of the Driver and the Expect session, on Linux, Windows, macOS and the MSRV | `cargo test --workspace` in the four `test (…)` jobs |
| A login to a real telnetd (GNU inetutils telnetd 2.8) | `e2e (ubuntu-latest, inetutils telnetd)` |

## Required: the crates package

On the same commit:

- [ ] `cargo package --workspace` succeeds. It packages the three crates together, so the dependent crates resolve before any of them is on crates.io, and builds each library from its packaged files. `cargo package -p <crate>` fails until its dependencies are published.
- [ ] No packaged test reads a file outside its own crate; this prints nothing:

  ```sh
  grep -rnE 'include_(str|bytes)!\("\.\./\.\.' --include=*.rs justelnet-core justelnet justelnet-expect
  ```

  `cargo package` does not build tests, so it passes even when a packaged test would not compile from the published crate.

## Required: the changelogs

- [ ] Each crate being published has a section for the version being released in its `CHANGELOG.md`, dated, moved out of `Unreleased`, and naming any raise of the MSRV (see [CONTRIBUTING.md](CONTRIBUTING.md#changelogs)).

## Publishing

A crate is published by pushing its tag, `<crate>-v<version>`: the `Release`
workflow checks that the tag matches the crate's `Cargo.toml` and is on
`main`, then publishes that crate with a short-lived token from crates.io's
Trusted Publishing. No API token is stored in the repository.

Once every box above is checked on the commit to release, on `main`:

1. Each crate being released has its new version in its `Cargo.toml`, and the
   crates depending on it require that version (`justelnet` on
   `justelnet-core`, `justelnet-expect` on `justelnet`). Its `CHANGELOG.md`
   heading carries the version and the publish date, with an empty
   `Unreleased` above it. The versions follow the
   [README's Versioning](README.md#versioning), including the cascade.
2. `cargo publish --workspace --dry-run` passes.
3. Push the tags one at a time in dependency order (`justelnet-core`,
   `justelnet`, `justelnet-expect`), each after the previous `Release` run
   has finished, since a crate cannot be published before the version it
   depends on is on crates.io:

   ```sh
   git tag justelnet-core-v0.2.0
   git push origin justelnet-core-v0.2.0
   ```

   A tag whose version is already on crates.io passes without publishing.

### The first release, 0.1.0

Trusted Publishing can only be set up for a crate that already exists on
crates.io, so 0.1.0 is published by hand with an API token:

1. Steps 1 and 2 above.
2. `cargo login` with a crates.io API token scoped to publishing new crates,
   then `cargo publish --workspace`, which publishes the three in dependency
   order. Revoke the token afterwards.
3. On crates.io, in each crate's Settings → Trusted Publishing, add GitHub
   with owner `kihyun1998`, repository `justelnet`, workflow `release.yml` and
   environment `release`.
4. Push the three `-v0.1.0` tags. Their `Release` runs find 0.1.0 already
   published and pass, which also checks the workflow.

## Not gates

These do not block a release:

- Device captures from real network equipment ([#9](https://github.com/kihyun1998/justelnet/issues/9)).
- The nightly `Fuzz` workflow (`fuzz receive (nightly)`).
