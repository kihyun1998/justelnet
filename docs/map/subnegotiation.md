# Subnegotiation

How the Core takes `IAC SB <option> … IAC SE` out of the stream, what it keeps of the body, and which subnegotiations it answers.

## Only TTYPE SEND is answered today

A complete subnegotiation is discarded unless it is TTYPE SEND (the body exactly `01`) while TTYPE is on on our side. NEW-ENVIRON SEND is answered from #18, and subnegotiations of Passthrough options reach the caller from #20. A SEND while TTYPE is off gets no answer: RFC 855 has subnegotiation follow agreement, and every telnetd read (target research #2) sends SEND only after the client's WILL.

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
