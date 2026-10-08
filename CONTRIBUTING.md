# Contributing

## Fuzzing the Core

`justelnet-core/fuzz` holds a cargo-fuzz target, `receive`, that feeds arbitrary bytes, split at arbitrary points, into a Core on the default Option policy. It backs the Core's promise that receiving bytes never fails and never panics.

It needs nightly Rust and [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz), and runs on Linux or macOS; on Windows it builds (`cargo +nightly fuzz build receive`) but does not run, so use the workflow there:

```sh
cargo install cargo-fuzz
cd justelnet-core
cargo +nightly fuzz run receive -- -max_total_time=60
```

The `Fuzz` workflow runs it for five minutes every night, and on demand (`gh workflow run fuzz.yml --ref <branch>`); pull-request CI stays on stable and does not fuzz. When it finds a crash, the input is uploaded as the `fuzz-artifacts` artifact.

### A crashing input becomes a Transcript

A fix for a crash lands with the crashing input as a Transcript under `justelnet-core/tests/transcripts/`, so it replays on every pull request from then on:

1. Take the input (from `fuzz/artifacts/receive/` or the workflow artifact), shrink it with `cargo +nightly fuzz tmin receive <file>`, and see what it holds: `cargo +nightly fuzz fmt receive <file>`, or a hex dump.
2. Its first byte is the number of chunk lengths that follow; those bytes are the lengths, and the rest is what the peer sent. The Transcript needs only that rest: the replay splits every Transcript at arbitrary points already.
3. Write it as `server:` lines with the `client:` and `event:` lines the fixed Core must produce, starting with `policy:` lines only if the crash needs a policy other than the default. The format is [docs/transcripts.md](docs/transcripts.md).
4. Watch the Transcript fail on the unfixed Core before the fix goes in.

## Logging in to a real telnetd

`justelnet-expect/tests/real_telnetd.rs` logs in to GNU inetutils telnetd 2.8, runs one command and logs out. It is ignored by default and needs the container in `ci/inetutils-telnetd` and Docker:

```sh
docker build -t justelnet-telnetd ci/inetutils-telnetd
docker run -d --name telnetd --hostname e2e -p 127.0.0.1:2323:23 justelnet-telnetd
JUSTELNET_E2E_ADDR=127.0.0.1:2323 cargo test -p justelnet-expect --test real_telnetd -- --ignored
```

Set `JUSTELNET_E2E_RECORDING=<file>` to keep the bytes each side sent. The `e2e` job in the `CI` workflow runs it on every pull request, also with a wrong password, which must fail, and uploads those bytes as the `inetutils-telnetd-recording` artifact.
