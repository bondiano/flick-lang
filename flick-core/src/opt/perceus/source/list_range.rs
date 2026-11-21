//! Exact checked Int ranges via O(n) `Intrinsic::ListRange`.
use super::{Code, Intrinsic, Local, Lower, LowerError, Name, Span, unsupported};
use flick_ir::{Adapter, Copies, Foreign, ForeignMode, ResultAdapter, Shape};
pub(super) fn is_binding(value: &Foreign) -> bool {
    value.binding.from.as_deref() == Some("flick:rt") && value.binding.name == "listRange"
}
fn supports(value: &Foreign) -> bool {
    is_binding(value)
        && value.mode == ForeignMode::Pure
        && value.arity == 2
        && value.dicts == 0
        && value.stages == [false, false]
        && value.catch.is_none()
        && value.adapter
            == Adapter {
                receiver: false,
                cancel: false,
                optional: vec![],
                variadic: false,
                result: ResultAdapter {
                    nullable: false,
                    unit: false,
                },
                callbacks: vec![None, None],
                copies: Copies {
                    args: vec![false, false],
                    result: true,
                    callbacks: vec![None, None],
                },
                shapes: vec![Shape::Data, Shape::Data],
                result_shape: Shape::Data,
            }
}
impl Lower<'_> {
    pub(super) fn list_range(
        &mut self,
        value: &Foreign,
        name: &Name,
        span: Span,
        block: &mut Code,
    ) -> Result<Local, LowerError> {
        if !supports(value) {
            return Err(unsupported("List.range ownership adapter", span));
        }
        let declarations = self.declarations.as_ref().ok_or_else(|| {
            unsupported("List.range requires a checked declaration context", span)
        })?;
        let source = Name::new(
            &name.module,
            value.source_name.as_deref().unwrap_or(&value.name),
        );
        declarations
            .list_range(&source)
            .map_err(|message| unsupported(format!("List.range: {message}"), span))?;
        self.intrinsic(Intrinsic::ListRange, false, vec![], block, span)
    }
}

