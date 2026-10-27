//! LSP-only stderr logging; stdout belongs to JSON-RPC.

use std::backtrace::Backtrace;
use std::io;
use std::panic;

use tracing::{Level, error};
use tracing_subscriber::EnvFilter;

pub(super) fn init() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(io::stderr)
        .with_ansi(false)
        .init();

    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        if tracing::enabled!(Level::ERROR) {
            error!(panic = %info, backtrace = %Backtrace::force_capture(), "language server panicked");
        } else {
            previous(info);
        }
    }));
}
