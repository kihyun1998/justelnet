# Every Event has a Transcript spelling

## The fact

Every public value a Transcript can name — each `Event` and `Warning` variant, each `Command`, each named `TelnetOption` constant, each `Error` variant, each `Side` — has one spelling that the runner prints, the runner parses, and `docs/transcripts.md` documents, and the three agree. Checkable in part by a test, in part only by reading (below).

## Why it is cross-cutting

The vocabulary is defined once in the Core and copied, by hand, into three places that do not call each other: the runner's printer, its parser, and the format page. A new variant compiles everywhere, because the Core's enums are `#[non_exhaustive]` and every printer has an `other =>` arm that falls back to `Debug` — so nothing the compiler sees connects a new Core variant to the format.

| Vocabulary | Core (`justelnet-core/src/lib.rs`) | Printer (`tests/common/mod.rs`) | Parser (`tests/common/mod.rs`) | Page (`docs/transcripts.md`) |
|---|---|---|---|---|
| Events, Warnings | `Event`, `Warning` | `event_text` | `event` | "Steps" table, `event:` rows |
| Commands | `Command`, `Command::from_code` | `command_text` | `command_named` | "Values", *A command* |
| Options | `TelnetOption::BINARY` … `COM_PORT` (14) | `option_text` | `option_named` | "Values", *An option* |
| Errors | `Error` | — (`error:` is expected only) | `parse`, `error:` arm | `error:` row |
| Sides | `Side` | `side_text` | `side_option`, `event` | "Values", *A side* |

What a tool can see: `cargo test -p justelnet-core --test transcript_format`. Its `every_byte_option_and_command_round_trips` builds every option code and asks the Core which `IAC <byte>` it reports as a Command (`commands_the_core_reports`), so the printer and parser are held to every option and every command the Core has. What it cannot see: the page's lists, a named `TelnetOption` constant the printer lacks (it prints as hex and round-trips), and Event, Warning or Error variants no Transcript expects.

## Territories it holds in

- [Transcript replay](transcript-replay.md) — `justelnet-core/tests/common/mod.rs — event_text, option_text, command_text, side_text, event, option_named, command_named`; `justelnet-core/tests/transcript_format.rs — every_byte_option_and_command_round_trips`; `docs/transcripts.md`.
- [Receive parsing](receive-parsing.md) — `Command`, `Warning::UnknownCommand`, `Event::Data`, `Event::Command`.
- [Option negotiation](option-negotiation.md) — `Event::OptionChanged`, `Warning::NoncompliantAnswer`, `TelnetOption`'s constants, `Side`.
- [Subnegotiation](subnegotiation.md) — `Event::Subnegotiation`, `Warning::MalformedSubnegotiation`, `Warning::SubnegotiationTruncated`, `Error`.
- [Sending](sending.md) — `Command` for `call: command`.

## What a violation looks like

A new Event or Warning variant that a Transcript expects prints as its `Debug`, which the parser rejects, so `every_expected_outcome_round_trips` goes red as soon as a Transcript expects one — good. A new Event or Warning variant no Transcript expects yet passes every test; a new `Command` the Core reports fails `every_byte_option_and_command_round_trips` until the printer and parser know it; a new named option constant prints as two-digit hex (still round-trips) and is simply missing from the page. A spelling dropped from the page fails nothing.

## Discovery history

- #63 (commit `606cd40`, 2026-10-08) made failures print as Transcript lines and added `transcript_format.rs` to hold printer to parser; the hand-written transcript-replay note recorded the `Debug` fallback as what turns a missed variant red.
- This note: the five-way copy read from the source at `9a09008` (2026-10-22).
- 2026-10-22: the hand-written command list in `transcript_format.rs` replaced by `commands_the_core_reports`. Checked by mutation: a Core given an `EndOfRecord` command (239) with the printer unchanged failed the new test (`unknown command`) and passed the old list.

## Where it will recur

If a change adds or renames a variant of `Event`, `Warning`, `Command`, `Error` or `Side`, or a named `TelnetOption` constant, it is subject to this: add its spelling to the printer, the parser and `docs/transcripts.md`, and write a Transcript that expects it. A command needs no list of its own; the round-trip test finds it.
