<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/kihyun1998/justelnet/main/logo/readme/justelnet-readme-dark.png">
    <img alt="justelnet" src="https://raw.githubusercontent.com/kihyun1998/justelnet/main/logo/readme/justelnet-readme-light.png" width="480">
  </picture>
</p>

<p align="center">
  A Rust Telnet client library: a protocol core that follows the Telnet RFCs,<br>
  with a tokio client and expect-style automation on top.
</p>

<p align="center">
  <a href="https://github.com/kihyun1998/justelnet/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/kihyun1998/justelnet/actions/workflows/ci.yml/badge.svg"></a>
  <a href="#versioning"><img alt="MSRV 1.94" src="https://img.shields.io/badge/MSRV-1.94-blue"></a>
  <img alt="License: MIT OR Apache-2.0" src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue">
</p>

> **Status:** early development, not yet published to crates.io. The Core
> and the tokio Driver are complete; the Expect session is in progress (see
> the [0.1 map](https://github.com/kihyun1998/justelnet/issues/1)).

## Crates

| Crate | What it is |
|---|---|
| [`justelnet-core`](justelnet-core) | Sans-IO Telnet protocol Core: bytes in, Events and outgoing bytes out. No I/O, no clock. |
| [`justelnet`](justelnet) | Telnet client for tokio: a Driver that binds a Core to an async stream and owns every timeout. |
| [`justelnet-expect`](justelnet-expect) | Expect-style automation: wait for patterns in the incoming data. |

## Design

The Core holds one connection's Telnet state and performs no I/O. The caller
feeds it received bytes, then pulls Events and the bytes to send back out of it.
It answers the peer's option negotiation on its own from an **Option policy**,
so a login cannot stall because the caller forgot to reply, and because it has
no clock, recorded server conversations (**Transcripts**) replay against it
deterministically. See [ADR 0001](docs/adr/0001-sans-io-core-with-tokio-driver.md).

```rust
use justelnet_core::{Core, Event};

let mut core = Core::new(); // default policy: a character-mode terminal client
let mut outgoing = Vec::new();

// Bytes read from the socket: "IAC DO SGA" followed by a login prompt.
core.receive(b"\xff\xfd\x03login: ");

while let Some(event) = core.poll_event() {
    match event {
        Event::Data(bytes) => print!("{}", String::from_utf8_lossy(&bytes)),
        Event::OptionChanged { option, side, enabled } => {
            println!("{option:?} on {side:?}: {enabled}")
        }
        _ => {}
    }
}

// Bytes to write to the socket: here, the policy's opening option requests.
// DO SGA answered our own WILL SGA, so it needs no reply.
core.poll_transmit(&mut outgoing);
```

## Versioning

The three crates are versioned independently and follow
[Semantic Versioning](https://semver.org). The policy below was decided in
[#12](https://github.com/kihyun1998/justelnet/issues/12), which has the reasoning.

### Minimum supported Rust version

All three crates share one MSRV: a Rust release at least six months old,
declared as `rust-version` in each `Cargo.toml`, shown by the badge above and
checked in CI. Raising it is not a breaking change, but it happens only in a
minor release, never a patch, and is noted in each crate's changelog.

### Breaking changes

- `justelnet` re-exports `justelnet-core`, so a breaking release of the core is
  a breaking release of `justelnet`.
- `justelnet-expect` exposes `justelnet` types, so a breaking release of
  `justelnet` is a breaking release of `justelnet-expect`.
- Non-breaking releases do not cascade.

Public enums are `#[non_exhaustive]`, and configuration such as the Option
policy has private fields and is built with a builder or `Default`. Adding an
Event, a Command, an error kind or a setting is therefore a minor release, and a
`match` on these needs a `_` arm. The marking is on the enums, not on their
variants, so adding a field to an existing variant is a breaking change; for
`Event::OptionChanged` that is deliberate, since `option`, `side` and `enabled`
are a closed set and a pattern may name them all.

### 1.0

`justelnet-core` reaches 1.0 first, once the terminal client that is
justelnet's first consumer has shipped on `justelnet` and the Core has gone
about three months without a breaking change. `justelnet` and
`justelnet-expect` reach 1.0 separately, by the same bar.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this project by you, as defined in the Apache-2.0 license,
shall be dual licensed as above, without any additional terms or conditions.
