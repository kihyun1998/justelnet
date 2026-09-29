# Waiting for patterns

How the Expect session (`justelnet-expect`) turns a client's Data into waits for prompts.

## The prototype is the oracle

The matching rules are the ones walked through in the prototype that settled #8 (`prototypes/expect-api.PROTOTYPE.html` on `prototype/expect-api`, commit `905e3e5`). Its `Expect` module has no DOM, so the tests' expected matches and buffers come from running that module itself under node on the same scenario steps, not from a Rust copy of it that could share the implementation's mistakes. To redo it: take the text from `const Expect = (() => {` to the matching `})();`, evaluate it, and dispatch the scenario's actions (`serverData`, `expect`, `tick`) as the page's `scenarios` list does. Done on 2026-09-29 for scenarios 1, 2 and 5.

Scenario 1 ends with `expect_any([">", "#"])`, which comes with #31; the #30 test waits for `>` alone, which the module gives the same result for.

## What a wait does

- The buffer is searched before any read, so data left by the last match is matched at once (scenario 3's `Username:` after `% Login invalid`).
- Each pattern's first match counts; the earliest start wins, ties going to the pattern listed first. The data ahead of it is `before`, the rest stays buffered.
- An empty literal matches at the start with nothing before it and takes nothing, as the prototype's `new RegExp("")` does.
- The timeout counts from the start of the wait, not from the last data, so a device trickling output without the prompt still times out. The error carries a copy of the buffer and the session keeps its own.
- Data is appended; a clean close fails the wait with `Closed { buffer }`; every other Event is skipped, since the Core already handled it.

Patterns are bytes (a `&str` literal matches its UTF-8 bytes), so an ASCII prompt matches on any encoding. The search runs over the whole buffer on every chunk; the buffer cap (#33) is what keeps that bounded, and resuming the search where the last one could no longer match is the known cheaper shape if it ever shows up in a measurement.

## Handing the client back

`into_inner` returns the client and the unmatched buffer, so a terminal taking over an automated login can draw what had already arrived instead of losing it. #8 said only that it hands the client back; returning the buffer too is the maintainer's call, made on 2026-09-29 over dropping the buffer and over a separate `into_parts`, and it is theirs to reverse.

`client()` lends the client out read-only, so option state (e.g. the peer's ECHO) can be asked mid-automation; nothing that reads or sends is reachable through it, so the buffer cannot be bypassed. #23's user story 18 asked for the query but no slice carried it; adding it here is the maintainer's call, made on 2026-09-29, and theirs to reverse.

Against the netkit telnetd on RHEL 9 (probed 2026-09-29): an Expect session over `Client::connect` logged in with `expect` and `send_line`, matched a command's computed output rather than its echo, and the client it handed back logged out and saw `Closed`.
