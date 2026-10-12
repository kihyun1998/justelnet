# Target options and quirks: network gear and Unix telnetd

Answers #2 (part of map #1): which telnet options each first-connection target
offers or requests on connect, and which non-RFC behaviours a client must
tolerate.

## Gist

- **Unix telnetd (netkit, GNU inetutils, BSD) opens in two blocking phases.**
  Phase 1 asks `DO TTYPE, DO TSPEED, DO XDISPLOC, DO NEW-ENVIRON` (and, on BSD
  and inetutils, `DO OLD-ENVIRON`), then sends `SB … SEND` for each one the
  client accepted. Phase 2 sends `WILL SGA, DO ECHO, [DO LINEMODE], DO NAWS,
  WILL STATUS, DO LFLOW`, then `WILL ECHO` and, on BSD and inetutils, `DO TM`.
  At each step the server reads from the socket with no timeout until the
  client answers. The session does not start until every `DO`/`WILL` gets a
  reply and every accepted `SEND` gets an `IS`.
- **Cisco IOS vty** is reported to send `WILL ECHO, WILL SGA, DO TTYPE, DO NAWS`,
  but this comes from a forum thread, not a capture. Treat it as unconfirmed.
  For reverse-telnet (console) lines, IOS documents knobs for end-of-line
  (`telnet transparent`), BREAK/IP mapping and refusing ECHO/SGA. These knobs
  show that end-of-line handling and BREAK behaviour vary from device to
  device.
- **Console servers.** ser2net/gensio, read from source, sends
  `WILL SGA, DO SGA, WILL ECHO, DONT ECHO, DO BINARY, WILL BINARY` (plus
  `DO COM-PORT-OPTION` and/or `DO NAWS` if enabled). The whole session then
  runs in binary mode, so there is no CR NUL stuffing.
- **Quirks a compliant client must tolerate:**
  - servers that block until every negotiation is answered;
  - `DO ECHO` sent to the client as a 4.2BSD probe;
  - BSD "kludge linemode", which overloads `TM` and `SGA`;
  - OLD-ENVIRON VAR/VALUE codes that are reversed on some peers;
  - the server hanging if the client accepts both ENVIRON options;
  - `IAC <not SE>` inside a subnegotiation;
  - a `DONT` answering a `WONT` the server already agreed with;
  - Synch/DM sent as TCP urgent data.
- **Nothing observed on the wire.** No live device or daemon was probed. The
  Unix telnetd and ser2net rows come from reading their source. The Cisco,
  Juniper and commercial console-server rows come from documentation or forums
  and are marked **needs confirming against the real thing**.

## Sources and how they were reached

| Tag | Source | Binding (per `docs/agents/thegraph.md`) | Reached by |
|---|---|---|---|
| RFC 854/855/856/857/858/860/1073/1079/1091/1123/1184/1408/1571/1572/2217 | rfc-editor.org `.txt` | spec (binding) | raw text, grepped |
| PuTTY | `otherbackends/telnet.c`, `doc/config.but` from the `github/putty` mirror (last commit 2020-09-13), checked by diff against the 0.85 release tarball (negotiation logic identical) | example | raw source, read in full |
| netkit | netkit-telnet 0.17-42 `telnetd/{telnetd,state,utility}.c`, `defs.h`, `Makefile` (Debian bullseye, via sources.debian.org) | observed target (source) | raw source |
| inetutils | GNU inetutils `telnetd/{telnetd,state,utility}.c` (savannah cgit, HEAD) | observed target (source) | raw source |
| NetBSD | NetBSD `libexec/telnetd/{telnetd,state,utility}.c` (trunk) | observed target (source); stands in for the 4.4BSD-derived family | raw source |
| gensio | cminyard/gensio `lib/telnet.c`, `lib/sergensio_telnet.c` (master); the telnet engine of ser2net | observed target (source) | raw source |
| Cisco docs | IOS 11.x "Telnet Configuration Commands" (employees.org univercd mirror of Cisco docs) | vendor doc (summarized fetch) | **needs confirming against the real thing** |
| Cisco forum | community.cisco.com thread titled `What does response "ff fb 01 ff fb 03 ff fd 18 ff fd 1f"` (page returned 403; only the title/search snippet was seen) | forum | **needs confirming against the real thing** |
| Opengear docs | Opengear user manual / quick-start (search snippets) | vendor doc (summarized) | **needs confirming against the real thing** |

