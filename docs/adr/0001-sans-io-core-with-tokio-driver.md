# The protocol core is sans-IO; tokio lives only in the Driver

Unix telnetd blocks login until every option request and subnegotiation request is answered, so negotiation correctness is what this library lives or dies by. We split a Core that takes bytes in and hands Events and outgoing bytes out (pulled by the caller, as in quinn-proto, rather than pushed through callbacks as in libtelnet) from a tokio Driver that owns the stream and all timeouts. The Core answers negotiation and subnegotiation (TTYPE SEND, NEW-ENVIRON SEND) itself from an Option policy, so a caller cannot stall a login by forgetting to reply, and it has no clock, so recorded server byte sequences can be replayed against it deterministically.

## Considered Options

- **Core written directly against tokio streams (russh's shape).** Less code at first, but tests need fake streams, and a sync API, a server role or TLS would each mean splitting the core later. russh's Handler is also awaited inside the read loop, which stops reading while a callback runs.

## Consequences

- expect-style automation needs timeouts, so it sits at or above the Driver, never in the Core.
- The out-of-scope items (sync API, server role, TLS) stay attachable without touching the Core.
