# Transcripts

A **Transcript** is a recorded Telnet conversation kept as data: what the server sent, what the Core must answer, and which Events it must emit. Each one is a `.txt` file under `justelnet-core/tests/transcripts/`, and every file there runs on every pull request:

- `tests/transcripts.rs` replays it through the Core's public API twice, once with each step's server bytes in one chunk and once a byte at a time;
- `tests/chunking.rs` replays it with each step's server bytes split at arbitrary points.

Adding a file is all it takes; no code changes. This page is the format the runner (`justelnet-core/tests/common/mod.rs`) reads, and the way to turn a capture from a real server into one.

## The format

One directive per line, `kind: value`. Blank lines and lines starting with `#` are ignored.

### The Option policy

Policy lines come before the first step and apply in order. Without any, the Transcript runs against `OptionPolicy::default()`, active start included.

| Line | Meaning |
|---|---|
| `policy: empty` | refuse every option on both sides and start passive |
| `policy: passive` | start passive |
| `accept: <side> <option>` | accept the option on that side |
| `request: <side> <option>` | request the option on that side at the start |
| `refuse: <side> <option>` | refuse the option on that side |
| `terminal-types: <name> <name> …` | the TTYPE list |
| `window-size: <width> <height>` | the NAWS window size |
| `end-of-line: crlf\|crnul\|lf` | what is sent for a CR |
| `passthrough: <option>` | a Passthrough option |
| `variable: "<name>" "<value>"` | a NEW-ENVIRON VAR |
| `user-variable: "<name>" "<value>"` | a NEW-ENVIRON USERVAR |

### Steps

A Transcript is a sequence of steps. A `server:` or `call:` line starts a new one; the lines after it, up to the next, say what the Core must do during that step. Lines before the first `server:` or `call:` form a step with no server bytes, which is where the bytes of an active start go.

| Line | Meaning |
|---|---|
| `server: <hex bytes>` | bytes the peer sends; starts a step |
| `call: enable\|disable <side> <option>` | a runtime request from the caller; starts a step |
| `call: window <width> <height>` | a window-size change; starts a step |
| `call: data "<text>"`, `call: raw "<text>"` | sending data; starts a step |
| `call: command <command>` | sending a command; starts a step |
| `call: subnegotiation <option> <hex bytes>` | sending a raw subnegotiation; starts a step |
| `error: not-enabled\|not-passthrough <option>` | the Error a `call: subnegotiation` must return; without it, the call must succeed |
| `client: <hex bytes>` | bytes the Core must send during the step; several lines add up |
| `event: data "<text>"` | a Data Event |
| `event: option <side> <option> on\|off` | an OptionChanged Event |
| `event: command <command>` | a Command Event |
| `event: subnegotiation <option> <hex bytes>` | a Subnegotiation Event |
| `event: warning malformed <option> <hex byte>` | a malformed-subnegotiation Warning |
| `event: warning truncated <option>` | a truncated-subnegotiation Warning |
| `event: warning unknown-command <hex byte>` | an unknown-command Warning |
| `event: warning noncompliant <side> <option>` | a noncompliant-answer Warning |
| `terminal-type-sent: <name>\|none` | the terminal type the Core reports having sent, at the end of the step |

The `client:` bytes and the Events of a step must match exactly, in order. Adjacent Data Events are merged on both sides before comparing, so how the server bytes were chunked never matters; the Core must never emit an empty one.

### Values

- **Hex bytes**: two-digit hex separated by spaces, `ff fd 18`. `xx*N` is the byte `xx` repeated N times.
- **A side**: `local` (this end) or `remote` (the peer).
- **An option**: `BINARY`, `ECHO`, `SGA`, `STATUS`, `TM`, `TTYPE`, `NAWS`, `TSPEED`, `LFLOW`, `LINEMODE`, `XDISPLOC`, `OLD-ENVIRON`, `NEW-ENVIRON`, `COM-PORT`, or any option's code as two-digit hex.
- **A command**: `NOP`, `DM`, `BRK`, `IP`, `AO`, `AYT`, `EC`, `EL`, `GA`.
- **Text**: in double quotes, with the escapes `\r`, `\n`, `\\`, `\"` and `\xNN`.

## From a capture to a Transcript

### What each kind of capture holds

| Capture | What it holds | What it lacks |
|---|---|---|
| a silent client's hex dump (`nc` piped to `xxd`, or the script below) | the server's bytes, exact | everything after the server's first burst |
| a PuTTY log | the negotiation both ways, in order, as names | bytes: option codes PuTTY has no name for, data, subnegotiation contents |
| the end-to-end recording (`JUSTELNET_E2E_RECORDING`, the `inetutils-telnetd-recording` artifact) | both sides' bytes, in order, as `server:` and `client:` lines | the `event:` lines |

**A silent capture stops at the server's first burst.** A telnetd sends its first requests and then waits for the answers, so a client that answers nothing records only those requests, however long it listens. Everything after depends on what the client answered.

**Later server bytes are only as good as the answers they followed.** A capture from a client that answered differently from the Core (PuTTY offers options the default policy does not, and refuses others) holds what the server said to *that* client. Use it only up to the first answer where the two differ.

### 1. Capture

Answer nothing and record what the server sends for about ten seconds:

```sh
nc -w 10 <host> 23 | xxd
```

Or, where `nc` is not at hand:

```python
import socket, sys, time
s = socket.create_connection((sys.argv[1], 23), timeout=5)
s.settimeout(1)
data, end = b"", time.time() + 10
while time.time() < end:
    try:
        chunk = s.recv(4096)
    except socket.timeout:
        continue
    if not chunk:
        break
    data += chunk
open("capture.bin", "wb").write(data)
print("server:", data.hex(" "))
```

which also prints the bytes as a `server:` line.

