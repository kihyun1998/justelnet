//! Replays every Transcript through the Core's public API: once with each step's
//! server bytes as one chunk, and once one byte at a time.

mod common;

use justelnet_core::Core;

#[test]
fn every_transcript_replays() {
    for t in common::load_all() {
        let mut core = Core::new();
        for (i, step) in t.steps.iter().enumerate() {
            let got = common::run_step(&mut core, [step.server.as_slice()]);
            assert_eq!(got, common::expected(step), "{} step {}", t.name, i + 1);
        }
    }
}

#[test]
fn every_transcript_replays_one_byte_at_a_time() {
    for t in common::load_all() {
        let mut core = Core::new();
        for (i, step) in t.steps.iter().enumerate() {
            let got = common::run_step(&mut core, step.server.chunks(1));
            assert_eq!(got, common::expected(step), "{} step {}", t.name, i + 1);
        }
    }
}
