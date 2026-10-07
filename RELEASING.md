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

## Not gates

These do not block a release:

- Device captures from real network equipment ([#9](https://github.com/kihyun1998/justelnet/issues/9)).
- The nightly `Fuzz` workflow (`fuzz receive (nightly)`).
