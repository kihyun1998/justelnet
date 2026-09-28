# justelnet

A Rust Telnet client library: a protocol core that follows the Telnet RFCs, with a tokio client and expect-style automation on top.

## Language

**Core**:
The part of justelnet that holds a connection's Telnet protocol state, turning received bytes into Events and outgoing bytes. It performs no I/O and has no notion of time.
_Avoid_: engine, parser, state machine

**Driver**:
The part that binds a Core to a real async stream and owns reading, writing and every timeout.
_Avoid_: runtime, transport, connection wrapper

**Event**:
Something the Core reports to its caller, taken out by the caller rather than pushed to it.
_Avoid_: callback, message

**Option policy**:
The options the client is willing to enable on each side, plus the values it answers subnegotiation requests with (terminal types, window size, environment). The Core answers the peer from it without asking the caller.
_Avoid_: option table, telopts

**Passthrough option**:
An option the Core has no built-in handling for but negotiates because the caller put it in the Option policy; its subnegotiations reach the caller unparsed.
_Avoid_: custom option, raw option

**Terminal client**:
The first application built on justelnet; it renders the data and supplies the Option policy's values.
_Avoid_: host app, frontend
