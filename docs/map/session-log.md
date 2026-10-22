# Session log

## What it is

How an Expect session hands what it sends and receives to a caller's callback (`Expect::set_log`), in the order it happens, so a failed automation can be read back — with a secret line logged only as having been sent.

## Governing decisions

**None.** The shape was decided in #75's triage on 2026-10-12 (below); no decision record covers it.

## Design model

From the source and #75.

`Expect::set_log` takes a callback and hands it `Logged::Sent` for the bytes given to `send_line` (the line and its CR, before the client turns the CR into the policy's end of line, unless BINARY is on on our side, and doubles IAC) or `send_raw`, `Logged::SentSecret` for `send_secret_line`, and `Logged::Received` for each Data as a wait reads it, before it joins the buffer (`Expect::wait`). Telnet commands (`Expect::send_command`) and other Events are not logged. A send is logged after it succeeded, so a failed send leaves no entry, even one that wrote part of its bytes before failing; the caller has its `Err`. Keeping that, over logging before sending, which would log sends that failed, is the maintainer's call, made on 2026-10-12 after the review of #75, and theirs to reverse. Expect reads only inside waits, so the entries follow the order of sending and reading; `into_inner` drops the callback with the session.

The callback is kept in a `Mutex` (`Log`) though it is only called through `&mut self`: `Expect<TcpStream>` was `Send + Sync` before (checked 2026-10-12), a boxed `FnMut + Send` alone would have made it `!Sync`, and losing an auto trait is a breaking change ([versioning](versioning.md)). `get_mut` reaches it without locking (`Expect::record`), so the `Mutex` is never locked and never poisoned. A test pins the auto traits (`a_session_stays_send_and_sync_with_a_log`). A callback that panics unwinds out of the send or the wait that called it; in a wait, that is before the chunk joins the buffer. `Logged` derives no `Copy`, so a later variant may own its data without a breaking change.

`SentSecret` masks only the sending side. A device that echoed the secret would send it back as Data, logged as `Received`, which the crate docs say; the lab telnetd did not echo a password ([waiting for patterns](waiting-for-patterns.md), Echo).

Logging data in Expect rather than wire bytes, a `(direction, bytes)` callback, `send_secret_line` for masking, and data only as sent are the maintainer's calls, made on 2026-10-12 in #75's triage. They chose these over a wire-level stream wrapper like the real-telnetd test's `Recorder`, either documented or as a public adapter (raw Telnet bytes, no masking, and unusable with `Client::connect`), over `tracing`, over one or two `io::Write`s, and over pausing the log or not masking at all; they are theirs to reverse.

Nothing bounds what the callback is handed beyond the read size (4 KiB per `Received`).

## Code

- `justelnet-expect/src/lib.rs` — `Expect::set_log`, `Expect::record`, `Logged`, `Log`, `Expect::send_line`, `Expect::send_secret_line`, `Expect::send_raw`, `Expect::wait` (the `Received` entry), the crate docs' "Logging a session"
- `justelnet-expect/tests/expect.rs` — `log_into`, `a_logged_login_records_both_directions_but_not_the_secret`, `raw_bytes_are_logged_and_telnet_commands_are_not`, `each_read_of_a_wait_is_logged_as_it_arrives`, `a_session_stays_send_and_sync_with_a_log`

## Reference behaviour

**None.** Not compared to another library's logging; the alternatives weighed are in #75.

## Cross-cutting invariants

**None.**

## Blast radius

- [Waiting for patterns](waiting-for-patterns.md) — the `Received` entry sits inside `wait`; a change to how data enters the buffer moves or duplicates it.
- [Sending](sending.md) — `Logged::Sent` carries the bytes before the Core's end-of-line translation; a caller reading the log as wire bytes would be wrong under the default CR LF.
- [Logging in to a real server](logging-in-to-a-real-server.md) — its `Recorder` is the wire-level alternative this note declined; the two record different things (Telnet bytes vs data).
- [Versioning](versioning.md) — `Expect`'s `Send + Sync` is semver surface the `Mutex` protects.

## Known holes / open

- Telnet commands sent with `Expect::send_command` and option changes are not logged, by design (above); a failed automation that hinged on negotiation cannot be read back from this log.
