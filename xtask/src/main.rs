//! `cargo xtask <gate>` — the build-pipeline gates that are not a `cargo` subcommand.
//!
//! Today there is one: `lint-references`, build-pipeline step 9. It replaced a bash
//! script and a PowerShell script implementing the same fourteen checks, ~3,400 lines
//! kept in step by hand — an arrangement that produced seven defects in a single QA
//! round, every one of them a divergence between the two ports rather than a bug in the
//! rule. The rule now has one implementation, so there is nothing to keep in step, and
//! the corpora that prove each pattern discriminates are ordinary `#[test]` cases run by
//! build-pipeline step 4 rather than a hand-rolled `--self-test` runner.
//!
//! Adding a gate here is the sanctioned route for any new mechanical check. Do NOT add a
//! second script in a shell: the interpreter argument is settled — `cargo` is the only one
//! on PATH on all three platforms without a locate-it step.

mod lint;

use std::process::ExitCode;

const USAGE: &str = "usage: cargo xtask lint-references";

fn main() -> ExitCode {
    // `skip(1)` drops the binary path; the alias in `.cargo/config.toml` passes everything
    // after `--` straight through, so argv[1] is the gate name.
    let args: Vec<String> = std::env::args().skip(1).collect();
    let gate = args.first().map(String::as_str);

    match gate {
        Some("lint-references") if args.len() == 1 => match lint::run() {
            Ok(true) => ExitCode::SUCCESS,
            Ok(false) => ExitCode::FAILURE,
            // A gate that cannot RUN is worse than one that is absent, because its red
            // reads as a verdict. Say which it is, on stderr, and exit non-zero anyway.
            Err(why) => {
                eprintln!("lint-references: {why}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}
