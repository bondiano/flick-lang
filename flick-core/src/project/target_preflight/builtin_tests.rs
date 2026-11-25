//! The summary of the built-in modules reads as their IR does: each item's
//! uses in order, its foreign name and whether it needs the Effect adapter.

use super::super::gated::needs_adapter;
use super::super::refs::{Reference, collect_item_references};
use super::BuiltinPreflight;
use crate::ir::{ItemKind, reach};
use crate::project::builtins::builtin_modules_of;
use crate::source::{FileId, Span};

fn shown(refs: &[Reference]) -> Vec<String> {
    (refs.iter())
        .map(|r| match r {
            Reference::Value { key, span, .. } => format!("value {}.{} at {span:?}", key.0, key.1),
            Reference::ModuleOnly {
                module,
                symbol,
                span,
            } => {
                format!("module {module} {symbol} at {span:?}")
            }
        })
        .collect()
}

#[test]
fn the_summary_reads_each_built_in_item_as_its_ir() {
    let origin = Some(Span::new(FileId::new(7), 3, 9));
    for by_class in [false, true] {
        let modules = builtin_modules_of(by_class);
        let summary = BuiltinPreflight::of(modules);
        let mut items = 0;
        for module in modules {
            for item in &module.items {
                let Some(name) = reach::defined(item).into_iter().next() else {
                    continue;
                };
                let at = summary
                    .find(&(module.name.clone(), name.clone()))
                    .unwrap_or_else(|| panic!("{}.{name} is summarized", module.name));
                let mut refs = Vec::new();
                collect_item_references(item, false, origin, 0, &mut refs);
                assert_eq!(
                    shown(&summary.uses(at, origin, 0)),
                    shown(&refs),
                    "{}.{name}",
                    module.name
                );
                if let ItemKind::Foreign(f) = &item.kind {
                    let shown = f.source_name.as_deref().unwrap_or(&f.name);
                    assert_eq!(summary.foreign_name(at), Some(shown));
                    for ui in [None, Some("react")] {
                        assert_eq!(
                            summary.needs_adapter(at, ui),
                            needs_adapter(f, ui),
                            "{shown}"
                        );
                    }
                } else {
                    assert_eq!(summary.foreign_name(at), None);
                }
                items += 1;
            }
        }
        assert!(items > 1000, "{items} built-in items");
    }
}

#[test]
fn the_summary_survives_a_round_trip() {
    let summary = BuiltinPreflight::of(builtin_modules_of(false));
    let json = serde_json::to_vec(&summary).expect("a summary serializes");
    let back: BuiltinPreflight = serde_json::from_slice(&json).expect("and reads back");
    assert_eq!(serde_json::to_vec(&back).expect("again"), json);
}
