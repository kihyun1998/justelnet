# Waiting for patterns

How the Expect session (`justelnet-expect`) turns a client's Data into waits for prompts.

## The prototype is the oracle

The matching rules are the ones walked through in the prototype that settled #8 (`prototypes/expect-api.PROTOTYPE.html` on `prototype/expect-api`, commit `905e3e5`). Its `Expect` module has no DOM, so the tests' expected matches and buffers come from running that module itself under node on the same scenario steps, not from a Rust copy of it that could share the implementation's mistakes. To redo it: take the text from `const Expect = (() => {` to the matching `})();`, evaluate it, and dispatch the scenario's actions (`serverData`, `expect`, `tick`) as the page's `scenarios` list does. Done on 2026-09-29 for scenarios 1, 2 and 5, and on 2026-10-07 for scenarios 1 (its last step, `expect_any([">", "#"])`), 3, 4 and 6 and for the earliest-match and tie cases. In scenario 4 the second page's `before` starts with a space: the one after `--More--`, left in the buffer by the first match.

The scenario 4 test's `--More--` loop stops itself after as many waits as there are pages. Without that bound, a wait that never reports the prompt (an `index` stuck at 0 did, 2026-10-07) keeps the loop sending spaces forever, and the test hangs rather than fails.

## What a wait does

