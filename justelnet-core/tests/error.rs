//! The Core's Error type reads as an error and says what was misused.

use justelnet_core::{Error, TelnetOption};

#[test]
fn option_not_enabled_names_the_option() {
    let error = Error::OptionNotEnabled {
        option: TelnetOption::COM_PORT,
    };
    let as_std: &dyn std::error::Error = &error;
    assert_eq!(
        as_std.to_string(),
        "option 44 is not enabled on either side"
    );
}

#[test]
fn not_passthrough_names_the_option() {
    let error = Error::NotPassthrough {
        option: TelnetOption::NAWS,
    };
    assert_eq!(error.to_string(), "option 31 is not a Passthrough option");
}
