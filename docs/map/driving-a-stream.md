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

A caller's own TCP stream gets no socket options from the client, so `Client::new` asks for urgent data to be kept inline: without it the Synch a netkit telnetd sends around its banner can lose its IAC (see [receive parsing](receive-parsing.md) and #28's comment, where the connect path sets it).

The stream must be `Unpin`, as tokio's `AsyncReadExt::read` requires; a stream that is not can be passed as `Pin<Box<S>>`.

## What the caller sees

`justelnet::Event` wraps the Core's Event (`Event::Core(core::Event::Data(..))`) and adds `Closed`, which a clean EOF returns. The Core's Event is `#[non_exhaustive]` and has no Closed, so the client needs its own type; wrapping passes any Event the Core adds through unchanged. This is the maintainer's call, made on 2026-09-29 over a flat mirror of the Core's variants plus Closed (easier to match, but a wildcard arm would silently absorb any new Core variant) and `Result<Option<core::Event>>` with `None` for the end (against #10's "Closed Event"). It is theirs to reverse.

The Core is re-exported as `justelnet::core`, so both Events (and, with #27, both Errors) are told apart by path. A file that does `use justelnet::core;` shadows the standard `core` crate there and writes `::core::` for it. This is the maintainer's call, made on 2026-09-29 over `justelnet::proto` (quinn's name, no clash, but not CONTEXT.md's word) and a glob re-export at the root (whose `Event` would be ambiguous to a reader). It is theirs to reverse.

## Testing the automatic answer

Asserting "the device receives `WILL TTYPE` before the next Data" needs a policy that accepts TTYPE on our side without requesting it. The default policy's active start already sent `WILL TTYPE`, so the device's `DO TTYPE` only confirms it (RFC 1143 `WANTYES` to `YES`) and nothing is written for it.
