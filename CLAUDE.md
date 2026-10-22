## Agent skills

### Issue tracker

Issues are tracked in GitHub Issues for `kihyun1998/justelnet` (via the `gh` CLI). See `docs/agents/issue-tracker.md`.

### Triage labels

Default vocabulary: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `GLOSSARY.md` + `docs/adr/` at the repo root (created lazily). See `docs/agents/domain.md`.

### thegraph

What this project is and which outside sources it is built against. See `docs/agents/thegraph.md`.

## Territory map

The map is [docs/map/README.md](docs/map/README.md): one note per area of what the system does, with what each change moves and why it is shaped as it is.

- **Read before the change:** in `read-it`, before a design is committed (or before designing any change), open the territory the change lands in and walk its `## Blast radius` and `## Cross-cutting invariants`.
- **Write after it:** in `sweep`, once the change lands, update every note it falsified; the hub's `grill-map synced at` line is advanced by `grill-map`.
- **Promotion:** when a fix reveals a fact, ask "does this hold at every site that shares the same assumption?" — if yes, the fix lands with an invariant note.

## Comments

A comment says what the code is. Why it is this way, what it deliberately leaves out, the trap and the measured value go to the territory note under `docs/map/`. Where none seems to hold it, search the folder and the code symbol first; only then start a new note, named for what the system does there, not for the file. History goes to the commit message. Comments written before this rule still carry the rest: never delete one whose content the map does not yet hold — move it first (`decant`).
