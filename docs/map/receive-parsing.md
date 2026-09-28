# Receive parsing

How the Core turns received bytes into Events.

## What it deliberately leaves out today

`IAC IAC` becomes a single 255 data byte, and `IAC WILL/WONT/DO/DONT x` is taken out of the data and handed to option negotiation (see [option-negotiation.md](option-negotiation.md)). For any other byte after IAC, only that one byte is dropped. So a subnegotiation's body (`IAC SB … IAC SE`) still leaks into Data, and so do the bytes of any command other than the four negotiation verbs.

On a real connection this puts junk bytes into Data whenever the server subnegotiates, which every Unix telnetd does (TTYPE, NEW-ENVIRON `SEND`). The gap closes when subnegotiation (#17) and commands (#19) take over the rest of the bytes after IAC. Until then, `Event::Data` promises only what its doc comment says.

The maintainer accepted this leak for the skeleton on 2026-09-28, after being shown `ff fd 01 41 ff fa 18 01 ff f0 42` coming out as Data `01 41 18 01 42`. The alternative on the table was to have the skeleton skip option bytes and subnegotiation bodies itself, taking on part of the parser #15/#17 own. It is theirs to reverse. #15 has since closed the option-byte half; the subnegotiation half stands.

## Data and other Events keep their order

Received data is held until the Core queues another Event or the `receive` call ends, then queued as one Data Event ahead of it. Data before a negotiation therefore comes out before its OptionChanged, and data after it comes out after.
