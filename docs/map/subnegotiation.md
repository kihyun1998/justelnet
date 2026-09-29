# Subnegotiation

How the Core takes `IAC SB <option> … IAC SE` out of the stream, what it keeps of the body, and which subnegotiations it answers.

## Which subnegotiations are answered

TTYPE SEND (the body exactly `01`) and NEW-ENVIRON SEND (`01` and a request list) are answered while the option is on on our side; NAWS is sent, never asked for. Every other subnegotiation is discarded; those of Passthrough options reach the caller from #20. A SEND while its option is off gets no answer: RFC 855 has subnegotiation follow agreement, and every telnetd read (target research #2) sends SEND only after the client's WILL.

## Recovering from `IAC x` inside a subnegotiation

The three references disagree when a byte other than SE or IAC follows IAC inside a subnegotiation:

- netkit telnetd (`state.c`, `TS_SE`) and libtelnet (`TELNET_STATE_SB_DATA_IAC`) end the subnegotiation there and process `IAC x` as a command, for every x.
- PuTTY (`telnet.c`, `SUBNEG_IAC`) drops the 255 and keeps x as subnegotiation data.

The Core splits on x. A command byte (241–254: NOP through DONT, SB included) is taken as a missing SE, the telnetd reading: the subnegotiation so far is handled, then the command. Any other byte is taken as a 255 the sender forgot to double, and both 255 and x are kept. A Warning (`MalformedSubnegotiation`) is emitted either way. The split matches the two likely sender bugs: a forgotten `IAC SE` before the next command, and an undoubled 255 in the data.

The decision behind this (#6) calls keeping x "PuTTY's reading" and describes it as an undoubled 255; PuTTY's code in fact drops the 255. Keeping both is the maintainer's call, made on 2026-09-29 when shown that difference, over PuTTY's actual behaviour; it is theirs to reverse. `subnegotiation-malformed.txt` tells all three readings apart.

`IAC SB` followed directly by `IAC SE` takes the 255 as the option code and SE as the first body byte, so everything up to the next `IAC SE` or `IAC <command>` is discarded as that subnegotiation, with no Warning. PuTTY and libtelnet read it the same way; netkit reads an empty subnegotiation.

## The size limit

The Core keeps at most 4096 bytes of a subnegotiation's body. Past that, bytes are discarded until `IAC SE` (IAC IAC still counts as one byte), the subnegotiation is handled as truncated, and one `SubnegotiationTruncated` Warning is emitted at its end. The rest of the body never reaches Data.

netkit telnetd truncates the same way at 512 bytes. libtelnet grows to 16384 and then abandons the subnegotiation, which sends the rest of the body to the application as data. The 4096 is not measured against anything: it is above every subnegotiation the Core answers or the target research saw (TTYPE and NEW-ENVIRON SENDs are a few bytes) and bounded, and a Passthrough option (#20) with larger subnegotiations is the case that would move it.

## TTYPE

Every SEND is answered with IS and the next name in RFC 1091's cycle: each name of the policy's list in turn, the last one again to mark the end, then from the top. A one-name list answers the same name every time. An empty list answers `UNKNOWN`. Names are sent as given; RFC 1091's 40-character limit and the Assigned Numbers' upper case are the caller's to meet.

The position in the cycle is kept for the whole connection: TTYPE turning off and on again does not send the list from the top. RFC 1091 does not say; BSD telnet's behaviour here was not read. The maintainer chose to leave it on 2026-09-29, since no server was seen to cycle at all; it is theirs to reverse.

The cycle has not been seen on a real server. A netkit telnetd on RHEL 9 (probed 2026-09-29) sent one SEND and took the answer, even an unknown name (`FOO`), where the target research read netkit's source as asking again until it recognises a type. `ttype-cycling.txt` is written from RFC 1091, not captured.

The default list is `["UNKNOWN"]`, RFC 1091's name for a type the sender does not know, since the Core does not know the caller's emulator. A terminal client sets its own list. The maintainer chose this on 2026-09-29 over `["XTERM"]` (PuTTY's family, working full-screen programs without configuration but wrong for an emulator that is not an xterm); it is theirs to reverse.

RFC 1091 has the client's emulation follow the type it sent last, while the TTYPE subnegotiation is kept out of the Event stream. `Core::terminal_type_sent` reports it on demand instead, like `is_enabled`. The maintainer chose a query on 2026-09-29 over leaving it out until a terminal client asked.

## NAWS

The window size (RFC 1073: width, then height, 16 bits each, big-endian) is sent right after NAWS turns on on our side, after the WILL when the Core answers the peer's DO, and again whenever `set_window_size` changes it while NAWS is on. A size set while NAWS is off is kept and sent when it turns on; setting the size already held sends nothing. A 255 in the size is sent as IAC IAC. The policy's size is the starting one, 80x24 by default (RFC 1073's example, PuTTY's default).

## NEW-ENVIRON

SEND is answered with IS as RFC 1572 lays it out, not as PuTTY does (PuTTY ignores the request list and sends every variable as VAR):

- A bare SEND (no list at all) gets every well-known variable (VAR), then every user variable (USERVAR), in the policy's order. Unix telnetd (netkit, inetutils, NetBSD) sends only bare SENDs. A list holding no VAR or USERVAR asks for nothing and gets an empty IS.
- A request list is answered entry by entry, in its order: a type with a name gets that variable, or the name alone when the policy lacks it (RFC 1572's "undefined"); a type with no name gets every variable of that type. An entry is answered as often as it is asked for, as in RFC 1572's own example.
- VAR, VALUE, ESC and USERVAR bytes inside a name or value are escaped with ESC, both ways; 255 is doubled at the subnegotiation layer.

The policy's variables are set with `variable` (VAR) and `user_variable` (USERVAR) rather than sorted by RFC 1572's list of well-known names, so a caller decides the type. The maintainer chose the two methods on 2026-09-29 over sorting by name; it is theirs to reverse. There are no variables by default: the Core has no view of the process environment, and USER in particular is what some servers use to pick the account to log in.

INFO (unsolicited changes after the first IS) is not sent.

What the server does with the variables is its own: netkit telnetd (`state.c`, the NEW-ENVIRON IS parser) skips bytes before the first type, unsets a variable sent without VALUE, honours USERVAR only when built with `ACCEPT_USERVAR`, and passes VAR names through `envvarok`, so a caller's variable can be dropped there without any sign on the wire.
