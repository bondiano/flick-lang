//! Component packaging grants no WASI adapters or ambient host authority.

use std::{env, error::Error, fs, process::ExitCode};
use wit_component::ComponentEncoder;

fn build() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: flick-component-build INPUT.wasm OUTPUT.wasm".into());
    }
    let module = fs::read(&args[0])?;
    let component = ComponentEncoder::default()
        .module(&module)?
        .validate(true)
        .encode()?;
    fs::write(&args[1], component)?;
    Ok(())
}

fn main() -> ExitCode {
    match build() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
