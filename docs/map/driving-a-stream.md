# Driving a stream

## What it is

The Driver (GLOSSARY.md): how the `justelnet` crate's `Client` binds one Core to one async stream — connecting by address, the `next_event` loop that writes the Core's answers before it reads, the send methods, cancel safety, and how a connection ends and what the caller sees afterwards. There is no background task.

## Governing decisions

- [ADR 0001](../adr/0001-sans-io-core-with-tokio-driver.md): tokio lives only in the Driver, which owns the stream and every timeout; the Core stays sans-IO.

The receive loop (#22), the end-of-connection rules (#10) and cancel safety (#26) are issue decisions; the maintainer's calls are dated below.

## Design model

From the source, the issues and probes against the maintainer's lab telnetd.

### Write, then return, then read

`next_event` writes every byte the Core has queued, then returns a queued Event if there is one, and only otherwise reads, feeds the Core and goes round again (`Client::drive`, #22's receive loop). So the Core's automatic answers are on the wire before the caller sees the Event that followed them: a telnetd waiting on `WILL TTYPE` is never kept waiting while the caller handles data. An active start's requests go out on the first call, before any read. Nothing is written between calls.

Against the netkit telnetd on RHEL 9, the client over a plain `TcpStream` with the default policy reached `localhost login: ` with no other code (probed 2026-09-29).

The order has a cost PuTTY does not pay. PuTTY's `sk_write` appends to the socket's output bufchain and returns (`sk_net_write` in `unix/network.c`), and it stops reading only when the terminal's backlog passes 4096 bytes (`c_write` in `otherbackends/telnet.c`), never for unsent output. The client does not read while a write is blocked. A peer that stops reading while it is still writing to us stalls `next_event` in the write. The order is #22's, and cancelling the call in a `select!` (#26) is what lets the caller get out.

### What is kept between calls

- Bytes taken from the Core's output stay in the client's own buffer (`outgoing`) with a count of how many are written (`written`), because `poll_transmit` hands over everything it holds. A write that stops part-way resumes from the count.
- Whether written bytes are still unflushed (`unflushed`) is kept apart from the buffer, so a stream with its own buffer (a `BufWriter`, TLS) is still flushed on the next call when the last one stopped between the write and the flush.
- The read buffer is a 4 KiB boxed field (`read_buf`), not a local: as a local it made the `next_event` future 4168 bytes, zeroed on every call, and `select!` loops build a new one each time round; as a field the future is 72 bytes (measured 2026-09-29).

The stream must be `Unpin`, as tokio's `AsyncReadExt::read` requires; a stream that is not can be passed as `Pin<Box<S>>`.

### What the caller sees

`justelnet::Event` wraps the Core's Event (`Event::Core(core::Event::Data(..))`) and adds `Closed`, which a clean EOF returns. The Core's Event is `#[non_exhaustive]` and has no Closed, so the client needs its own type; wrapping passes any Event the Core adds through unchanged. This is the maintainer's call, made on 2026-09-29 over a flat mirror of the Core's variants plus Closed (easier to match, but a wildcard arm would silently absorb any new Core variant) and `Result<Option<core::Event>>` with `None` for the end (against #10's "Closed Event"). It is theirs to reverse.

The Core is re-exported as `justelnet::core`, so both Events and both Errors are told apart by path. A file that does `use justelnet::core;` shadows the standard `core` crate there and writes `::core::` for it. This is the maintainer's call, made on 2026-09-29 over `justelnet::proto` (quinn's name, no clash, but not GLOSSARY.md's word) and a glob re-export at the root (whose `Event` would be ambiguous to a reader). It is theirs to reverse. The re-export is also why a breaking Core release is a breaking `justelnet` release ([versioning](versioning.md)).

### Cancelling `next_event`

`next_event` is cancel-safe, and says so in its documentation: nothing it holds between awaits lives in the future. What it read is in the Core before the next await; what it took from the Core is in the client's buffer with its written count; a cut-short flush leaves `unflushed` set. So a `select!` that drops the call, at any point, loses no byte either way.

The proof is a randomized test (`cancelled_calls_lose_no_byte_either_way`): the device plays the server bytes of the netkit Transcript through a 3-byte pipe, in random pieces at random moments, reading as slowly, and the client sits behind a 2-byte `BufWriter` so that a flush can be cut short as well as a write. Each call races a future that wins after a random number of polls (`biased`, so a seed replays the same way). 300 seeds cancel 1808 calls (2026-09-29; the test asserts only more than 1000), and the Data, the other Events and every byte the device received match an uncancelled run.

Each thing the proof leans on was broken once and the test went red (2026-09-29): output taken into a local before writing, an await between read and receive, and a write count reset on every call all fail from seed 1. Clearing the flush flag before the flush has finished fails only from seed 49: the flush window is narrow, and a seed count much below 300 would miss it.

The test holds its own copy of the netkit Transcript's five `server:` lines (`NETKIT_SERVER`) rather than including `justelnet-core/tests/transcripts/telnetd-netkit-rhel9.txt`, since the published `justelnet` crate ships its tests but not that file (#55, see [releasing](releasing.md)). The copy matched the Transcript byte for byte when made and still does (checked 2026-10-22), and the flush-flag mutation still failed from seed 49 (2026-10-07). Copying the lines, over copying the whole file or leaving the tests out of the package, is the maintainer's call, made on 2026-10-07; it is theirs to reverse. A change to that Transcript does not reach this test; the Transcript's header names the copy.

### Sending

Every send method (`send_data`, `send_raw`, `send_command`, `set_window_size`, `request_enable`, `request_disable`, `send_subnegotiation`) hands the call to the Core under the Core's own name, then writes everything queued (`Client::send_with`), so bytes queued earlier (an answer, an active start) go out first and in order. A Core refusal (`Error::Core`) returns before writing: the call sends nothing, anything queued earlier waits for the next call, and the connection stays usable (#10: caller misuse). A failed write ends the connection like a failed read. After the end, the methods return `Error::Closed` without giving the Core anything. `is_enabled` reads the Core's state and works at any time.

Every method takes `&mut self`, so a caller whose `next_event` future is pending in a `select!` has to drop it to send; that is why #26's cancel safety is what makes a terminal loop possible.

Against the netkit telnetd on RHEL 9 (probed 2026-09-29): login through `send_data`, then `set_window_size(100, 40)` made `stty size` answer `40 100`, and `exit` gave `Closed`, then `Err(Closed)` from the next send.

### Connecting by address

`Client::connect(addr, policy, timeout)` bounds resolving and connecting together by `timeout`; running out is `Error::ConnectTimeout`, and any other failure (refused, unreachable) is `Error::Io`. The timeout is an argument with no default, so the library holds no opinion on how long a device may take. The variant's name and the explicit argument are the maintainer's call, made on 2026-09-29 over `Error::Timeout` (the Expect session's pattern timeout already has that name, #10) and a fixed default beside a `connect_timeout` variant; it is theirs to reverse.

The socket options `connect` sets — urgent data inline and no delay — and why are in [socket setup matches connect](socket-setup-matches-connect.md). socket2 is the dependency that sets the first: already in the tree through tokio's `net` feature (0.6, `features = ["all"]`), so depending on it directly adds no crate; the dependency is the maintainer's call, made on 2026-09-29 over per-platform `setsockopt` code and leaving it unset, and theirs to reverse.

The timeout test (`an_unanswered_connect_times_out`) connects to TEST-NET-1 (`192.0.2.1`, RFC 5737) under paused time: the clock jumps to the timer once the runtime is idle, so it asserts the time waited, not only the variant, or a timeout ignoring its argument would pass. The other connect tests' `TIMEOUT` is 30 s, generous so that a slow CI machine never trips it (moved here from the comment on `TIMEOUT` in `connect.rs`, 2026-10-22).

### How a connection ends

As #10 decided: a clean EOF is `Event::Closed`, a failed read or write is `Error::Io` with the stream's own error, and either ends the connection (`closed`), so every later call returns `Error::Closed` without touching the stream. Output still queued at that point is dropped. An EOF is taken as the whole connection ending, not a half-close the peer might still read from. Peer faults stay Warning Events (#10's principle: what the peer sends is data).

Over real sockets on Windows (probed 2026-09-29): a peer's FIN gives `Closed`, then `Err(Closed)`; a reset (SO_LINGER 0) gives `Err(Io)` with `ConnectionReset` (os error 10054), then `Err(Closed)`.

`Error::Io` and `Error::Core` show and chain exactly as the error they wrap (Display and `source` forwarded), so a report printing the whole chain names each cause once. The Error is hand-written like the Core's, with no error-derive dependency.

### Testing the automatic answer

Asserting "the device receives `WILL TTYPE` before the next Data" needs a policy that accepts TTYPE on our side without requesting it (`the_answer_is_written_before_the_next_data` uses `refuse_all().accept(TTYPE, Local)`). The default policy's active start already sent `WILL TTYPE`, so the device's `DO TTYPE` only confirms it (RFC 1143 `WANTYES` to `YES`) and nothing is written for it.

### Input bounds

One read takes at most 4096 bytes (`read_buf`); the Core is fed each read before the next. Nothing here bounds what accumulates in `outgoing` while a write is blocked, beyond what the caller sends.

## Code

- `justelnet/src/lib.rs` — `Client`, `Client::connect`, `Client::new`, `Client::next_event`, `Client::drive`, `Client::write_outgoing`, `Client::send_with`, `Client::send_data`, `Client::send_raw`, `Client::send_command`, `Client::set_window_size`, `Client::request_enable`, `Client::request_disable`, `Client::send_subnegotiation`, `Client::is_enabled`, `Event`, `Error`, `pub use justelnet_core as core`
- `justelnet/tests/` — `client.rs`, `sending.rs`, `connect.rs` (`TIMEOUT`, `an_unanswered_connect_times_out`, `urgent_data_arrives_inline`), `connection_end.rs`, `cancel_safety.rs` (`NETKIT_SERVER`, `cancelled_calls_lose_no_byte_either_way`), `common/mod.rs` (`received`)

## Reference behaviour

- PuTTY `unix/network.c` (`sk_net_write`), `otherbackends/telnet.c` (`c_write`), read raw.
- [libtelnet vs russh](../research/libtelnet-vs-russh.md): quinn-proto's pull shape and russh's Handler awaited inside the read loop, the comparison ADR 0001 rests on.

## Cross-cutting invariants

- [Socket setup matches connect](socket-setup-matches-connect.md) — `Client::connect` is one of its sites.

## Blast radius

- [Protocol core](protocol-core.md) — the Driver relies on the Core's pull API (`receive`, `poll_event`, `poll_transmit`) and that `receive` never fails; a Core that pushed or failed would break `drive`.
- [Sending](sending.md) and [option negotiation](option-negotiation.md) — each `Client` method forwards to the Core method of the same name and restates its doc; a changed Core rule changes the Driver's docs.
- [Waiting for patterns](waiting-for-patterns.md) — Expect's waits are `next_event` calls under `timeout_at`; they rely on cancel safety to lose nothing at a timeout, and on `Event::Closed` to end a wait.
- [Logging in to a real server](logging-in-to-a-real-server.md) — the e2e test builds its client with `Client::new` on a stream it sets up itself.
- [Transcript replay](transcript-replay.md) — `cancel_safety.rs` holds a copy of `telnetd-netkit-rhel9.txt`'s server lines; changing that Transcript does not reach the copy.
- [Releasing](releasing.md) — a test that includes a file outside its crate breaks the published package; the copy above exists for that.
- [Versioning](versioning.md) — `Event` and `Error` are `#[non_exhaustive]`; the Core re-export makes Core breaks cascade.

## Known holes / open

- A peer that stops reading while still writing stalls `next_event` in its write; only cancellation gets the caller out (above).
- No way to send TCP urgent data, so a Synch cannot be sent ([sending](sending.md)).
- `TCP_NODELAY` has no test ([socket setup matches connect](socket-setup-matches-connect.md)).
