# libtelnet's event model vs russh's Handler model

Research for [#3](https://github.com/kihyun1998/justelnet/issues/3) (part of map #1).
Facts only; the choice belongs to the next ticket.

## Sources

All claims below come from source read raw at these revisions unless marked otherwise.

| Source | Revision | Files read |
|---|---|---|
| libtelnet (seanmiddleditch/libtelnet), branch `develop` | `5f5ecee776b9bdaa4e981e5f807079a9c79d633e` | `libtelnet.h`, `libtelnet.c` |
| russh (Eugeny/russh), branch `main`, crate `russh` 0.63.3 | `36fa1bcb3bdf0ea2214d2aa5ef5ea20c287c6312` | `russh/src/client/mod.rs`, `client/session.rs`, `client/encrypted.rs`, `session.rs`, `channels/mod.rs`, `sshbuffer.rs` |
| quinn (quinn-rs/quinn), branch `main` | `74b6a84ee31fe30974aa12bc208ba5c9b68cdda2` | `quinn-proto/src/lib.rs`, `quinn-proto/src/connection/mod.rs`, `quinn-proto/src/endpoint.rs`, `quinn/src/connection.rs` |
| rustls (rustls/rustls), branch `main`, 0.24.0-dev.1 | `99f2358cae2954837dbb866faf6727de75489ab9` | `rustls/src/conn/mod.rs` |
| tokio-rustls (rustls/tokio-rustls), branch `main`, 0.26.5 (depends on rustls 0.23.27) | `9d0dae6593daec0f9c44144e9ad7127fe574c8b0` | `src/common/mod.rs` |

No docs.rs prose or blog write-ups were used.

## libtelnet

### Entry points

One opaque state object per connection, `telnet_t` (`libtelnet.h`: "Each connection must have its own telnet state tracker object").

- `telnet_t *telnet_init(const telnet_telopt_t *telopts, telnet_event_handler_t eh, unsigned char flags, void *user_data)` — `calloc`s the struct and stores the four arguments; sends nothing (`libtelnet.c` `telnet_init`).
- `void telnet_free(telnet_t *)`.
- Input: `void telnet_recv(telnet_t *, const char *buffer, size_t size)` — "The byte buffer is most often going to be the buffer that recv() was called for" (header doc). Returns nothing; all results come out through the event handler.
- Output (all return `void` and emit bytes via the handler, never to a socket): `telnet_iac`, `telnet_negotiate`, `telnet_send` (escapes IAC), `telnet_send_text` (escapes IAC and translates `\r`→CR NUL, `\n`→CR LF unless in BINARY), `telnet_begin_sb`/`telnet_finish_sb`, `telnet_subnegotiation`, `telnet_begin_compress2`, `telnet_printf`/`telnet_raw_printf` (+`v` variants), and option helpers: `telnet_begin_newenviron`/`telnet_newenviron_value`, `telnet_ttype_send`/`telnet_ttype_is`, `telnet_send_zmp*`/`telnet_begin_zmp`/`telnet_zmp_arg`.
- Flags: `TELNET_FLAG_PROXY` (pass negotiation through untouched), `TELNET_FLAG_NVT_EOL` (translate incoming CR LF / CR NUL). Bits 5–7 of the same `flags` byte are internal (`TELNET_FLAG_TRANSMIT_BINARY`, `TELNET_FLAG_RECEIVE_BINARY`, `TELNET_PFLAG_DEFLATE`).

### Event / callback type

A single C function pointer, fixed at `telnet_init`:

```c
typedef void (*telnet_event_handler_t)(telnet_t *telnet, telnet_event_t *event, void *user_data);
```

`telnet_event_t` is a tagged union; `type` selects the member. Event types (`enum telnet_event_type_t`):

| Event | Payload | Emitted from |
|---|---|---|
| `TELNET_EV_DATA` | `data.buffer`, `data.size` — a borrowed slice into the caller's input buffer (or a 1-byte stack local for an escaped IAC / CR) | `_process` |
| `TELNET_EV_SEND` | `data.buffer`, `data.size` — bytes the application must write to the peer | `_send`, used by every output function **and** by `telnet_recv` (automatic negotiation replies) |
| `TELNET_EV_IAC` | `iac.cmd` | any IAC command other than SB/WILL/WONT/DO/DONT/IAC |
| `TELNET_EV_WILL/WONT/DO/DONT` | `neg.telopt` | `_negotiate`, only on RFC 1143 state transitions (see below), or unconditionally in PROXY mode |
| `TELNET_EV_SUBNEGOTIATION` | `sub.telopt`, `sub.buffer`, `sub.size` | `_subnegotiate`, always, before any option-specific parse |
| `TELNET_EV_COMPRESS` | `compress.state` | MCCP2 start/stop (zlib builds only) |
| `TELNET_EV_ZMP` / `TTYPE` / `ENVIRON` / `MSSP` | parsed argv / name / key-value arrays | `_subnegotiate`, *in addition to* the raw SUBNEGOTIATION event |
| `TELNET_EV_WARNING` / `TELNET_EV_ERROR` | `error.file/func/line/msg/errcode` | `_error` (fatal flag selects ERROR) |

Observed in the source: `_error` fills `file`, `func`, `line`, `msg` but never assigns `ev.error.errcode`; the code is only returned to the internal caller. `msg` points at a 512-byte stack buffer valid only during the callback.

All pointers in events are borrowed and valid only for the duration of the callback.

### Who owns I/O

The application. libtelnet contains no socket, no read, no write, no timer, no allocation of the caller's buffers. Incoming bytes arrive by `telnet_recv`; outgoing bytes leave as `TELNET_EV_SEND` events that the handler must write out (synchronously, since the handler returns `void` and the bytes are borrowed). Both directions go through the same callback, and outgoing SEND events can be emitted from inside `telnet_recv` (e.g. `_negotiate` sends `IAC DO x` then emits `TELNET_EV_WILL`), so the handler can be re-entered for output while processing input.

zlib (MCCP2) sits inside the state object: `telnet_recv` inflates before `_process`, `_send` deflates before emitting SEND. When a COMPRESS2 SB ends mid-buffer, `_process` recursively calls `telnet_recv` on the rest of the buffer so it is inflated.

### How protocol / option-negotiation state is held

All in `struct telnet_t` (`libtelnet.c`):

- `ud`, `telopts`, `eh`, `flags` — from `telnet_init`.
- `state` — a byte-level parser state (`TELNET_STATE_DATA, EOL, IAC, WILL, WONT, DO, DONT, SB, SB_DATA, SB_DATA_IAC`). Survives across `telnet_recv` calls, so a sequence split across reads resumes correctly.
- `buffer`, `buffer_size`, `buffer_pos` — the subnegotiation accumulator; grows through `{0, 512, 2048, 8192, 16384}` and emits a `TELNET_EOVERFLOW` warning past 16384 bytes.
- `sb_telopt` — option of the SB in progress.
- `q`, `q_size`, `q_cnt` — the RFC 1143 "Q method" table: a growable array of `{telopt, state}` where `state` packs `us` and `him` each in `Q_NO, Q_YES, Q_WANTNO, Q_WANTYES, Q_WANTNO_OP, Q_WANTYES_OP`. Entries are created lazily on first state change; unknown options read as `Q_NO/Q_NO`.
- `z` — zlib stream.

Policy is the static `telnet_telopt_t` table passed to `telnet_init` (`{telopt, us: WILL|WONT, him: DO|DONT}`, terminated by `telopt = -1`). On an incoming WILL/DO for an option in state NO, `_negotiate` consults the table: supported → set YES, send DO/WILL, emit the event; unsupported → send DONT/WONT, no event, no state entry. Local requests go through `telnet_negotiate`, which runs the other half of RFC 1143 (sets WANTYES/WANTNO/…_OP and sends only when not redundant — "may be ignored if they are determined to be redundant"). The application never sees raw WILL/DO for options it did not enable; it sees a WILL/DO event only when an option becomes enabled on that side, WONT/DONT when it becomes disabled. `_set_rfc1143` also mirrors the BINARY option's state into the `TRANSMIT_BINARY`/`RECEIVE_BINARY` flags, which `telnet_send_text` and NVT EOL handling read. Protocol violations (e.g. "DONT answered by WILL") are reported as WARNING events and the state is still updated.

### Where the seam sits

Between bytes and events, both directions, synchronous:

- In: `&[u8]` → `telnet_recv` → zero or more callback invocations.
- Out: API call → zero or more `TELNET_EV_SEND` callbacks carrying `&[u8]`.
- The protocol state object is fully passive: nothing happens unless the application calls into it. There is no notion of time, no blocking, and no queue — every output is pushed to the handler immediately.

## russh (client)

### Entry points

- `pub async fn connect<H: Handler + Send + 'static, A: ToSocketAddrs>(config: Arc<Config>, addrs: A, handler: H) -> Result<Handle<H>, H::Error>` — opens a `tokio::net::TcpStream`, optionally sets `nodelay`, then calls `connect_stream` (`client/mod.rs`, not on wasm32).
- `pub async fn connect_stream<H, R>(config: Arc<Config>, stream: R, handler: H) -> Result<Handle<H>, H::Error>` where `R: AsyncRead + AsyncWrite + Unpin + Send + 'static`. It writes the SSH identification string, reads the server's (`SshRead::read_ssh_id`), builds a `Session`, calls `session.begin_rekey()`, then **spawns** `session.run(stream, handler, Some(kex_done_signal))` via `russh_util::runtime::spawn`, and awaits a oneshot fired when the first key exchange finishes. It returns `Handle<H>` only after that.
- `Handle<H>` holds `sender: mpsc::Sender<Msg>` (bounded, capacity 10), `receiver: UnboundedReceiver<Reply>`, the task's `JoinHandle`, and `channel_buffer_size`. Its methods are `async` and send `Msg` values to the task: `authenticate_*` (then await a `Reply`), `channel_open_session` / `_x11` / `_direct_tcpip` / `_direct_streamlocal`, `tcpip_forward`, `disconnect`, `data`, `send_keepalive`, `rekey_soon`, etc. `Handle<H>` also implements `Future<Output = Result<(), H::Error>>` by polling the join handle.
- `Channel<Msg>` (from `channel_open_session`, `channels/mod.rs`): holds a bounded `mpsc` receiver of `ChannelMsg` and a sender back to the session task. Methods: `wait() -> Option<ChannelMsg>`, `data`, `data_bytes`, `request_pty`, `request_shell`, `exec`, `window_change`, `eof`, `close`, `split()` into `ChannelReadHalf`/`ChannelWriteHalf`, `make_reader()`/`make_writer()` (`AsyncRead`/`AsyncWrite` adapters), `into_stream()`.

### Event / callback type

`pub trait Handler: Sized + Send` with `type Error: From<crate::Error> + Send + Debug`. It has 27 methods. Every method takes `&mut self`; all but `adjust_window` return `impl Future<Output = Result<_, Self::Error>> + Send` (optionally via `async_trait` feature), and has a default body. Most take `session: &mut Session`. Examples read from the source:

- `auth_banner(&mut self, banner: &str, session: &mut Session)`
- `check_server_key(&mut self, server_public_key: &PublicKeyOrCertificate) -> Result<bool, _>` — default rejects all keys; the doc says "You must at the very least implement" it.
- `kex_done(&mut self, shared_secret: Option<&[u8]>, names: &negotiation::Names, session: &mut Session)`
- `channel_open_confirmation`, `channel_success`, `channel_failure`, `channel_close`, `channel_eof`, `channel_open_failure`
- `server_channel_open_forwarded_tcpip`, `…_streamlocal`, `…_agent_forward`, `…_session`, `…_direct_tcpip`, `…_x11`, `should_accept_unknown_server_channel`, `server_channel_open_unknown`
- `data(&mut self, channel: ChannelId, data: &[u8], session: &mut Session)`, `extended_data(…, ext: u32, …)`, `xon_xoff`, `exit_status`, `exit_signal`, `window_adjusted`
- `adjust_window(&mut self, channel, window: u32) -> u32` (sync, not a future)
- `openssh_ext_host_keys_announced`
- `disconnected(&mut self, reason: DisconnectReason<Self::Error>)` — default returns `Ok(())` on `ReceivedDisconnect`, re-returns the error on `Error`.

Data is delivered twice for a channel the application opened: `client/encrypted.rs`, on `CHANNEL_DATA`, first does `chan.send(ChannelMsg::Data { data: data.clone() }).await` to the `Channel`'s bounded receiver, then calls `client.data(channel_num, &data, self).await` on the Handler. So the application may consume data either through the `Channel` object or through the Handler.

### Who owns I/O

The library, inside a spawned task. `Session::run` splits the stream (`stream.split()` into read/write halves) and runs `run_inner`, a `tokio::select!` loop over:

1. the pending read of the next packet (`start_reading` → `cipher::read`),
2. a keepalive timer and an inactivity timer (`tokio::time::sleep`, from `Config::keepalive_interval`, `inactivity_timeout`),
3. `self.receiver` (messages from `Handle`, and from the `Channel`s it opens), `self.priority_receiver`, and `self.inbound_channel_receiver` (messages from `Channel`s for server-initiated channel opens; `client/encrypted.rs` hands `inbound_channel_sender.clone()` to `Channel::new` there) — `receiver` and `inbound_channel_receiver` are gated off while a key exchange is active or data is pending on a window-blocked channel.

After each branch: `self.flush()` (encrypts queued packets into `common.packet_writer`) then `flush_or_timeout(&mut packet_writer, stream_write, …)` writes them to the socket (`PacketWriter::flush_into` does `write_all` + `flush`). On exit it closes the receivers, `shutdown()`s the write half and calls `handler.disconnected(...)`.

The Handler is owned by the task (`handler: H` moved into `run`) and is awaited inline: incoming packet → `reply(self, handler, …)` → dispatch in `client/encrypted.rs` → `handler.<method>(…, self).await`. While a Handler future is pending, the loop does not read further packets or service the `Handle`. Inside a callback the `&mut Session` methods (`client/session.rs`: `data`, `request_shell`, `exec`, `channel_open_session`, `eof`, `close`, `window_change`, …) are **synchronous** and only encode packets into the session's write buffer (`push_packet!(enc.write, …)` / `data_with_writer(&mut common.packet_writer, …)`); they are flushed to the socket after the callback returns.

### How protocol state is held

`client::Session` (`client/mod.rs`) owns: `kex: SessionKexState<ClientKex>` (key-exchange state machine), `common: CommonSession<Arc<Config>>` (`session.rs`: `packet_writer`, `remote_to_local` cipher, `encrypted: Option<Encrypted>` holding per-channel state and windows, auth state, `disconnected`, `strict_kex`, `alive_timeouts`, `remote_sshid`), `channels: HashMap<ChannelId, ChannelRef>` (the senders to each `Channel`), `target_window_size`, `pending_reads`, the mpsc endpoints listed above, `open_global_requests: VecDeque<…>`, `server_sig_algs`. Algorithm negotiation (the SSH analogue of option negotiation) runs inside the task during kex; the application is told the outcome through `kex_done(names, …)` and consulted only for the host key (`check_server_key`). Flow-control windows are adjusted by the library, with the Handler asked for the new target via `adjust_window`.

### Where the seam sits

russh has no public sans-IO layer in the files read: `Session` is constructed only inside `connect_stream`, `run`/`run_inner` are private, and parsing, encryption, the `select!` loop and socket writes are interleaved in the same `async fn`s. The seams the caller does get are:

- below: any `AsyncRead + AsyncWrite + Unpin + Send + 'static` stream (`connect_stream`), i.e. the transport is pluggable but it must be a tokio async stream;
- above: the `Handler` trait (callbacks awaited inside the I/O task, with sync `&mut Session` for replies) and the `Handle` / `Channel` message-passing objects (used from other tasks, all `async`).

## Side by side

| | libtelnet | russh client |
|---|---|---|
| Instantiation | `telnet_init(telopts, eh, flags, ud)`; nothing sent | `connect` / `connect_stream(config, stream, handler)`; performs id exchange + first kex before returning |
| Input path | caller calls `telnet_recv(bytes)` | library task reads the stream itself |
| Output path | `TELNET_EV_SEND` callback with bytes; caller writes them | library task writes the stream; callers enqueue via `Handle`/`Channel` (async mpsc) or `&mut Session` (sync, inside callbacks) |
| Callback shape | one C fn pointer + tagged union, `void` return, sync | trait with 27 methods, nearly all `async`, returning `Result`, most with defaults |
| Who calls the callbacks | `telnet_recv` and the send APIs, on the caller's stack | the spawned session task, awaited inline |
| Timers | none | keepalive + inactivity timers in the task |
| Concurrency | none; single-threaded by construction | tokio task + mpsc channels (bounded `Msg` 10, bounded per-channel `ChannelMsg`, unbounded `Reply`/priority) |
| Negotiation state | `telnet_t.q` RFC 1143 table + static `telopts` policy table; auto-replies inside `telnet_recv` | `SessionKexState` + `CommonSession`; auto-handled inside the task; app asked only `check_server_key` |
| Backpressure | none (caller controls reads) | per-channel bounded mpsc: `chan.send(...).await` in the read path blocks the loop if a `Channel` is not drained; outbound receivers are paused while a channel is window-blocked |
| Transport | none (bytes only) | any tokio `AsyncRead + AsyncWrite` |
| Error reporting | WARNING/ERROR events (errcode not set) | `Result` from every `Handle` method + `Handler::disconnected` + `Handle` as `Future` |

## Rust precedent for the sans-IO split

### quinn-proto / quinn

`quinn-proto/src/lib.rs` crate doc: "contains a fully deterministic implementation of QUIC protocol logic. It contains no networking code and does not get any relevant timestamps from the operating system." `quinn-proto::Connection` (`connection/mod.rs`):

- input: `handle_event(&mut self, event: ConnectionEvent)` (datagrams, routed by `Endpoint::handle(...)`), `handle_timeout(&mut self, now: Instant)`;
- output: `poll_transmit(&mut self, now: Instant, max_datagrams: usize, buf: &mut Vec<u8>) -> Option<Transmit>` (bytes to send, written into the caller's buffer), `poll_timeout(&self) -> Option<Instant>` (when to call `handle_timeout`), `poll(&mut self) -> Option<Event>` (application events, drained from an internal queue), `poll_endpoint_events`;
- streams: `streams()`, `send_stream(id)`, `recv_stream(id)`, `datagrams()` — synchronous accessors on the state.

Time is passed in as `now`; the state never reads a clock. `quinn/src/connection.rs` is the tokio driver: `drive_transmit` loops `poll_transmit` and sends; `drive_timer` calls `handle_timeout(self.runtime.now())`; incoming events go to `self.inner.handle_event(event)`; `while let Some(event) = self.inner.poll()` wakes application tasks. Shape: pull-based (caller polls for output/events), unlike libtelnet's push callbacks.

### rustls / tokio-rustls

tokio-rustls 0.26.5 (against rustls 0.23.27), `src/common/mod.rs`: `read_io` wraps the async stream in a `SyncReadAdapter` and calls `session.read_tls(&mut reader)` (mapping `WouldBlock` to `Poll::Pending`), then `session.process_new_packets()`; `write_io` calls `session.write_tls(&mut SyncWriteAdapter)`. So in 0.23 the state object takes `&mut dyn io::Read` / `&mut dyn io::Write`, and the async crate adapts a tokio stream into those. Shape: state object with read-in / process / write-out calls; plaintext via the connection's reader/writer.

rustls `main` (0.24.0-dev.1, `rustls/src/conn/mod.rs`) is mid-redesign: the `Connection` trait there has `write(&mut self, plaintext: OutboundPlain<'_>, tls: &mut Vec<u8>)`, `read_tls(&mut self, input: &mut dyn TlsInputBuffer, tls: &mut Vec<u8>) -> MessageHandler`, `wants_read()`, `send_close_notify(&mut self, tls: &mut Vec<u8>)` — output bytes are appended to a caller-supplied `Vec<u8>` rather than written to an `io::Write`. This is unreleased; the released 0.23 method signatures were confirmed only via tokio-rustls call sites, not by reading rustls 0.23's own source.

## Gaps / needs confirming

- rustls 0.23's `read_tls`/`process_new_packets`/`write_tls` signatures: inferred from tokio-rustls's calls, not read in the rustls 0.23 tree.
- russh: only the client module and the shared files listed were read. Whether russh exposes any lower-level (non-spawned) API elsewhere in the crate was not checked beyond these files; `Session::new` and `run` are private in `client/mod.rs`.
- russh's per-channel `chan.send(...).await` blocking the read loop is read from source (`client/encrypted.rs`); its runtime effect was not observed with a probe.
- libtelnet's `develop` branch was read; whether a tagged release differs was not checked.
- The `quinn` driver's exact structure was confirmed by grep of call sites in `quinn/src/connection.rs`, not a full read.
