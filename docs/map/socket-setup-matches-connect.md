# Socket setup matches connect

## The fact

Every TCP socket a justelnet client runs over keeps urgent data inline (`SO_OOBINLINE`) and sends without delay (`TCP_NODELAY`), exactly as `Client::connect` sets them. Checkable: each site that builds a `TcpStream` for a `Client` calls `set_out_of_band_inline(true)` and `set_nodelay(true)` before handing it on.

```sh
rg -n "set_out_of_band_inline|set_nodelay" justelnet justelnet-expect
```

## Why it is cross-cutting

`Client::connect` sets both, but a caller with its own stream uses `Client::new`, which cannot reach the socket — the stream is generic. Nothing shares a function between the sites: the e2e test copies the two calls because it wraps its stream in a recorder before handing it to `Client::new`, and a public helper both would call was declined (maintainer, 2026-10-22, #88). So the rule is held only by `Client::new`'s doc asking for it and by each site repeating it.

Why urgent data has to stay inline: the Core sees only a byte stream and cannot repair what the urgent mark takes out of it ([receive parsing](receive-parsing.md), "DM is reported"). A netkit telnetd on RHEL 9 sends its Synch as TCP urgent data around the login banner; read on Windows through a socket without `SO_OOBINLINE` (probed 2026-09-29), the IAC was taken out of the stream and a lone `f2` arrived as data, in one of two runs. On BSD-semantics stacks the DM byte is the one taken, leaving a bare IAC to swallow the next byte (not observed).

Why no delay: PuTTY's default (`tcp_nodelay`, `conf.h`), so each keystroke goes out at once. The maintainer chose it on 2026-09-29 on PuTTY's default alone, with nothing measured; it is theirs to reverse.

## Territories it holds in

- [Driving a stream](driving-a-stream.md) — `justelnet/src/lib.rs — Client::connect` sets both; `Client::new`'s doc asks a TCP caller for `SO_OOBINLINE`.
- [Logging in to a real server](logging-in-to-a-real-server.md) — `justelnet-expect/tests/real_telnetd.rs — a_login_to_inetutils_telnetd` sets both on its `TcpStream` before wrapping it in `Recorder`.
- [Receive parsing](receive-parsing.md) — the reader that depends on it: `justelnet-core/src/lib.rs — Core::command` reports `IAC DM` as `Command::DataMark` only when both bytes arrive; it cannot repair a stream the urgent mark cut.
- Test-only: `justelnet/tests/connect.rs — urgent_data_arrives_inline` sends a byte with `send_out_of_band` through `connect` and requires it among the Data; without the setting it is lost.

## What a violation looks like

On a connection where the server sends a Synch (netkit sent one on about 13% of connections, 5 in 30, below), a socket without `SO_OOBINLINE` loses a byte: on Windows the IAC, so a lone `f2` appears in Data — a stray byte in a prompt that a regex may then miss; on BSD stacks the DM, so a bare IAC swallows the next byte. It is intermittent, which is how a flaky e2e run would look. Without `TCP_NODELAY` nothing visible breaks on a short path (measured below).

## Discovery history

- 2026-09-29, netkit telnetd on RHEL 9 from Windows, 30 connections each: through `connect`, the Synch arrived whole as a DataMark Command in 5 and never as a stray byte; through a plain `TcpStream`, it never arrived whole and put a lone `f2` into the Data in 2. The server sends the Synch only on some connections, so the counts are small; the local test is the deterministic proof.
- After 2026-09-29, against the lab telnetd over a LAN: two keystrokes back to back echoed in a median 0.90 ms with `TCP_NODELAY` and 1.23 ms without (maximum 40 ms and 43 ms, 10 each); on loopback the two were the same — no gain shown on a short path, where the delayed ACK that Nagle waits for comes back at once (maintainer measurement). No test covers the setting, since the client does not expose its socket and a timing threshold would not hold on CI.
- 2026-10-22, #85/#88: the e2e test built its client from a plain `TcpStream`, running the configuration the docs tell callers not to use; fixed by commit `797c8ab`. Its recording up to the login prompt held no Synch before the fix (six runs), so the change matches the docs rather than fixing a failure seen there. On 2026-10-22 the lab sent no Synch in 11 connections from the e2e test with both settings made (each reached the login prompt), which is within the 5-in-30 rate.

## Where it will recur

If code creates a `TcpStream` (or any socket) and hands it to `Client::new` — a new test, an example, a future TLS or proxy path — it is subject to this: set both options, or say why not. A future public helper for socket setup would turn this from an invariant into a design line of [driving a stream](driving-a-stream.md).
