//! Exact nonreturning native App.fail bridge; its payload is an ordinary record.
use super::{Code, Intrinsic, Local, Lower, LowerError, Name, Operation, Span, unsupported};
use flick_ir::{Adapter, Copies, Foreign, ForeignMode, ResultAdapter, Shape};

pub(super) fn is_binding(value: &Foreign) -> bool {
    value.binding.module_export() == Some(("flick:rt", "appFail"))
}

pub(super) fn supports(value: &Foreign) -> bool {
    is_binding(value)
        && value.mode == ForeignMode::Effect
        && value.arity == 1
        && value.dicts == 0
        && value.stages == [true]
        && value.catch.is_none()
        && value.adapter
            == Adapter {
                optional: vec![],
                owned: vec![],
                result: ResultAdapter {
                    nullable: false,
                    unit: false,
                },
                callbacks: vec![None],
                copies: Copies {
                    args: vec![false],
                    result: true,
                    callbacks: vec![None],
                },
                shapes: vec![Shape::Data],
                result_shape: Shape::Data,
                words: vec![],
            }
}

impl Lower<'_> {
    pub(super) fn app_fail(
        &mut self,
        value: &Foreign,
        name: &Name,
        span: Span,
        block: &mut Code,
    ) -> Result<Local, LowerError> {
        if !supports(value) {
            return Err(unsupported("App.fail ownership adapter", span));
        }
        let declarations = self
            .declarations
            .as_ref()
            .ok_or_else(|| unsupported("App.fail requires a checked declaration context", span))?;
        let source = Name::new(
            &name.module,
            value.source_name.as_deref().unwrap_or(&value.name),
        );
        declarations
            .app_fail(&source)
            .map_err(|message| unsupported(format!("App.fail: {message}"), span))?;
        // The effect remains cold until the unary closure is applied. The host
        // result-copy bit is discharged ONLY because Raise never returns.
        let code = self.reserve(span)?;
        let mut function = Code::new(vec![0, 1]);
        let details = function.emit(
            Operation::Construct {
                tag: "Prelude.Option.None".into(),
                fields: vec![],
            },
            span,
        )?;
        let record = function.emit(
            Operation::Record {
                fields: vec![("message".into(), 1), ("details".into(), details)],
            },
            span,
        )?;
        let payload = function.emit(
            Operation::Construct {
                tag: "Prelude.AppError.AppError".into(),
                fields: vec![record],
            },
            span,
        )?;
        let result = function.emit(
            Operation::Intrinsic {
                kind: Intrinsic::Raise,
                args: vec![payload],
            },
            span,
        )?;
        self.finish(code, &function.finish(result))?;
        block.emit(
            Operation::Closure {
                code,
                captures: vec![],
            },
            span,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::supports;
    use crate::{
        build,
        diagnostics::Severity,
        ir::{Expr, ExprKind, ItemKind, Lit},
        source::{FileId, Span},
    };
    use flick_ir::{Foreign, ForeignMode, Shape};

    #[test]
    #[allow(clippy::too_many_lines)]
    fn genuine_builtin_app_fail_pins_every_abi_dimension() {
        let original = build::builtin_modules()
            .iter()
            .find(|m| m.name == "App")
            .expect("genuine App builtin")
            .items
            .iter()
            .find_map(|i| {
                if let ItemKind::Foreign(f) = &i.kind {
                    Some(f)
                } else {
                    None
                }
            })
            .expect("genuine foreign");
        assert!(supports(original), "{original:#?}");
        let mutations: Vec<fn(&mut Foreign)> = vec![
            |v| {
                v.binding = flick_ir::Binding::Global {
                    name: v.binding.export_name().unwrap_or("").to_owned(),
                }
            },
            |v| {
                if let flick_ir::Binding::Module { name, .. } | flick_ir::Binding::Global { name } =
                    &mut v.binding
                {
                    name.push('!');
                }
            },
            |v| v.mode = ForeignMode::Pure,
            |v| v.mode = ForeignMode::Sync,
            |v| v.arity = 0,
            |v| v.arity = 2,
            |v| v.dicts = 1,
            |v| v.stages.clear(),
            |v| v.stages[0] = false,
            |v| v.stages.push(true),
            |v| {
                v.catch = Some(Expr::new(
                    ExprKind::Lit(Lit::Unit),
                    Span::new(FileId::new(0), 0, 0),
                ));
            },
            |v| {
                v.adapter.words.push(flick_ir::BackendAdapter {
                    name: "this".into(),
                    args: vec![],
                    span: flick_ir::Span::new(flick_ir::FileId::new(0), 0, 0),
                });
            },
            |v| {
                v.adapter.words.push(flick_ir::BackendAdapter {
                    name: "cancel".into(),
                    args: vec![flick_ir::AdapterArg::Word("signal".into())],
                    span: flick_ir::Span::new(flick_ir::FileId::new(0), 0, 0),
                });
            },
            |v| v.adapter.optional.push(0),
            |v| {
                v.adapter.words.push(flick_ir::BackendAdapter {
                    name: "variadic".into(),
                    args: vec![],
                    span: flick_ir::Span::new(flick_ir::FileId::new(0), 0, 0),
                });
            },
            |v| v.adapter.result.nullable = true,
            |v| v.adapter.result.unit = true,
            |v| v.adapter.callbacks.clear(),
            |v| v.adapter.callbacks.push(None),
            |v| v.adapter.callbacks[0] = Some(1),
            |v| v.adapter.copies.args.clear(),
            |v| v.adapter.copies.args.push(false),
            |v| v.adapter.copies.args[0] = true,
            |v| v.adapter.copies.result = false,
            |v| v.adapter.copies.callbacks.clear(),
            |v| v.adapter.copies.callbacks.push(None),
            |v| v.adapter.copies.callbacks[0] = Some((vec![false], false)),
            |v| v.adapter.shapes.clear(),
            |v| v.adapter.shapes.push(Shape::Data),
            |v| v.adapter.shapes[0] = Shape::Option(Box::new(Shape::Data)),
            |v| v.adapter.result_shape = Shape::Option(Box::new(Shape::Data)),
        ];
        let module = build::builtin_modules()
            .iter()
            .find(|m| m.name == "App")
            .expect("genuine module");
        let entry = super::Name::new("App", &original.name);
        assert!(
            super::super::lower([module], &entry)
                .expect_err("no declaration evidence")
                .to_string()
                .contains("requires a checked declaration context")
        );
        for (index, mutate) in mutations.into_iter().enumerate() {
            let mut changed = original.clone();
            mutate(&mut changed);
            assert!(!supports(&changed), "metadata mutation {index}");
            let mut module = module.clone();
            for item in &mut module.items {
                if matches!(&item.kind,ItemKind::Foreign(f) if f.name==original.name) {
                    item.kind = ItemKind::Foreign(changed.clone());
                }
            }
            let error = super::super::lower_with_declarations(
                [&module],
                build::builtin_declarations().iter().map(AsRef::as_ref),
                &entry,
            )
            .expect_err("mutated ABI rejects at lowering boundary");
            assert!(
                matches!(error, super::LowerError::Unsupported { .. }),
                "mutation {index}: {error}"
            );
        }
    }
    #[test]
    fn checked_input_proof_observes_aliases_and_forall_but_never_the_result_domain() {
        use super::super::scalar_context::ScalarContext;
        use super::Name;
        use crate::{
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
        for input in [
            "String",
            "Alias String",
            "Secret",
            "Int",
            "Float",
            "Unit",
            "a",
        ] {
            for output in ["Unit", "Int", "String", "a"] {
                let source = Source(format!(
                    "module App.Proof exposing (stop)\ntype alias Alias a = a\nopaque type alias Secret = String\nstop : {input} ->{{; AppError}} {output}\nforeign effect stop from \"flick:rt\" \"appFail\"\n"
                ));
                let mut sources = SourceMap::default();
                let (diagnostics, compiled) = project::compile(
                    &source,
                    &mut sources,
                    None,
                    &[Path::new("/Main.fl").to_path_buf()],
                );
                assert!(
                    !diagnostics.iter().any(|d| d.severity == Severity::Error),
                    "{input}->{output}: {diagnostics:?}"
                );
                let context = ScalarContext::new(
                    compiled
                        .modules
                        .iter()
                        .map(|m| m.declarations.as_ref())
                        .chain(build::builtin_declarations().iter().map(AsRef::as_ref)),
                )
                .expect("genuine checked private/open declarations");
                assert_eq!(
                    context.app_fail(&Name::new("App.Proof", "stop")).is_ok(),
                    matches!(input, "String" | "Alias String" | "Secret"),
                    "{input}->{output}"
                );
            }
        }
    }
    #[test]
    fn generated_foreign_identity_requires_explicit_source_name() {
        let module = build::builtin_modules()
            .iter()
            .find(|m| m.name == "App")
            .expect("App module");
        for source_name in [Some("fail"), None, Some("missing")] {
            let mut module = module.clone();
            for item in &mut module.items {
                if let ItemKind::Foreign(f) = &mut item.kind {
                    f.name = "$appFail.raw".into();
                    f.source_name = source_name.map(str::to_owned);
                }
            }
            let entry = super::Name::new("App", "$appFail.raw");
            let result = super::super::lower_with_declarations(
                [&module],
                build::builtin_declarations().iter().map(AsRef::as_ref),
                &entry,
            );
            assert_eq!(
                result.is_ok(),
                source_name == Some("fail"),
                "{source_name:?}: {result:?}"
            );
        }
    }
}
