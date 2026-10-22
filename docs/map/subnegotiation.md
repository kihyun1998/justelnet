# Subnegotiation

## What it is

How the Core takes `IAC SB <option> … IAC SE` out of the stream, what it keeps of the body, which subnegotiations it answers itself (TTYPE SEND, NEW-ENVIRON SEND), which it sends unasked (NAWS), and how a Passthrough option's subnegotiations go to and come from the caller instead.

## Governing decisions

- [ADR 0001](../adr/0001-sans-io-core-with-tokio-driver.md): the Core answers TTYPE SEND and NEW-ENVIRON SEND itself from the Option policy, so a caller cannot stall a Unix telnetd login.

The recovery rule (#6), the Passthrough rules (#10, #19) and the TTYPE/NEW-ENVIRON details (#17, #18) are issue decisions, with the maintainer's calls dated below.

## Design model

From the source, the issues and the RFCs.

### Which subnegotiations are answered

TTYPE SEND (the body exactly `01`) and NEW-ENVIRON SEND (`01` and a request list) are answered while the option is on on our side (`Core::subnegotiated`); NAWS is sent, never asked for. A Passthrough option's subnegotiations go to the caller instead (below); every other subnegotiation, OLD-ENVIRON's included, is discarded. A SEND while its option is off gets no answer: RFC 855 has subnegotiation follow agreement, and every telnetd read (target research #2) sends SEND only after the client's WILL.

### Recovering from `IAC x` inside a subnegotiation

The three references disagree when a byte other than SE or IAC follows IAC inside a subnegotiation:

- netkit telnetd (`state.c`, `TS_SE`) and libtelnet (`TELNET_STATE_SB_DATA_IAC`) end the subnegotiation there and process `IAC x` as a command, for every x.
- PuTTY (`telnet.c`, `SUBNEG_IAC`) drops the 255 and keeps x as subnegotiation data.

The Core splits on x (`Core::receive`, the `Parse::SbIac` arms). A command byte (241–254: NOP through DONT, SB included) is taken as a missing SE, the telnetd reading: the subnegotiation so far is handled, then the command. Any other byte is taken as a 255 the sender forgot to double, and both 255 and x are kept. A Warning (`MalformedSubnegotiation`) is emitted either way. The split matches the two likely sender bugs: a forgotten `IAC SE` before the next command, and an undoubled 255 in the data.

The decision behind this (#6) calls keeping x "PuTTY's reading" and describes it as an undoubled 255; PuTTY's code in fact drops the 255. Keeping both is the maintainer's call, made on 2026-09-29 when shown that difference, over PuTTY's actual behaviour; it is theirs to reverse. `justelnet-core/tests/transcripts/subnegotiation-malformed.txt` tells all three readings apart.

`IAC SB` followed directly by `IAC SE` takes the 255 as the option code and SE as the first body byte, so everything up to the next `IAC SE` or `IAC <command>` is discarded as that subnegotiation. PuTTY and libtelnet read it the same way; netkit reads an empty subnegotiation. Ended by `IAC SE`, it raises no Warning of its own (an undoubled 255 inside it, or a body past the size limit, warns as anywhere else). Ended by `IAC <command>`, it is a subnegotiation missing its SE like any other: a `MalformedSubnegotiation` Warning naming option 255, then the command. That Warning is the maintainer's call, made on 2026-10-22 over making option 255 a silent exception (#85); it is theirs to reverse. `subnegotiation-malformed.txt` replays both endings.

### The size limit

The Core keeps at most `SUBNEGOTIATION_LIMIT` (4096) bytes of a subnegotiation's body. Past that, bytes are discarded until `IAC SE` (IAC IAC still counts as one byte), the subnegotiation is handled as truncated, and one `SubnegotiationTruncated` Warning is emitted at its end. The rest of the body never reaches Data. This is the only bound on a peer-supplied length in the Core.

netkit telnetd truncates the same way at 512 bytes. libtelnet grows to 16384 and then abandons the subnegotiation, which sends the rest of the body to the application as data. The 4096 is not measured against anything: it is above every subnegotiation the Core answers or the target research saw (TTYPE and NEW-ENVIRON SENDs are a few bytes) and bounded, and a Passthrough option with larger subnegotiations is the case that would move it.

### TTYPE

Every SEND is answered with IS and the next name in RFC 1091's cycle (`Core::answer_ttype`): each name of the policy's list in turn, the last one again to mark the end, then from the top. A one-name list answers the same name every time. An empty list answers `UNKNOWN`. Names are sent as given; RFC 1091's 40-character limit and the Assigned Numbers' upper case are the caller's to meet.

The position in the cycle (`ttype_answers`) is kept for the whole connection: TTYPE turning off and on again does not send the list from the top. RFC 1091 does not say; BSD telnet's behaviour here was not read. The maintainer chose to leave it on 2026-09-29, since no server was seen to cycle at all; it is theirs to reverse.

What real servers did: a netkit telnetd on RHEL 9 (probed 2026-09-29) sent one SEND and took the answer, even an unknown name (`FOO`), where the target research read netkit's source as asking again until it recognises a type. GNU inetutils telnetd 2.8 sends a second SEND after the first answer, and takes the repeated `UNKNOWN` (`telnetd-inetutils-2.8-recorded.txt`, recorded 2026-10-07) — the one-name case of the cycle; a longer list has still not been cycled through on a real server. `ttype-cycling.txt` is written from RFC 1091, not captured.

The default list is `["UNKNOWN"]`, RFC 1091's name for a type the sender does not know, since the Core does not know the caller's emulator. A terminal client sets its own list. The maintainer chose this on 2026-09-29 over `["XTERM"]` (PuTTY's family, working full-screen programs without configuration but wrong for an emulator that is not an xterm); it is theirs to reverse.

RFC 1091 has the client's emulation follow the type it sent last, while the TTYPE subnegotiation is kept out of the Event stream. `Core::terminal_type_sent` reports it on demand instead, like `is_enabled`. The maintainer chose a query on 2026-09-29 over leaving it out until a terminal client asked.

### NAWS

The window size (RFC 1073: width, then height, 16 bits each, big-endian; `Core::send_naws`) is sent right after NAWS turns on on our side, after the WILL when the Core answers the peer's DO, and again whenever `set_window_size` changes it while NAWS is on. A size set while NAWS is off is kept and sent when it turns on; setting the size already held sends nothing. A 255 in the size is sent as IAC IAC. The policy's size is the starting one, 80x24 by default (RFC 1073's example, PuTTY's default).

### NEW-ENVIRON

SEND is answered with IS as RFC 1572 lays it out (`Core::answer_environ`), not as PuTTY does (PuTTY ignores the request list and sends every variable as VAR):

- A bare SEND (no list at all) gets every well-known variable (VAR), then every user variable (USERVAR), in the policy's order. Unix telnetd (netkit, inetutils, NetBSD) sends only bare SENDs. A list holding no VAR or USERVAR asks for nothing and gets an empty IS.
- A request list is answered entry by entry, in its order: a type with a name gets that variable, or the name alone when the policy lacks it (RFC 1572's "undefined"); a type with no name gets every variable of that type. An entry is answered as often as it is asked for, as in RFC 1572's own example.
- VAR, VALUE, ESC and USERVAR bytes inside a name or value are escaped with ESC, both ways (`push_escaped`); 255 is doubled at the subnegotiation layer (`queue_subnegotiation`).

The policy's variables are set with `variable` (VAR) and `user_variable` (USERVAR) rather than sorted by RFC 1572's list of well-known names, so a caller decides the type. The maintainer chose the two methods on 2026-09-29 over sorting by name; it is theirs to reverse. There are no variables by default: the Core has no view of the process environment, and USER in particular is what some servers use to pick the account to log in.

INFO (unsolicited changes after the first IS) is not sent.

What the server does with the variables is its own: netkit telnetd (`state.c`, the NEW-ENVIRON IS parser) skips bytes before the first type, unsets a variable sent without VALUE, honours USERVAR only when built with `ACCEPT_USERVAR`, and passes VAR names through `envvarok`, so a caller's variable can be dropped there without any sign on the wire. inetutils 2.8 ignores every environment option by default ([logging in to a real server](logging-in-to-a-real-server.md)).

### Passthrough options

A Passthrough option's subnegotiations reach the caller as `Event::Subnegotiation`, the body as received with IAC IAC unescaped, while the option is `live` on at least one side — on, or asked off by us with the peer's confirmation not yet in: an answer the peer sent before it saw our WONT/DONT still arrives, as with the peer's BINARY (#19). Once it is off on both sides they are discarded, as the Core's own are. The size limit and its Warning apply the same.

`Core::send_subnegotiation` sends one, doubling IAC. It is for Passthrough options only: for any other it returns `Error::NotPassthrough`, since the Core writes those subnegotiations itself and a caller's would interleave with them (a caller NAWS among the Core's). For an option off on both sides it returns `Error::OptionNotEnabled`. Either way nothing is sent; these are the caller misuses the Core reports (#10). Both rules are the maintainer's calls, made on 2026-09-29 over sending for any option that is on and over discarding once we have asked the option off; they are theirs to reverse.

Marking an option Passthrough says only who handles its subnegotiations; whether it is refused, accepted or asked for is set with `refuse`, `accept` and `request` like any other ([option policy](option-policy.md)). RFC 2217's COM-PORT, for one, is offered by the client (WILL). A Passthrough mark on TTYPE, NAWS or NEW-ENVIRON takes them over: the Core stops answering SEND and stops sending the window size. Both are the maintainer's calls, made on 2026-09-29 over a Passthrough mark that also accepts the option on both sides, and over a mark the built-in options ignore; they are theirs to reverse.

## Code

- `justelnet-core/src/lib.rs` — `Parse::Sb`, `Parse::SbData`, `Parse::SbIac` (in `Core::receive`), `SUBNEGOTIATION_LIMIT`, `Core::subnegotiation_byte`, `Core::subnegotiated`, `Core::answer_ttype`, `Core::terminal_type_sent`, `Core::answer_environ`, `push_escaped`, `Core::send_naws`, `Core::set_window_size`, `Core::queue_subnegotiation`, `Core::send_subnegotiation`, `Error`, `Warning::MalformedSubnegotiation`, `Warning::SubnegotiationTruncated`, `Event::Subnegotiation`, the constants `IS`, `SEND`, `VAR`, `VALUE`, `ESC`, `USERVAR`

## Reference behaviour

- RFC 855, 1091 (TTYPE), 1073 (NAWS), 1572 (NEW-ENVIRON), 2217 (COM-PORT) — binding per [thegraph](../agents/thegraph.md).
- netkit telnetd `state.c`, libtelnet, PuTTY `telnet.c`, read raw (2026-09-29).
- [Target research](../research/target-options-quirks.md): which SENDs Unix telnetd sends, OLD-ENVIRON's reversed codes, the ENVIRON-both-on hang.
- Transcripts captured from servers: `telnetd-netkit-rhel9.txt`, `telnetd-inetutils-2.8-recorded.txt`.

## Cross-cutting invariants

- [Every Event has a Transcript spelling](every-event-has-a-transcript-spelling.md) — Subnegotiation, MalformedSubnegotiation, SubnegotiationTruncated, and the two `Error`s (`error:` lines).

## Blast radius

- [Option policy](option-policy.md) — terminal types, window size, variables and the Passthrough marks are read from it; a new built-in answer needs a policy setting and a Transcript directive for it.
- [Option negotiation](option-negotiation.md) — answers depend on `is_enabled(…, Local)`, delivery on `live`; NAWS is sent from `received`.
- [Receive parsing](receive-parsing.md) — `SbIac` re-enters `Core::command`, so a change to command handling changes recovery here.
- [Sending](sending.md) — `queue_subnegotiation` doubles IAC through `send_raw`.
- [Driving a stream](driving-a-stream.md) — `Client::send_subnegotiation` and `set_window_size` restate these rules in their docs; `Error::Core` carries this territory's errors.
- [Transcript replay](transcript-replay.md) — `ttype-*`, `naws*`, `new-environ*`, `passthrough-*`, `subnegotiation-*` Transcripts pin each rule.

## Known holes / open

- An OLD-ENVIRON SEND is never answered, even when a caller's policy accepts OLD-ENVIRON (see [option policy](option-policy.md)).
- NEW-ENVIRON INFO is not sent.
- The 4096 limit is unmeasured; a Passthrough option with larger subnegotiations would move it.
- A TTYPE list longer than one name has not been cycled through by a real server.
