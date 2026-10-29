//! What a module's exports carry beyond their JavaScript: the TypeScript
//! types `--emit ts` annotates them with, and which are Sync functions the
//! runtime wraps for TypeScript callers.

use std::collections::HashMap;

use crate::codegen::exports::sync_stages;
use crate::codegen::names;
#[cfg(feature = "typescript")]
use crate::codegen::ts::Projection;
use crate::iface::{Interface, Namespace};

/// The exported values of `module` whose last stage is Sync.
pub(super) fn sync_exports(interface: &Interface, module: &str) -> HashMap<String, usize> {
    interface
        .exports
        .iter()
        .filter(|e| e.entity.module == module && e.entity.namespace == Namespace::Value)
        .filter_map(|e| {
            Some((
                names::ident(&e.entity.name),
                sync_stages(e.scheme.as_ref()?)?,
            ))
        })
        .collect()
}

#[cfg(feature = "typescript")]
/// The TypeScript type of each exported value and constructor that has
/// one; the others stay `any`.
pub(super) fn annotations(
    projection: &Projection,
    interface: &Interface,
) -> HashMap<String, String> {
    interface
        .exports
        .iter()
        .filter(|e| {
            e.entity.module == projection.module
                && matches!(
                    e.entity.namespace,
                    Namespace::Value | Namespace::Constructor
                )
        })
        .filter_map(|e| {
            let ty = projection.value(e.scheme.as_ref()?).ok()?;
            Some((names::ident(&e.entity.name), ty))
        })
        .collect()
}