Note the server's implementation (inetutils, netkit, BSD, the device's OS), its version and the date.

### 2. Write the server's bytes

Turn the dump into a `server:` line:

```sh
xxd -p capture.bin | tr -d '\n' | sed 's/../& /g; s/ $//'
```

Start a new `server:` line wherever the server waits for an answer, and only there; a step is "the server said this, the Core answered that".

**Strip everything after the login prompt**, and keep out any address, user name or password. A Transcript is committed and public.

### 3. Write what the Core must do

Begin the file with a comment saying where the bytes came from: the server, its version, the date, how it was captured and the policy. Add `policy:` lines only if the capture was made for a policy other than the default.

Then run the replay with only the `server:` lines written:

```sh
cargo test -p justelnet-core --test transcripts
```

It fails, and the failure shows what the Core did (`left`) next to what the file expects (`right`). The bytes print in **decimal**: `255, 252, 32` is `ff fc 20`, IAC WONT TSPEED.

**Check each byte and Event against the RFCs and the policy before writing it down.** Copying `left` into the file makes a Transcript that records what the Core happens to do, and it can never fail. A byte the Core gets wrong is a Transcript worth keeping: write what it should do, watch it fail, and fix the Core.

| Byte | | Byte | |
|---|---|---|---|
| `ff` | IAC | `fa` | SB |
| `fb` | WILL | `f0` | SE |
| `fc` | WONT | `01` | ECHO |
| `fd` | DO | `03` | SGA |
| `fe` | DONT | `18` | TTYPE |
| `1f` | NAWS | `27` | NEW-ENVIRON |

### 4. Run it

```sh
cargo test -p justelnet-core --test transcripts --test chunking
```

## A worked example

[`telnetd-netkit-rhel9-silent-capture.txt`](../justelnet-core/tests/transcripts/telnetd-netkit-rhel9-silent-capture.txt) comes from a netkit telnetd on RHEL 9, captured with the script above on 2026-10-08. In ten seconds the server sent twelve bytes and nothing more:

```
00000000: fffd 18ff fd20 fffd 23ff fd27            ..... ..#..'
```

That is `IAC DO TTYPE, IAC DO TSPEED, IAC DO XDISPLOC, IAC DO NEW-ENVIRON`, and as one line:

```
server: ff fd 18 ff fd 20 ff fd 23 ff fd 27
```

With only that line in the file, the replay fails with:

```
left: Outcome { client: [255, 251, 31, 255, 251, 24, 255, 251, 39, 255, 253, 1, 255, 251, 3, 255, 253, 3, 255, 252, 32, 255, 252, 35], events: [OptionChanged { option: TelnetOption(24), side: Local, enabled: true }, OptionChanged { option: TelnetOption(39), side: Local, enabled: true }] }
right: Outcome { client: [], events: [] }
```

Read against the default policy:

- `ff fb 1f ff fb 18 ff fb 27 ff fd 01 ff fb 03 ff fd 03` is the active start, WILL NAWS, TTYPE and NEW-ENVIRON, DO ECHO, WILL SGA, DO SGA. The Core sends it before any input, so it goes in a step before the first `server:` line.
- DO TTYPE and DO NEW-ENVIRON agree with WILLs the Core already sent, so they need no answer and turn both options on: two OptionChanged Events, `local`.
- `ff fc 20 ff fc 23` refuses TSPEED and XDISPLOC, which the default policy does not support.

So the Transcript is:

```
client: ff fb 1f ff fb 18 ff fb 27 ff fd 01 ff fb 03 ff fd 03

server: ff fd 18 ff fd 20 ff fd 23 ff fd 27
client: ff fc 20 ff fc 23
event: option local TTYPE on
event: option local NEW-ENVIRON on
```

Going further than this burst takes a capture where the client answered as the Core does. [`telnetd-netkit-rhel9.txt`](../justelnet-core/tests/transcripts/telnetd-netkit-rhel9.txt) is the same server through to its login prompt, captured with the Core itself answering.

## From a PuTTY log

Logging set to "SSH packets and raw data" writes a telnet session's negotiation as Event Log lines, in the order it happened:

```
Event Log: client negotiation: WILL NAWS
Event Log: server negotiation: DO TTYPE
Event Log: server subnegotiation: SB TTYPE SEND
Event Log: client subnegotiation: SB TTYPE IS XTERM
```

Each `server negotiation:` line is three bytes: IAC, then `fb` WILL, `fc` WONT, `fd` DO or `fe` DONT, then the option's code. PuTTY writes `OLD_ENVIRON`, `NEW_ENVIRON` and `COM_PORT_OPTION` with underscores, and an option it has no name for as `<unknown>`, which loses its code; that one needs a silent capture.

The log holds no data and no subnegotiation bytes beyond these descriptions ("All session output" logs data, after Telnet commands are removed, and not in the same file). Use it to see the order of the negotiation, and take the bytes from a silent capture or the end-to-end recording where they matter.

## From the end-to-end recording

The recording is already in `server:` and `client:` lines: each line is everything one side sent until the other side next sent something, so it reads as steps already. In the runs so far the first line is a `client:` line holding the active start. To make it a Transcript:

1. Cut it at the login prompt. What follows carries the test account's name and password, which the Transcript does not need.
2. Under each `server:` line, add the `event:` lines the Core must emit, checked as in step 3. The prompt itself is an `event: data "…"`.
3. Check that each `client:` line is the Core's answer to the `server:` line above it. Bytes the caller sent, such as the user name, are a `call: data` step of their own, not an answer.

## From a fuzz finding

A crashing input from the fuzzer becomes a Transcript too; [CONTRIBUTING.md](../CONTRIBUTING.md#a-crashing-input-becomes-a-transcript) has how to read one.