FreeBSD's `contrib/telnet/telnetd` could not be fetched: it is gone from
`main`, and the `stable/14` raw path returned 404. NetBSD is used as the BSD
representative. Juniper Junos is FreeBSD-based, but its telnetd was not
available to read.

## Per-target table: what each side offers or requests on connect

"Server sends" lists what the server sends unprompted on connect, in wire
order. "Server accepts" lists client-initiated options the server says yes to.

| Target | Server sends on connect (in order) | Server accepts if client offers | Source |
|---|---|---|---|
| **netkit telnetd 0.17** (Debian ≤ bullseye `telnetd`) | Phase 1 (`getterminaltype`): `DO TTYPE`, `DO TSPEED`, `DO XDISPLOC`, `DO NEW-ENVIRON` (its `TELOPT_ENVIRON` is `#define`d to NEW-ENVIRON, 39). It blocks until all four are answered, then sends `SB TSPEED SEND`, `SB XDISPLOC SEND`, `SB NEW-ENVIRON SEND`, `SB TTYPE SEND` for each one accepted, and blocks for each `IS`. TTYPE is re-requested until an acceptable type appears or the list repeats. Phase 2 (`telnet()`): `WILL SGA`, `DO ECHO`, `DO NAWS`, `WILL STATUS`, `DO LFLOW`. It blocks until the NAWS answer arrives, then sends `WILL ECHO`. No `DO LINEMODE` and no `DO TM`: `LINEMODE` is never defined in the Debian build (the Makefile has only `-DKLUDGELINEMODE -DDIAGNOSTICS`). `DO AUTHENTICATION` and `WILL ENCRYPT` only if those are compiled in; they are off in this build. | WILL: TTYPE, SGA, NAWS, TSPEED, XDISPLOC, NEW-ENVIRON, LFLOW, BINARY (and ECHO is noted as "4.2 client"). DO: ECHO, BINARY, SGA, STATUS, TM (answered `WILL TM` and immediately treated as off again), LOGOUT (answers `WILL LOGOUT` then hangs up). | netkit `telnetd.c` L451-585, L747-861; `defs.h` L43-45; `Makefile` L11-12 |
| **GNU inetutils telnetd** (current Debian/Ubuntu `inetutils-telnetd`) | Phase 1 (`utility.c getterminaltype`): `WILL`/`DO AUTHENTICATION` if built with auth; `WILL ENCRYPT` if built with encryption; `DO TTYPE`, `DO TSPEED`, `DO XDISPLOC`, `DO NEW-ENVIRON`, `DO OLD-ENVIRON`; then SENDs as in netkit. Only **one** ENVIRON SEND goes out: NEW if accepted, otherwise OLD. Phase 2 (`telnetd_run`): `WILL SGA`, `DO ECHO`, `DO LINEMODE`, `DO NAWS`, `WILL STATUS`, `DO LFLOW`, (wait), `WILL ECHO`, `DO TM` (kludge-linemode probe, sent whenever real LINEMODE was not agreed). | Same family as netkit, plus OLD-ENVIRON and LINEMODE. | inetutils `utility.c` L703-809; `telnetd.c` L490-554 |
| **NetBSD telnetd** (4.4BSD family; FreeBSD/Junos presumed similar, **needs confirming**) | Phase 1: `DO AUTHENTICATION` (if `auth_level >= 0`, built with auth), `WILL ENCRYPT` (if built), `DO TTYPE`, `DO TSPEED`, `DO XDISPLOC`, `DO NEW-ENVIRON`, `DO OLD-ENVIRON`; SENDs as in inetutils (one ENVIRON SEND). Phase 2: `WILL SGA`, `DO ECHO`, `DO LINEMODE`, `DO NAWS`, `WILL STATUS`, `DO LFLOW`, (wait), `WILL ECHO`, `DO TM`. | WILL: BINARY, ECHO (4.2 detection → replies `DONT ECHO`), TM (kludge-linemode signal; never answered), LFLOW, TTYPE, SGA, NAWS, TSPEED, XDISPLOC, NEW/OLD-ENVIRON, LINEMODE, AUTH/ENCRYPT if built. DO: ECHO, BINARY, SGA, STATUS, TM, LOGOUT, ENCRYPT. | NetBSD `telnetd.c` L487-594, L743-857; `state.c` L445-941 |
| **ser2net / gensio (server mode)**: the software on Linux console servers and many DIY terminal servers | `WILL SGA`, `DO SGA`, `WILL ECHO`, `DONT ECHO`, `DO BINARY`, `WILL BINARY`; plus `DO COM-PORT-OPTION` (RFC 2217) if `rfc2217` is enabled and `DO NAWS` if `winsize` is enabled. Nothing else. No TTYPE, no ENVIRON. | Only the options in its table: SGA, ECHO, BINARY, COM-PORT-OPTION, NAWS. Anything else: `WILL x` → `DONT x`, `DO x` → `WONT x`. | gensio `sergensio_telnet.c` L790-840; `telnet.c` L60-133 |
| **Cisco IOS vty** (router/switch login) | Reported: `WILL ECHO`, `WILL SGA`, `DO TTYPE`, `DO NAWS` (bytes `ff fb 01 ff fb 03 ff fd 18 ff fd 1f`). | unknown | Cisco forum title only. **needs confirming against the real thing** |
| **Cisco IOS reverse telnet** (async/aux lines, TCP 2000+line, rotary 3000+group) | Not documented in what was read. Per-line knobs change behaviour: `telnet refuse-negotiations` (refuse remote ECHO / SGA), `telnet transparent` (send CR as CR NUL instead of CR LF), `telnet break-on-ip` (turn received IAC IP into a hardware BREAK on the serial line), `telnet sync-on-break` (send Synch on receiving BREAK), `telnet speed` (speed negotiation; the IOS doc says it "adheres to the Remote Flow Control option, defined in RFC 1080"). | unknown | Cisco IOS 11.x Telnet command reference (summarized fetch). **needs confirming against the real thing** |
| **Juniper Junos** (`set system services telnet`) | Not documented in what was read. Junos is FreeBSD-derived, so it is plausibly the BSD telnetd row. | unknown | assumption. **needs confirming against the real thing** |
| **Opengear and other commercial console servers** | Opengear documents per-port listeners: Telnet 2000+port, SSH 3000+port, raw TCP 4000+port, RFC 2217 5000+port. Negotiated options are not documented in what was read. | unknown | Opengear manual (search snippet). **needs confirming against the real thing** |

