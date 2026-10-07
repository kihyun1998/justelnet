# Driving a stream

How the `justelnet` client binds one Core to one async stream: what `next_event` does in which order, and what the caller sees.

## Write, then return, then read

`next_event` writes every byte the Core has queued, then returns a queued Event if there is one, and only otherwise reads, feeds the Core and goes round again (#22's receive loop). So the Core's automatic answers are on the wire before the caller sees the Event that followed them: a telnetd waiting on `WILL TTYPE` is never kept waiting while the caller handles data. An active start's requests go out on the first call, before any read. Nothing is written between calls; there is no background task.

Against the netkit telnetd on RHEL 9, the client over a plain `TcpStream` with the default policy reached `localhost login: ` with no other code (probed 2026-09-29).

The order has a cost PuTTY does not pay. PuTTY's `sk_write` appends to the socket's output bufchain and returns (`sk_net_write` in `unix/network.c`), and it stops reading only when the terminal's backlog passes 4096 bytes (`c_write` in `otherbackends/telnet.c`), never for unsent output. The client does not read while a write is blocked. A peer that stops reading while it is still writing to us stalls `next_event` in the write. The order is #22's, and cancelling the call in a `select!` (#26) is what lets the caller get out.

## What is kept between calls

- Bytes taken from the Core's output stay in the client's own buffer with a count of how many are written, because `poll_transmit` hands over everything it holds. A write that stops part-way resumes from the count.
- Whether written bytes are still unflushed is kept apart from the buffer, so a stream with its own buffer (a `BufWriter`, TLS) is still flushed on the next call when the last one stopped between the write and the flush.
- The read buffer is a 4 KiB field, not a local: as a local it made the `next_event` future 4168 bytes, zeroed on every call, and `select!` loops build a new one each time round; as a field the future is 72 bytes (measured 2026-09-29).

A caller's own TCP stream gets no socket options from the client, so `Client::new` asks for urgent data to be kept inline: without it the Synch a netkit telnetd sends around its banner can lose its IAC (see [receive parsing](receive-parsing.md)). `Client::connect` sets it itself; see below.

The stream must be `Unpin`, as tokio's `AsyncReadExt::read` requires; a stream that is not can be passed as `Pin<Box<S>>`.

## What the caller sees

`justelnet::Event` wraps the Core's Event (`Event::Core(core::Event::Data(..))`) and adds `Closed`, which a clean EOF returns. The Core's Event is `#[non_exhaustive]` and has no Closed, so the client needs its own type; wrapping passes any Event the Core adds through unchanged. This is the maintainer's call, made on 2026-09-29 over a flat mirror of the Core's variants plus Closed (easier to match, but a wildcard arm would silently absorb any new Core variant) and `Result<Option<core::Event>>` with `None` for the end (against #10's "Closed Event"). It is theirs to reverse.

The Core is re-exported as `justelnet::core`, so both Events and both Errors are told apart by path. A file that does `use justelnet::core;` shadows the standard `core` crate there and writes `::core::` for it. This is the maintainer's call, made on 2026-09-29 over `justelnet::proto` (quinn's name, no clash, but not GLOSSARY.md's word) and a glob re-export at the root (whose `Event` would be ambiguous to a reader). It is theirs to reverse.

## Cancelling `next_event`

`next_event` is cancel-safe, and says so in its documentation: nothing it holds between awaits lives in the future. What it read is in the Core before the next await; what it took from the Core is in the client's buffer with its written count; a cut-short flush leaves `unflushed` set. So a `select!` that drops the call, at any point, loses no byte either way.

The proof is a randomized test: the device plays the server bytes of the netkit Transcript through a 3-byte pipe, in random pieces at random moments, reading as slowly, and the client sits behind a 2-byte `BufWriter` so that a flush can be cut short as well as a write. Each call races a future that wins after a random number of polls (`biased`, so a seed replays the same way). 300 seeds cancel 1808 calls, and the Data, the other Events and every byte the device received match an uncancelled run.

Each thing the proof leans on was broken once and the test went red (2026-09-29): output taken into a local before writing, an await between read and receive, and a write count reset on every call all fail from seed 1. Clearing the flush flag before the flush has finished fails only from seed 49: the flush window is narrow, and a seed count much below 300 would miss it.

The test holds its own copy of the netkit Transcript's five `server:` lines rather than including `justelnet-core/tests/transcripts/telnetd-netkit-rhel9.txt`, since the published `justelnet` crate ships its tests but not that file (#55). The copy matched the Transcript byte for byte when made, and the flush-flag mutation still failed from seed 49 (2026-10-07). Copying the lines, over copying the whole file or leaving the tests out of the package, is the maintainer's call, made on 2026-10-07; it is theirs to reverse. A change to that Transcript does not reach this test.

## Sending

Every send method (`send_data`, `send_raw`, `send_command`, `set_window_size`, `request_enable`, `request_disable`, `send_subnegotiation`) hands the call to the Core under the Core's own name, then writes everything queued, so bytes queued earlier (an answer, an active start) go out first and in order. A Core refusal (`Error::Core`) returns before writing: the call sends nothing, anything queued earlier waits for the next call, and the connection stays usable (#10: caller misuse). A failed write ends the connection like a failed read. After the end, the methods return `Error::Closed` without giving the Core anything. `is_enabled` reads the Core's state and works at any time.

Every method takes `&mut self`, so a caller whose `next_event` future is pending in a `select!` has to drop it to send; that is why #26's cancel safety is what makes a terminal loop possible.

Against the netkit telnetd on RHEL 9 (probed 2026-09-29): login through `send_data`, then `set_window_size(100, 40)` made `stty size` answer `40 100`, and `exit` gave `Closed`, then `Err(Closed)` from the next send.

## Connecting by address

`Client::connect(addr, policy, timeout)` bounds resolving and connecting together by `timeout`; running out is `Error::ConnectTimeout`, and any other failure (refused, unreachable) is `Error::Io`. The timeout is an argument with no default, so the library holds no opinion on how long a device may take. The variant's name and the explicit argument are the maintainer's call, made on 2026-09-29 over `Error::Timeout` (the Expect session's pattern timeout already has that name, #10) and a fixed default beside a `connect_timeout` variant; it is theirs to reverse.

The socket keeps urgent data inline (`SO_OOBINLINE`), set through socket2 because tokio exposes no setter. socket2 is already in the tree through tokio's `net` feature (0.6, `features = ["all"]`), so depending on it directly adds no crate; the dependency is the maintainer's call, made on 2026-09-29 over per-platform `setsockopt` code and leaving it unset, and theirs to reverse. A local test sends a byte with `send_out_of_band` and requires it among the Data; without the setting it is lost.

Against the netkit telnetd on RHEL 9 from Windows (probed 2026-09-29, 30 connections each): through `connect`, the Synch arrived whole as a DataMark Command in 5 and never as a stray byte; through a plain `TcpStream`, it never arrived whole and put a lone `f2` into the Data in 2. The server sends the Synch only on some connections, so the counts are small; the local test is the deterministic proof.

The socket also sends without delay (`TCP_NODELAY`), as PuTTY does by default (`tcp_nodelay`, `conf.h`). The maintainer chose it on 2026-09-29 on PuTTY's default alone, with nothing measured; it is theirs to reverse. Measured afterwards against the lab telnetd over a LAN, two keystrokes back to back echoed in a median 0.90 ms with it and 1.23 ms without (maximum 40 ms and 43 ms, 10 each), and on loopback the two were the same: no gain shown on a short path, where the delayed ACK that Nagle waits for comes back at once. No test covers the setting, since the client does not expose its socket and a timing threshold would not hold on CI.

The timeout test connects to TEST-NET-1 (`192.0.2.1`, RFC 5737) under paused time: the clock jumps to the timer once the runtime is idle, so it asserts the time waited, not only the variant, or a timeout ignoring its argument would pass.

## How a connection ends

As #10 decided: a clean EOF is `Event::Closed`, a failed read or write is `Error::Io` with the stream's own error, and either ends the connection, so every later call returns `Error::Closed` without touching the stream. Output still queued at that point is dropped. An EOF is taken as the whole connection ending, not a half-close the peer might still read from. Peer faults stay Warning Events (#10's principle: what the peer sends is data).

Over real sockets on Windows (probed 2026-09-29): a peer's FIN gives `Closed`, then `Err(Closed)`; a reset (SO_LINGER 0) gives `Err(Io)` with `ConnectionReset` (os error 10054), then `Err(Closed)`.

`Error::Io` and `Error::Core` show and chain exactly as the error they wrap (Display and `source` forwarded), so a report printing the whole chain names each cause once. The Error is hand-written like the Core's, with no error-derive dependency.

## Testing the automatic answer

Asserting "the device receives `WILL TTYPE` before the next Data" needs a policy that accepts TTYPE on our side without requesting it. The default policy's active start already sent `WILL TTYPE`, so the device's `DO TTYPE` only confirms it (RFC 1143 `WANTYES` to `YES`) and nothing is written for it.
