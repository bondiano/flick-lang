//! What a module's exports carry beyond their JavaScript: which are Sync
//! functions the runtime wraps for TypeScript callers.

use std::collections::HashMap;

use crate::codegen::names;
use flick_ir::Exports;

/// The exported values of a module whose last stage is Sync, with the
/// arrows a call supplies.
pub(super) fn sync_exports(exports: &Exports) -> HashMap<String, usize> {
    (exports.values.iter())
        .filter_map(|(name, sync)| Some((names::ident(name), (*sync)?)))
        .collect()
}