For comparison, the example client:

| Client | Sends on connect | Source |
|---|---|---|
| **PuTTY (active mode, default)** | `WILL NAWS`, `WILL TSPEED`, `WILL TTYPE`, `WILL NEW-ENVIRON`, `DO ECHO`, `WILL SGA`, `DO SGA`. OLD-ENVIRON and BINARY are only accepted when the server asks. In passive mode it sends nothing until the server negotiates, then adds `DO ECHO`, `WILL SGA`, `DO SGA` if they are still unset. | PuTTY `telnet.c` L145-169, L249-275, L742-756; `config.but` "Passive and active Telnet negotiation modes" |

## Quirks

Each quirk gives who shows it, whether it complies with or deviates from an
RFC, the RFC section, and what a client must do about it.

### Deviations from an RFC

1. **Server blocks the session on negotiation replies, with no timeout.**
   Shown by netkit, inetutils and NetBSD telnetd. `ttloop()` is a bare
   blocking `read()`: it exits on EOF and has no timer (netkit `utility.c`
   L76-97). The server loops until each `DO`/`WILL` is answered and each
   accepted SEND gets its `IS`. Login does not start until then.
   - *RFC:* not a violation as such. RFC 854 p.2 requires a response to state
     changes, so a client that never answers is the non-compliant side.
     *Client must:* answer every `DO`/`WILL` promptly, including unknown ones
     (RFC 1123 §3.2.1 MUST refuse unsupported options). Always answer an
     accepted `SEND`. A client that sits "passive" and never answers makes the
     server hang forever.