- The buffer is searched before any read, so data left by the last match is matched at once (scenario 3's `Username:` after `% Login invalid`).
- Each pattern's first match counts; the earliest start wins, ties going to the pattern listed first. The data ahead of it is `before`, the rest stays buffered.
- `expect_any` takes `&[P]` for any `P: Into<Pattern> + Clone`, the shape #8 settled (`expect_any(&[patterns], timeout)`), so `&[">", "#"]` works as written and a list mixing literals and regexes is a `&[Pattern]`. An empty list matches nothing, so the wait ends at the timeout or a close, which is what the matching rule gives with no candidates.
- An empty literal matches at the start with nothing before it and takes nothing, as the prototype's `new RegExp("")` does.
- The timeout counts from the start of the wait, not from the last data, so a device trickling output without the prompt still times out. The error carries a copy of the buffer and the session keeps its own.
- Data is appended; a clean close fails the wait with `Closed { buffer }`; every other Event is skipped, since the Core already handled it.

Patterns are bytes (a `&str` literal matches its UTF-8 bytes), so an ASCII prompt matches on any encoding. How each chunk is searched is under *Searching as data arrives*.

## Searching as data arrives

Within one wait, a literal is searched with `memchr::memmem`, and each search starts where the last one could no longer have matched: the last `len - 1` bytes of what it searched, moved back by whatever the limit dropped since. A wait on a literal therefore costs, per chunk, about what the chunk costs. A new wait searches from the beginning, since its patterns are not the last wait's.

A regex searches all the unmatched data on every chunk. One not bounded in length (`a.*b`) can start a match anywhere once more data arrives, so there is no safe point to resume from. pexpect draws the same line (`pexpect/expect.py`, master `ad4be95`, read 2026-10-12): its literal searcher starts at `-(freshlen + len(s))`, since "the match, if any, can only be in the fresh data, or at the very end of the old data", and its regex searcher starts over, since "'freshlen' doesn't help here -- we cannot predict the length of a match". One anchored with `$` costs little anyway: the regex crate searches it from the end, and on 2026-10-07 (release build, no prompt in the data) the documented prompt regex took 17–178 ns per search from 4 KiB to 1 MiB of buffer. An unanchored regex re-searches the whole buffer on each chunk, bounded only by the buffer limit. Saying so here and not in the crate documentation, and resuming literals only with no `searchwindowsize`-like setting, are the maintainer's calls, made on 2026-10-12 in #60's triage; they are theirs to reverse.

Measured on 2026-10-12 (release build, Windows), a wait on a literal that never matches, fed through a 1 KiB duplex until it closes: 256 KiB took 1.2–1.9 ms, 512 KiB 1.9–2.3 ms, 1 MiB 3.8–4.5 ms, 2 MiB (2 MiB limit) 7.8–9.7 ms and 4 MiB (1 MiB limit) 16.5–19.6 ms, linear and no slower at the limit. The whole-buffer search took 71 ms, 284 ms, 1.39 s, 6.7 s and 15.2 s on the same runs. One search of 1 MiB took 772 µs with a byte-by-byte window scan and 45 µs with `memmem`. The program is not in the repository, by the maintainer's call (2026-10-12): a scratch binary on `justelnet-expect` writes N KiB of `x` in 1 KiB writes and times `expect("NEVER-MATCHES")` until `Closed`.

`memchr` is a dependency of this crate alone, at the version `regex` already pulls in; adding it is the maintainer's call, made on 2026-10-12, and theirs to reverse.

## Byte regexes

A pattern is either a literal or a `regex::bytes::Regex`; a regex counts by its leftmost-first match, the same rule as a literal's first occurrence. The `regex` crate is a dependency of this crate alone (#23 user story 24), and it is re-exported so a caller builds patterns with the version the session accepts.

The documentation's prompt regex is scenario 6's, `\n\S+[#>] ?$`. Its `$` is the end of the data received so far, so it holds only while the prompt is the last thing the device sent. It matches from the `\n`, so on a `\r\n` line end `matched` is `\nR1#` and `before` ends in a lone `\r`, which is what the prototype gives (run under node on 2026-10-07).

The regex crate keeps Unicode mode on by default, and then `\S` and `.` match only valid UTF-8. The search still runs across EUC-KR or other non-UTF-8 bytes, so an ASCII prompt after an EUC-KR banner matches. But a prompt whose own text is EUC-KR, such as a Korean hostname, does not match `\S+`; `(?-u:\S)` matches any non-whitespace byte. Checked in the regex 1.13.1 source (`src/regex/bytes.rs`, "matching invalid UTF-8").

The crate docs say this in one line under the example, pointing at `(?-u:\S)`, and the example keeps scenario 6's regex. The maintainer chose that on 2026-10-07 over switching the example to `(?-u:\S)` (which would part it from the prototype) and over leaving the trap to this note alone; it is theirs to reverse. A test pins both halves: `\S` times out on an EUC-KR hostname and `(?-u:\S)` matches it.

`Pattern` also comes from `&Regex`, cloning it (the regex crate shares a compiled regex behind a reference count), so waiting on one prompt regex repeatedly needs no `.clone()` at each wait. #23 asked only that byte regexes be accepted wherever a pattern is; the borrowed form is the maintainer's call, made on 2026-10-07 over leaving it to #31 or not adding it, and theirs to reverse. A byte-literal pattern (`&[u8]`) was considered and dropped the same day: nobody asked for it, and a byte regex such as `(?-u)\xc7\xe3` already expresses one.

`before_lossy` and `matched_lossy` are for display only. Some EUC-KR byte pairs are also valid UTF-8 (`\xda\xb8` reads as U+06B8), so the lossy text of an EUC-KR banner is a mix of replacement characters and unrelated letters, not a predictable string.

## The buffer limit

The session keeps at most `DEFAULT_BUFFER_LIMIT` (1 MiB) of unmatched data unless `set_buffer_limit` says otherwise, and drops the oldest bytes past it: after each chunk is appended and before it is searched, and at once when the limit is lowered, so the unmatched data never exceeds the limit. The dropped bytes are skipped rather than moved out at once, and reclaimed in one move when they add up to the limit, so a wait at the limit does not shift the whole buffer on every chunk (44 µs per 1 KiB chunk at 1 MiB, measured 2026-10-12). They are also reclaimed before a chunk that would make the buffer reallocate, leaving room for as much data again. Without that, a `Vec` that had doubled to exactly twice the limit doubled again when a one-byte read put the next reclaim off by a byte: 1 KiB reads with one 1-byte read after 2 MiB, limit 1 MiB, ended at 4.00× the limit, and at 2.00× with it (2026-10-12, release build, capacity read from a scratch copy). With a limit smaller than one read (the Driver reads up to 4 KiB at a time), the allocation follows the read instead: about twice the unmatched data plus the read. Holding about twice the limit is the maintainer's call, made on 2026-10-12 and theirs to reverse. The public API cannot see the allocation, so a unit test inside the crate (`the_buffer_allocation_stays_near_twice_the_limit`) drives a real wait and reads the capacity; without the reclaim before a reallocating chunk it reads 4× the limit. A test behind the public surface is the maintainer's call, made on 2026-10-12 after both reclaims were shown disabled with every public test still passing; it is theirs to reverse. What a wait returns (`before`, the Timeout and Closed buffers, `into_inner`) and what `Debug` shows never hold more than the limit. Dropping rather than refusing is #8's and #23's decision. It parts from pexpect, whose buffer is unbounded and whose `searchwindowsize` only narrows what is searched ("Data before searchwindowsize point is preserved, but not searched", `pexpect/spawnbase.py`).

The buffer grows only inside a wait: nothing reads the socket between waits, so a chatty device then fills the socket and TCP slows it, not this buffer (#23's story 21 assumed otherwise). What the limit bounds is one long wait that never matches.

1 MiB is this implementation's value, which #23 left to it: it holds a large `show running-config` in one wait. The limit is also the ceiling on an unanchored regex's re-search (above); the test of the default waits on an anchored regex to stay fast.

Past the limit, `before` silently loses its head. Saying so in the documentation of `set_buffer_limit` and the crate, rather than reporting the dropped count in `Match`, is the maintainer's call, made on 2026-10-07; it is theirs to reverse. Shown afterwards that the Timeout and Closed buffers are cut the same way, they chose the same: one more line in `set_buffer_limit`'s documentation, no new field.

## Paging

The crate documentation shows the `--More--` loop (wait for the marker or the prompt, send a space on the marker), the shape scenario 4's test proves, and the commands that turn paging off: `terminal length 0` on Cisco IOS and `set cli screen-length 0` on Junos (Juniper's CLI environment settings documentation, read through a search summary on 2026-10-07). Paging is not built in, per #8.

## Handing the client back

`into_inner` returns the client and the unmatched buffer, so a terminal taking over an automated login can draw what had already arrived instead of losing it. #8 said only that it hands the client back; returning the buffer too is the maintainer's call, made on 2026-09-29 over dropping the buffer and over a separate `into_parts`, and it is theirs to reverse.

`client()` lends the client out read-only, so option state (e.g. the peer's ECHO) can be asked mid-automation; nothing that reads or sends is reachable through it, so the buffer cannot be bypassed. #23's user story 18 asked for the query but no slice carried it; adding it here is the maintainer's call, made on 2026-09-29, and theirs to reverse.

Against the netkit telnetd on RHEL 9 (probed 2026-09-29): an Expect session over `Client::connect` logged in with `expect` and `send_line`, matched a command's computed output rather than its echo, and the client it handed back logged out and saw `Closed`.
