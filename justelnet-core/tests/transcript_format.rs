//! A replay's Outcome prints as Transcript lines, which parse back to it.

mod common;

use common::Outcome;
use justelnet_core::{Command, Event, Side, TelnetOption, Warning};

fn round_trip(outcome: &Outcome, at: &str) {
    let text = format!("{outcome:?}");
    assert_eq!(&common::parse_outcome(&text), outcome, "{at}:\n{text}");
}

#[test]
fn every_expected_outcome_round_trips() {
    for t in common::load_all() {
        for (i, step) in t.steps.iter().enumerate() {
            round_trip(
                &common::expected(step),
                &format!("{} step {}", t.name, i + 1),
            );
        }
    }
}

#[test]
fn every_byte_option_and_command_round_trips() {
    let mut events = vec![Event::Data((0..=u8::MAX).collect())];
    for code in 0..=u8::MAX {
        let option = TelnetOption::new(code);
        events.push(Event::OptionChanged {
            option,
            side: Side::Remote,
            enabled: code % 2 == 0,
        });
        events.push(Event::Subnegotiation {
            option,
            data: vec![code, 0xff],
        });
        events.push(Event::Warning(Warning::MalformedSubnegotiation {
            option,
            byte: code,
        }));
        events.push(Event::Warning(Warning::SubnegotiationTruncated { option }));
        events.push(Event::Warning(Warning::NoncompliantAnswer {
            option,
            side: Side::Local,
        }));
        events.push(Event::Warning(Warning::UnknownCommand { byte: code }));
        events.push(Event::Data(b"\"\\\r\n".to_vec()));
    }
    for command in [
        Command::NoOperation,
        Command::DataMark,
        Command::Break,
        Command::InterruptProcess,
        Command::AbortOutput,
        Command::AreYouThere,
        Command::EraseCharacter,
        Command::EraseLine,
        Command::GoAhead,
    ] {
        events.push(Event::Command(command));
    }
    let outcome = Outcome {
        client: (0..=u8::MAX).collect(),
        events,
    };
    round_trip(&outcome, "every byte, option and command");
}

#[test]
fn nothing_round_trips() {
    round_trip(
        &Outcome {
            client: Vec::new(),
            events: Vec::new(),
        },
        "an empty Outcome",
    );
}
