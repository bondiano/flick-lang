//! The plugins linked into the compiler: those of the packages that come
//! with it, which every build has, the browser's among them.

use std::sync::Arc;

use flick_plugin_cli::Cli;

use super::{Expander, Native};

/// The heads of the plugins linked into the compiler and the module that
/// exposes each. One is a block even where no import exposes it, so the
/// compiler can say which import it needs: `cli "x" { … }` stays the block
/// it was before `Cli` gave it.
pub const HEADS: [(&str, &str); 1] = [("cli", "Cli")];

/// The plugin linked into the compiler for `package`.
pub fn plugin(package: &str) -> Option<Arc<dyn Expander>> {
    match package {
        "std" => Some(Arc::new(Native::new("std", Cli))),
        _ => None,
    }
}
