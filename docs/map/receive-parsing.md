# Receive parsing

How the Core turns received bytes into Events.

## What it deliberately leaves out today

`IAC IAC` becomes a single 255 data byte, `IAC WILL/WONT/DO/DONT x` goes to option negotiation (see [option-negotiation.md](option-negotiation.md)), and `IAC SB … IAC SE` to subnegotiation (see [subnegotiation.md](subnegotiation.md)). Any other command (`IAC x`, x one of NOP, DM, BRK, IP, AO, AYT, EC, EL, GA) is dropped whole and reported nowhere until commands (#19) turn them into Events. No Telnet control byte reaches Data any more.

The skeleton leaked option bytes and subnegotiation bodies into Data; the maintainer accepted that on 2026-09-28 for the time between tickets, after being shown `ff fd 01 41 ff fa 18 01 ff f0 42` coming out as Data `01 41 18 01 42`. #15 and #17 have closed both halves; the same bytes now give Data `41 42`.

## Data and other Events keep their order

Received data is held until the Core queues another Event or the `receive` call ends, then queued as one Data Event ahead of it. Data before a negotiation therefore comes out before its OptionChanged, and data after it comes out after.
