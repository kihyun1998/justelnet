//! Chunking is invisible: splitting any Transcript's server bytes at arbitrary
//! points yields the same outgoing bytes and Events as the Transcript expects.

mod common;

use justelnet_core::Core;
use proptest::prelude::*;

proptest! {
    #[test]
    fn every_transcript_survives_arbitrary_chunking(
        cuts in proptest::collection::vec(proptest::collection::vec(any::<usize>(), 0..12), 64)
    ) {
        for t in common::load_all() {
            let mut core = Core::new();
            for (i, step) in t.steps.iter().enumerate() {
                let chunks = common::split(&step.server, &cuts[i % cuts.len()]);
                let got = common::run_step(&mut core, chunks);
                prop_assert_eq!(got, common::expected(step), "{} step {}", t.name, i + 1);
            }
        }
    }
}