2. **Probing the client with `DO ECHO` (4.2BSD detection).** Shown by netkit,
   inetutils and NetBSD. The server asks the client to echo. If the client
   says `WILL ECHO`, the server labels it a 4.2 client and immediately sends
   `DONT ECHO`. If the client never answers, the server fakes receipt of
   `WILL ECHO` and sends `DONT ECHO` anyway ("simulating recv", netkit
   `telnetd.c` L821-839).
   - *RFC:* sending `DO ECHO` is legal (RFC 857 lets either side echo), but
     faking a reply that never came deviates from RFC 854 p.2 / RFC 1143
     state tracking. *Client must:* reply `WONT ECHO`, and tolerate an
     unsolicited `DONT ECHO`. That `DONT ECHO` is a request to enter a state
     the client is already in, so per RFC 854 p.2 rule b it must **not** be
     acknowledged.
3. **"Kludge linemode": `DO TM` probe and SGA overloaded as line mode.** Shown
   by inetutils and NetBSD (compiled with `LINEMODE`+`KLUDGELINEMODE`); not by
   netkit as built. When real LINEMODE is refused, the server sends `DO TM`.
   A `WILL TM` reply is taken to mean "client does kludge linemode", and the
   server then sends `WONT SGA` to switch the client into line-at-a-time.
   Later, a client `DO SGA` means "turn linemode off" and `DONT SGA` means
   "turn linemode on". The source itself says "This violates design of telnet.
   Gross. Very Gross." (NetBSD `state.c` L998-1018).
   - *RFC:* this deviates from RFC 858 (SGA only suppresses GA) and from
     RFC 860 (TM is a sync point, not a capability flag). RFC 1184 p.11-12
     documents this 4.3BSD ECHO/SGA reading as "not what the
     SUPPRESS-GO-AHEAD option is supposed to mean".
     *Client must:* answer `DO TM` with `WILL TM`, placed in the data stream
     per RFC 860, or with `WONT TM`. Saying `WILL TM` on connect opts in to
     kludge linemode. A character-mode client should either refuse TM at
     connect or be ready for `WONT SGA`.
4. **OLD-ENVIRON VAR and VALUE codes swapped.** RFC 1408 gave VAR=0,
   VALUE=1, but BSD implementations shipped VAR=1, VALUE=0 and never changed
   (PuTTY `config.but` "Handling of OLD_ENVIRON ambiguity"; RFC 1571 §
   "Background"). inetutils and NetBSD carry the `ENV_HACK` guesser (inetutils
   `state.c` L1329-1385). PuTTY guesses from the bytes in the server's SEND
   (`telnet.c` L413-433).
   - *RFC:* deviates from RFC 1408. RFC 1571 standardises the heuristic: a
     VALUE code seen inside SEND means the codes are reversed.
     *Client must:* prefer NEW-ENVIRON (RFC 1572), which is unambiguous. Speak
     OLD-ENVIRON only if the server refuses NEW-ENVIRON, and then apply the
     RFC 1571 heuristic.
5. **Accepting both ENVIRON options can hang the server.** Shown by inetutils
   and NetBSD. The server sends only one SEND (NEW if the client accepted it,
   otherwise OLD). But it then waits for an OLD-ENVIRON `IS` whenever the
   client said `WILL OLD-ENVIRON` (inetutils `utility.c` L779-803; NetBSD
   `telnetd.c` L548-583). A client that accepts both and only answers the
   NEW SEND leaves the server blocked in quirk 1. *This was inferred from
   reading the source and has not been observed; confirm with a probe.* PuTTY
   avoids it by allowing only one ENVIRON at a time. It refuses OLD once NEW
   is active, and falls back to offering OLD only if NEW is refused
   (`telnet.c` L282-303).
   - *RFC:* this is a server bug; nothing in RFC 1572 forbids both.
     *Client must:* never accept both. Answer `DONT`/`WONT` for OLD-ENVIRON
     once NEW-ENVIRON is agreed.
