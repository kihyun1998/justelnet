# Receive parsing

How the Core turns received bytes into Events.

## What it deliberately leaves out today

Only `IAC IAC` is understood: it becomes a single 255 data byte. For any other byte after IAC, only that one byte is dropped. So three-byte negotiations (`IAC DO x`, `IAC WILL x`, …) leak their option byte `x` into Data, and a subnegotiation's body (`IAC SB … IAC SE`) leaks into Data too.

On a real connection this puts junk bytes into Data, because every Unix telnetd opens with `IAC DO x` requests. The gap closes when negotiation (#15), subnegotiation (#17) and commands (#19) take over the bytes after IAC. Until then, `Event::Data` promises only what its doc comment says.

The maintainer accepted this leak for the skeleton on 2026-09-28, after being shown `ff fd 01 41 ff fa 18 01 ff f0 42` coming out as Data `01 41 18 01 42`. The alternative on the table was to have the skeleton skip option bytes and subnegotiation bodies itself, taking on part of the parser #15/#17 own. It is theirs to reverse.
