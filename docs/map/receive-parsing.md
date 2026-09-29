# Receive parsing

How the Core turns received bytes into Events.

## What comes out of the bytes after IAC

`IAC IAC` becomes a single 255 data byte, `IAC WILL/WONT/DO/DONT x` goes to option negotiation (see [option-negotiation.md](option-negotiation.md)), and `IAC SB … IAC SE` to subnegotiation (see [subnegotiation.md](subnegotiation.md)). The nine commands NOP, DM, BRK, IP, AO, AYT, EC, EL and GA each become a Command Event in place among the data. Any other byte after IAC (SE outside a subnegotiation, or anything below 240, EOR, ABORT, SUSP and EOF included, whose options the Core refuses) is dropped with an `UnknownCommand` Warning. No Telnet control byte reaches Data.

Warning on those bytes is the maintainer's call, made on 2026-09-29 over dropping them silently or warning on a stray SE only; it is theirs to reverse.

## DM is reported, and no data is discarded

RFC 854's Synch has the receiver discard data up to the DM once TCP urgent data signals it, and RFC 1123 §3.2.4 makes that a MUST. The Core cannot know where discarding should start: it sees a byte stream, and a generic stream carries no urgent signal. So `IAC DM` is a Command Event like the others and the data around it is kept (#6). PuTTY discards while `in_synch` is set and stops at the next 0xF2 byte, a heuristic its own comment calls hoping for the best.

The urgent mark also damages the bytes before they reach the Core. A netkit telnetd on RHEL 9 sends its Synch as TCP urgent data around the login banner; read on Windows through a socket without `SO_OOBINLINE` (probed 2026-09-29), the IAC was taken out of the stream and a lone `f2` arrived as data, in one of two runs. On BSD-semantics stacks the DM byte is the one taken, leaving a bare IAC to swallow the next byte (not observed). The Core cannot repair this from the bytes it is given; the socket has to keep urgent data inline, which `Client::connect` sets (see [driving a stream](driving-a-stream.md)).

## CR NUL

While the peer's BINARY is off, a NUL right after a data CR is dropped, so CR NUL arrives as CR (RFC 854). CR LF and a bare LF pass through as data. The CR is not held back: it goes out at once, and a NUL at the start of the next `receive` is still dropped. Anything between the two, a command included, keeps the NUL, as in PuTTY (`SEENCR` lasts one byte).

## Data and other Events keep their order

Received data is held until the Core queues another Event or the `receive` call ends, then queued as one Data Event ahead of it. Data before a negotiation therefore comes out before its OptionChanged, and data after it comes out after.
