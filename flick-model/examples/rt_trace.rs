//! `cargo run -p flick-model --example rt_trace -- TRACE…`: checks each
//! trace a runtime wrote with `FLICK_TRACE` against `λRT` and prints its
//! projection; fails on the first violation.

use std::process::ExitCode;
use std::{env, fs};

use flick_model::rt::{monitor, parse, project};

fn main() -> ExitCode {
    let mut ok = true;
    for path in env::args().skip(1) {
        let events = match fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|t| parse(&t))
        {
            Ok(events) => events,
            Err(e) => {
                eprintln!("{path}: {e}");
                ok = false;
                continue;
            }
        };
        match monitor::check(&events) {
            Ok(()) => print!("{path}:\n{}", project::project(&events)),
            Err(v) => {
                eprint!("{path}: {v}");
                ok = false;
            }
        }
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