6. **Unsolicited acknowledgement of a negative.** Shown by gensio. On `WONT x`
   for an option it never asked about, it sends `DONT x` once (`sent_do`
   guard; the same pattern applies to `DONT` → `WONT`) (gensio `telnet.c`
   L91-131).
   - *RFC:* deviates from RFC 854 p.2 rule b ("a request to enter some mode it
     is already in … should not be acknowledged"). *Client must:* not answer
     a `DONT`/`WONT` for an option that is already off. A client that answers
     it creates exactly the loop the RFC rule exists to prevent. RFC 1143's Q
     method handles this.
7. **Malformed subnegotiation: `IAC <byte other than SE/IAC>` inside SB.**
   netkit and NetBSD telnetd end the SB there and treat the byte as a new
   command (NetBSD `state.c` L290-311). gensio drops both bytes (`telnet.c`
   L232-240). PuTTY keeps the byte and drops the IAC (`telnet.c` L595-597).
   - *RFC:* the sender deviates from RFC 855 (SB data must double IAC and end
     with IAC SE). *Client must:* choose a defined recovery; the three
     references disagree. Also double IAC in every SB the client sends,
     including NAWS values of 255 (RFC 1073; PuTTY `telnet.c` L873-880).
8. **Echo request refused on reverse-telnet lines.** Cisco IOS
   `telnet refuse-negotiations` "suppresses negotiation of the Telnet Remote
   Echo and Suppress Go Ahead options". **needs confirming against the real
   thing.**
   - *RFC:* this is compliant if the device answers with `WONT`/`DONT`
     (refusal is always legal, RFC 854). It deviates from RFC 1123 §3.2.2
     ("A User or Server Telnet MUST always accept negotiation of the Suppress
     Go Ahead option") if it refuses SGA. *Client must:* work with ECHO and
     SGA both off (NVT half-duplex defaults, local echo).
9. **BREAK and IP conflated.** Cisco IOS `telnet break-on-ip` turns a
   received IAC IP into a hardware BREAK on the serial line. The knob exists
   because some clients send IP where the user means BREAK. **needs
   confirming against the real thing.**
   - *RFC:* this deviates from RFC 854's distinct BRK and IP semantics
     (p.13-14). *Client must:* expose both IAC BRK and IAC IP to the caller
     and not substitute one for the other.

### RFC-compliant behaviour a client must still support

10. **CR NUL and CR LF on input; bare CR not stuffed in binary mode.**
    Unix telnetd strips the NUL or LF after CR when BINARY is off: CR LF →
    CR, CR NUL → CR (NetBSD `state.c` L101-143). PuTTY drops NUL after a
    received CR (`telnet.c` L515-541), sends a user CR as CR NUL (`telnet.c`
    L105-107, L836-839), and sends "Telnet New Line" CR LF for Return by
    default (`config.but` "Return key sends Telnet New Line"). It notes that
    "some servers do expect New Line, and some servers prefer to see ^M".
    Cisco `telnet transparent` exists to flip CR LF to CR NUL on a line.
    - *RFC:* compliant with RFC 854 p.11-12 and RFC 1123 §3.3.1 (a User
      Telnet MUST be able to send CR LF, CR NUL and LF, SHOULD default to
      CR LF, and SHOULD make it user-controllable). In BINARY no CR
      processing is allowed (RFC 1123 §3.2.7).
      *Client must:* make the end-of-line sequence caller-selectable (CR LF
      by default). On receive, strip CR NUL → CR only while the peer's
      BINARY is off. With gensio, which negotiates BINARY both ways, send
      CR unstuffed.
11. **TTYPE list cycling.** netkit, inetutils and NetBSD re-send
    `SB TTYPE SEND` until the server accepts a name or sees the same name
    twice, then send one more SEND to reset the list ("so that RFC1091
    compliant telnets will cycle back", netkit `telnetd.c` L529-581). netkit
    also rejects names containing `/`.
    - *RFC:* compliant with RFC 1091 (repeating the last name marks the end
      of the list; names are case-insensitive, so PuTTY upper-cases them,
      `telnet.c` L391-394). *Client must:* answer every SEND, repeat the
      last name at the end of its list, and then wrap round.
12. **Synch (urgent data plus DM).** When telnetd receives AO it replies
    with `IAC DM` as TCP urgent data (NetBSD `state.c` L177-196; RFC 1123
    §3.2.4 makes this MUST). PuTTY discards data after the urgent
    notification until DM, and also leaves synch on a bare 0xF2 data byte,
    because Winsock delivers OOB out of position (`telnet.c` L525-536).
    - *RFC:* compliant with RFC 854 "Synch" and RFC 1123 §3.2.4 (MUST
      discard non-command data until DM). *Client must:* take the urgent
      mark from the transport. Over a generic tokio stream no OOB signal
      exists, so treat `IAC DM` as a no-op and record the gap.
13. **`DO TM` answered as a one-shot.** telnetd answers every `DO TM` with
    `WILL TM` and keeps its state as "off", so that a later `DO TM` gets a
    new answer (NetBSD `state.c` L890-898, L417-424).
    - *RFC:* compliant with RFC 860 (TM is a mark, not a persistent mode).
      *Client must:* not apply RFC 854's "already in that mode, don't
      acknowledge" rule to TM; each `DO TM` needs its own reply.
14. **`DO LOGOUT` ends the session.** NetBSD telnetd answers `WILL LOGOUT`,
    flushes and exits (`state.c` L900-913).
    - *RFC:* compliant with RFC 727.
15. **Unknown commands and options.** Every source refuses unknown
    `WILL`/`DO` with `DONT`/`WONT` and ignores unknown two-byte commands
    (PuTTY `telnet.c` L347-354, L557-562; gensio `telnet.c` L74-107).
    - *RFC:* compliant with RFC 1123 §3.2.1 and §3.2.3.
16. **Option negotiation mixed into the data stream after a prompt.**
    Servers negotiate in two phases, and phase 2 comes after the login
    process has started. Cisco is also reported to interleave. Negotiations
    can therefore arrive after text or a prompt.
    - *RFC:* compliant (RFC 1123 §3.2.1: option negotiation SHOULD work for
      the life of the connection). *Client / expect layer must:* strip
      commands anywhere in the stream and never assume negotiation is
      "finished" before the first prompt.

## Implications for justelnet 0.1 (input to the spec, not decisions)

- To reach a login prompt on any Unix telnetd, the minimum client
  capabilities are:
  - an answer to every option;
  - TTYPE with SEND/IS and cycling;
  - NAWS;
  - SGA and ECHO answered as a character-mode client;
  - NEW-ENVIRON, or at least a refusal of it;
  - no acceptance of both ENVIRONs;
  - TM answered.
- The core should implement the RFC 1143 Q method. It alone prevents loops
  against peers that send redundant acknowledgements (gensio, telnetd's
  fake `WILL ECHO`).
- End-of-line policy (CR LF / CR NUL / bare CR, binary-aware) should be a
  caller setting. This feeds the map's open "Line-ending handling" item.

## Unconfirmed / open

- **No wire capture of any target.** Every "server sends" row for Cisco
  IOS, Cisco reverse telnet, Juniper and Opengear/commercial console
  servers is from documentation, a forum title, or an assumption, and
  **needs confirming against the real thing**. The Unix and gensio rows are
  from source and still deserve one probe each, especially quirk 5 (the
  ENVIRON hang).
- **FreeBSD telnetd and Junos:** not read (FreeBSD path gone from `main`,
  Junos closed source).
- **PuTTY version (resolved):** line numbers cite the `github/putty`
  mirror, whose last commit is 2020-09-13. The official tartarus git
  returned 403 to scripted fetches, so the 0.85 release tarball
  (`the.earth.li/~sgtatham/putty/latest/putty.tar.gz`) was diffed against
  it. 0.85 changes only SB-buffer construction (strbuf), log strings and
  socket/interactor plumbing. The option table, `proc_rec_opt`,
  `do_telnet_read`, the send path and the initial-negotiation code are
  unchanged, so every PuTTY behaviour cited here holds for 0.85.
- **Cisco `telnet speed` citing RFC 1080 (remote flow control) instead of
  RFC 1079 (TSPEED):** quoted from a summarized fetch of the IOS 11 docs;
  could be the doc's own error.
- **The NetBSD, FreeBSD and Junos telnetd relationship** is assumed from
  common 4.4BSD lineage, not verified.