#[cfg(test)]
mod tests {
    use super::supports;
    use crate::{
        build,
        ir::{Expr, ExprKind, Foreign, ItemKind, Lit, memory},
        opt::perceus::{LowerError, lower, lower_with_declarations},
        source::{FileId, Span},
    };
    use flick_ir::{ForeignMode, Shape};
    type Mutation = Box<dyn Fn(&mut Foreign)>;
    fn mutations() -> Vec<Mutation> {
        let mut edits: Vec<Mutation> = vec![
            Box::new(|v| v.binding.from = None),
            Box::new(|v| v.binding.from = Some("other".into())),
            Box::new(|v| v.binding.name.push('!')),
            Box::new(|v| v.mode = ForeignMode::Sync),
            Box::new(|v| v.mode = ForeignMode::Async),
            Box::new(|v| v.mode = ForeignMode::Effect),
            Box::new(|v| v.arity = 0),
            Box::new(|v| v.arity = 1),
            Box::new(|v| v.arity = 3),
            Box::new(|v| v.dicts = 1),
            Box::new(|v| v.stages.clear()),
            Box::new(|v| v.stages.push(false)),
            Box::new(|v| {
                v.catch = Some(Expr::new(
                    ExprKind::Lit(Lit::Unit),
                    Span::new(FileId::new(0), 0, 0),
                ));
            }),
            Box::new(|v| v.adapter.receiver = true),
            Box::new(|v| v.adapter.cancel = true),
            Box::new(|v| v.adapter.optional.push(0)),
            Box::new(|v| v.adapter.variadic = true),
            Box::new(|v| v.adapter.result.nullable = true),
            Box::new(|v| v.adapter.result.unit = true),
            Box::new(|v| v.adapter.callbacks.clear()),
            Box::new(|v| v.adapter.callbacks.push(None)),
            Box::new(|v| v.adapter.copies.args.clear()),
            Box::new(|v| v.adapter.copies.args.push(false)),
            Box::new(|v| v.adapter.copies.result = false),
            Box::new(|v| v.adapter.copies.callbacks.clear()),
            Box::new(|v| v.adapter.copies.callbacks.push(None)),
            Box::new(|v| v.adapter.shapes.clear()),
            Box::new(|v| v.adapter.shapes.push(Shape::Data)),
            Box::new(|v| v.adapter.result_shape = Shape::Option(Box::new(Shape::Data))),
        ];
        for index in 0..2 {
            edits.push(Box::new(move |v| v.stages[index] = true));
            edits.push(Box::new(move |v| v.adapter.callbacks[index] = Some(1)));
            edits.push(Box::new(move |v| v.adapter.copies.args[index] = true));
            edits.push(Box::new(move |v| {
                v.adapter.copies.callbacks[index] = Some((vec![], false));
            }));
            edits.push(Box::new(move |v| {
                v.adapter.shapes[index] = Shape::Option(Box::new(Shape::Data));
            }));
        }
        edits
    }
    #[test]
    fn genuine_range_pins_every_adapter_dimension_and_rejection_span() {
        let module = build::builtin_modules()
            .iter()
            .find(|m| m.name == "List.Core")
            .expect("List.Core");
        let index = module
            .items
            .iter()
            .position(|i| matches!(&i.kind,ItemKind::Foreign(f)if f.name=="range"))
            .expect("range");
        let item = &module.items[index];
        let ItemKind::Foreign(original) = &item.kind else {
            unreachable!()
        };
        assert!(supports(original));
        let entry = memory::Name::new("List.Core", "range");
        assert!(
            lower([module], &entry)
                .expect_err("no guessed Int domain without declarations")
                .to_string()
                .contains("requires a checked declaration context")
        );
        assert!(
            lower_with_declarations(
                [module],
                build::builtin_declarations().iter().map(AsRef::as_ref),
                &entry
            )
            .is_ok()
        );
        let edits = mutations();
        assert_eq!(edits.len(), 39);
        for (index, edit) in edits.into_iter().enumerate() {
            let mut altered = module.clone();
            let ItemKind::Foreign(value) = &mut altered
                .items
                .iter_mut()
                .find(|i| i.span == item.span)
                .expect("item")
                .kind
            else {
                unreachable!()
            };
            edit(value);
            assert!(!supports(value), "mutation {index}");
            assert!(
                matches!(lower_with_declarations([&altered],build::builtin_declarations().iter().map(AsRef::as_ref),&entry),Err(LowerError::Unsupported{span,..})if span==item.span),
                "mutation {index}"
            );
        }
    }
    #[test]
    fn list_range_lowers_directly_to_linear_intrinsic() {
        let module = build::builtin_modules()
            .iter()
            .find(|m| m.name == "List.Core")
            .expect("List.Core");
        let program = lower_with_declarations(
            [module],
            build::builtin_declarations().iter().map(AsRef::as_ref),
            &memory::Name::new("List.Core", "range"),
        )
        .expect("List.Core.range lowers");
        let intrinsics = program
            .functions
            .values()
            .flat_map(|f| f.body.instructions.iter())
            .filter(|i| {
                matches!(
                    i,
                    memory::Instruction::Let {
                        value: memory::Operation::Intrinsic {
                            kind: memory::Intrinsic::ListRange,
                            ..
                        },
                        ..
                    }
                )
            })
            .count();
        assert_eq!(intrinsics, 1);
    }
    #[test]
    fn checked_domain_proof_observes_private_aliases_but_never_guesses_named_types() {
        use super::super::scalar_context::ScalarContext;
        use crate::{
            diagnostics::Severity,
            project::{self, Host},
            source::SourceMap,
        };
        use std::{io, path::Path};
        struct Source(String);
        impl Host for Source {
            fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
                if path == Path::new("/Main.fl") {
                    Ok(self.0.as_bytes().to_vec())
                } else {
                    Err(io::ErrorKind::NotFound.into())
                }
            }
            fn display(&self, path: &Path) -> String {
                path.display().to_string()
            }
        }
        for (signature, accepted) in [
            ("Int -> Int -> List Int", true),
            ("Bound -> Bound -> Items Bound", true),
            ("Secret -> Secret -> Hidden", true),
            ("Signature Int", true),
            ("Float -> Int -> List Int", false),
            ("Int -> Float -> List Int", false),
            ("String -> String -> List Int", false),
            ("Count -> Count -> List Count", false),
            ("Int -> Int -> List Float", false),
            ("Int -> Int -> List a", false),
            ("Int -> Int -> (Int, Int)", false),
            ("Int -> Int -> List (List Int)", false),
            ("Int -> Int -> Box Int", false),
        ] {
            let text = Source(format!(
                "module App.RangeProof exposing (custom)\ntype alias Bound = Int\ntype alias Items a = List a\nopaque type alias Secret = Int\nopaque type alias Hidden = List Int\ntype alias Signature a = a -> a -> List a\ntype Count = Count Int\ntype Box a = Box a\ncustom : {signature}\nforeign pure custom from \"flick:rt\" \"listRange\"\n"
            ));
            let mut sources = SourceMap::default();
            let (diags, compiled) = project::compile(
                &text,
                &mut sources,
                None,
                &[Path::new("/Main.fl").to_path_buf()],
            );
            assert!(
                !diags.iter().any(|d| d.severity == Severity::Error),
                "{signature}: {diags:?}"
            );
            let context = ScalarContext::new(
                compiled
                    .modules
                    .iter()
                    .map(|m| m.declarations.as_ref())
                    .chain(build::builtin_declarations().iter().map(AsRef::as_ref)),
            )
            .expect("checked private/open facts");
            assert_eq!(
                context
                    .list_range(&memory::Name::new("App.RangeProof", "custom"))
                    .is_ok(),
                accepted,
                "{signature}"
            );
        }
    }
}

#[cfg(test)]
mod provenance_tests {
    use crate::{
        build,
        ir::{ItemKind, memory},
        opt::perceus::lower_with_declarations,
    };
    #[test]
    fn only_explicit_source_name_restores_a_generated_foreign_scheme() {
        let module = build::builtin_modules()
            .iter()
            .find(|m| m.name == "List.Core")
            .expect("List.Core");
        for source in [Some("range"), None, Some("missing")] {
            let mut altered = module.clone();
            let value = altered
                .items
                .iter_mut()
                .find_map(|i| match &mut i.kind {
                    ItemKind::Foreign(f) if f.name == "range" => Some(f),
                    _ => None,
                })
                .expect("range");
            value.name = "$range.raw".into();
            value.source_name = source.map(str::to_owned);
            let result = lower_with_declarations(
                [&altered],
                build::builtin_declarations().iter().map(AsRef::as_ref),
                &memory::Name::new("List.Core", "$range.raw"),
            );
            assert_eq!(
                result.is_ok(),
                source == Some("range"),
                "{source:?}: {result:?}"
            );
        }
    }
}
