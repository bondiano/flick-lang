use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    if let Some("--version" | "-V") = env::args().nth(1).as_deref() {
        println!(
            "flick {} (Flick {})",
            env!("CARGO_PKG_VERSION"),
            flick_core::LANGUAGE_VERSION
        );
        ExitCode::SUCCESS
    } else {
        eprintln!("usage: flick --version");
        ExitCode::from(2)
    }
}
