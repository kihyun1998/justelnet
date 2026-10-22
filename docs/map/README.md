# Territory map

<!-- grill-map build stamp: e1ef9ba -->
<!-- grill-map synced at: 9a09008 (2026-10-22) -->

justelnet's linked note graph. It answers the two questions that gate a change:

- **If I touch this, what else moves?** Open the territory, read `## Blast radius` — a checklist; an item with nothing to do is a correct outcome — then follow `## Cross-cutting invariants`.
- **What decision is this shape derived from?** Read `## Governing decisions`, then the dated "maintainer's call" lines in `## Design model`. A **None.** under a heading is itself the answer.

A comment's why, trap and measured value go to a note here (CLAUDE.md, "Comments").

## The system in one paragraph

A Telnet client in three crates. The **Core** (`justelnet-core`, sans-IO, [ADR 0001](../adr/0001-sans-io-core-with-tokio-driver.md)) parses received bytes into Events, negotiates options by RFC 1143 from an **Option policy**, answers TTYPE, NAWS and NEW-ENVIRON itself or passes a subnegotiation through, and encodes what the caller sends. The **Driver** (`justelnet`, `Client`) binds a Core to a tokio stream and owns every timeout. The **Expect session** (`justelnet-expect`) waits for patterns and logs. It is proven by **Transcript** replay, nightly fuzzing and a real-telnetd login in CI, and shipped as three independently versioned crates, one tag each. The largest gap: no bytes from network gear (Cisco IOS, Juniper) — every non-Unix target row rests on documentation (#9). What a capture would test is spread across notes: the default stances ([option policy](option-policy.md)), the end of line and BRK/IP for reverse-telnet lines ([sending](sending.md)), `--More--` erased by backspaces ([waiting for patterns](waiting-for-patterns.md)), and the capture rules themselves ([transcript replay](transcript-replay.md)).

## Territories

The Core — [aggregate](protocol-core.md):

- [Receive parsing](receive-parsing.md)
- [Option negotiation](option-negotiation.md)
- [Option policy](option-policy.md)
- [Subnegotiation](subnegotiation.md)
- [Sending](sending.md)

The Driver and the Expect session:

- [Driving a stream](driving-a-stream.md)
- [Waiting for patterns](waiting-for-patterns.md)
- [Session log](session-log.md)

Proving it:

- [Transcript replay](transcript-replay.md)
- [Fuzzing](fuzzing.md)
- [Logging in to a real server](logging-in-to-a-real-server.md)

Shipping it:

- [CI gates](ci-gates.md)
- [Versioning](versioning.md)
- [Releasing](releasing.md)

## Cross-cutting invariants

Facts that hold at several sites that do not call each other:

- [No empty Data Event](no-empty-data-event.md)
- [Every Event has a Transcript spelling](every-event-has-a-transcript-spelling.md)
- [Socket setup matches connect](socket-setup-matches-connect.md)

## Conventions

- Every note of a kind has the same headings; an empty one says **None.**, so `rg -lU '## Governing decisions\r?\n(\r?\n)?\*\*None\.\*\*' docs/map/` lists the areas no record governs.
- Code is named by symbol (`file — Type::method`), never by line.
- Every claim carries its evidence (a symbol, a commit, a Transcript) or its source and date — "the maintainer's call, made on 2026-09-29" is a stated reason, not a checked one.
- A fact lives in one note; others link to it.
- The layout is flat: territories, the aggregate and invariants side by side, invariants named for their fact.

## What the map cannot answer

Source files, issues, Transcripts and research docs are text inside notes, not nodes: the map says which note mentions `Core::received` or #9, not everything that does. Use `rg` on the code for that.

## Scope

Mapped: the three crates, their tests, `justelnet-core/fuzz`, `ci/`, `.github/workflows/`, `docs/transcripts.md`, `RELEASING.md`, `CONTRIBUTING.md`, the README's Versioning.

Not mapped, by reason:

- `docs/research/` and `docs/prototypes/` — sources the notes cite (the target research, libtelnet vs russh, the Expect prototype), not areas of the system.
- `docs/agents/` — configuration for the engineering skills.
- `logo/` — artwork; the one way it touches the system, the crates' docs pointing at it by URL, is in [releasing](releasing.md).
- `target/` — build output, untracked.
- The **Terminal client**, justelnet's first consumer (GLOSSARY.md), is not built yet (maintainer, 2026-10-22); its call site is unmapped, and the 1.0 bar waits on it ([versioning](versioning.md)).
- Server role, sync API, TLS — out of scope per ADR 0001, attachable without touching the Core.

**What an absent note means.** A territory appears when something in the repo commits to it: code, a decision record, or an open issue with a design. No note for, say, a TLS Driver is correct today. A new crate, entry point or top-level folder with no note is a missing note.

## Using the map

- **Before designing a change**, open the territory it lands in and read its blast radius and invariants.
- **After it lands**, update the notes its change falsified, and run `grill-map` to resync the stamps.
- **At the first fix that reveals a fact**, ask whether it holds at every site that shares the assumption; if yes, it lands with an invariant note.
