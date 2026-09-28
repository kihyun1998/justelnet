# thegraph

## What this project is

An open-source Telnet library in Rust.

## References

| Source | Informs | Reached by | Binding |
|---|---|---|---|
| Telnet RFCs (854, 855, and each option's own RFC, e.g. 1073 NAWS, 1091 TTYPE) | how it works | the RFC text, raw | binding |
| libtelnet (seanmiddleditch/libtelnet) | how it works: API shape (no socket; bytes in, events out) | its source tree, raw | example |
| PuTTY `telnet.c` | how it works: option negotiation against real servers | its source tree, raw | example |
