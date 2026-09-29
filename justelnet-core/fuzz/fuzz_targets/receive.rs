//! Feeds arbitrary bytes, split at arbitrary points, into a Core on the
//! default Option policy and drains everything it produces.
//!
//! Input layout: the first byte is the number of chunk lengths that follow
//! (up to 15); each of those bytes is one chunk's length; the rest is the
//! stream. The stream left after the chunks is fed as one last chunk.

#![no_main]

use justelnet_core::{Core, Event};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    let Some((&count, rest)) = input.split_first() else {
        return;
    };
    let count = usize::from(count % 16).min(rest.len());
    let (lengths, mut stream) = rest.split_at(count);

    let mut core = Core::new();
    let mut out = Vec::new();
    for &length in lengths {
        let (chunk, tail) = stream.split_at(usize::from(length).min(stream.len()));
        feed(&mut core, chunk, &mut out);
        stream = tail;
    }
    feed(&mut core, stream, &mut out);
});

fn feed(core: &mut Core, chunk: &[u8], out: &mut Vec<u8>) {
    core.receive(chunk);
    while let Some(event) = core.poll_event() {
        assert!(
            event != Event::Data(Vec::new()),
            "the Core emitted an empty Data Event"
        );
    }
    core.poll_transmit(out);
    out.clear();
}
