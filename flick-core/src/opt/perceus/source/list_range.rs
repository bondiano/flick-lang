//! Exact checked Int ranges through ordinary code; materialization is not FBIP.
use super::{
    BinOp, Code, Env, Expr, ExprKind, Lit, Local, Lower, LowerError, Name, Param, Pat, Span, Stage,
    Stmt, unsupported,
};
use flick_ir::{Adapter, Arith, Copies, Foreign, ForeignMode, Num, ResultAdapter, Shape};
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
        self.expr(&Builder(span).expression(), &Env::new(), block)
    }
}
struct Builder(Span);
impl Builder {
    fn at(&self, kind: ExprKind) -> Expr {
        Expr::new(kind, self.0)
    }
    fn local(&self, name: &str) -> Expr {
        self.at(ExprKind::Local(format!("$range.{name}")))
    }
    fn bind(name: &str) -> Pat {
        Pat::Bind(format!("$range.{name}"))
    }
    fn lambda(&self, names: &[&str], body: Expr) -> Expr {
        self.at(ExprKind::Lambda {
            params: names
                .iter()
                .map(|name| Param {
                    pat: Self::bind(name),
                    stage: Stage::Pure,
                    ty: None,
                })
                .collect(),
            body: Box::new(body),
        })
    }
    fn expression(&self) -> Expr {
        let next = self.at(ExprKind::Binary(
            BinOp::Arith(Arith::Add, Num::Int),
            Box::new(self.local("current")),
            Box::new(self.at(ExprKind::Lit(Lit::Int(1)))),
        ));
        let rest = self.at(ExprKind::App {
            func: Box::new(self.local("loop")),
            arg: Box::new(next),
            stage: Stage::Pure,
            arg_first: false,
        });
        let cons = self.at(ExprKind::Binary(
            BinOp::ListConcat,
            Box::new(self.at(ExprKind::List(vec![self.local("current")]))),
            Box::new(rest),
        ));
        // The increment exists ONLY in the current < until branch. Even until=MAX
        // stops at MAX without adding to it; equal/reversed endpoints allocate [].
        let body = self.at(ExprKind::If(
            Box::new(self.at(ExprKind::Binary(
                BinOp::Lt,
                Box::new(self.local("current")),
                Box::new(self.local("until")),
            ))),
            Box::new(cons),
            Box::new(self.at(ExprKind::List(vec![]))),
        ));
        let invoke = self.at(ExprKind::App {
            func: Box::new(self.local("loop")),
            arg: Box::new(self.local("from")),
            stage: Stage::Pure,
            arg_first: false,
        });
        self.lambda(
            &["from", "until"],
            self.at(ExprKind::Block(
                vec![Stmt::Fun {
                    name: "$range.loop".into(),
                    rec: true,
                    dicts: vec![],
                    value: self.lambda(&["current"], body),
                }],
                Box::new(invoke),
            )),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Builder, supports};
    use crate::{
        build,
        ir::{BinOp, Expr, ExprKind, Foreign, ItemKind, Lit, Stmt, memory},
        opt::perceus::{LowerError, lower, lower_with_declarations},
        source::{FileId, Span},
    };
    use flick_ir::{Arith, ForeignMode, Num, Shape};
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
    fn additions(expr: &Expr) -> usize {
        let mut count = usize::from(matches!(
            &expr.kind,
            ExprKind::Binary(BinOp::Arith(Arith::Add, Num::Int), ..)
        ));
        expr.each_child(&mut |child| count += additions(child));
        count
    }
    #[test]
    fn ordinary_template_guards_increment_and_keeps_both_source_parameters_cold() {
        let template = Builder(Span::new(FileId::new(0), 0, 0)).expression();
        assert!(super::super::free::free(&template).is_empty());
        let ExprKind::Lambda { params, body } = &template.kind else {
            panic!("two curried parameters")
        };
        assert_eq!(params.len(), 2);
        let ExprKind::Block(functions, _) = &body.kind else {
            panic!("ordinary recursive loop")
        };
        let Stmt::Fun { value, .. } = &functions[0] else {
            panic!("loop")
        };
        let ExprKind::Lambda { body, .. } = &value.kind else {
            panic!("current parameter")
        };
        let ExprKind::If(condition, yes, no) = &body.kind else {
            panic!("guard before increment")
        };
        assert!(matches!(&condition.kind, ExprKind::Binary(BinOp::Lt, ..)));
        assert!(matches!(&no.kind,ExprKind::List(items)if items.is_empty()));
        assert_eq!(additions(condition), 0);
        assert_eq!(additions(no), 0);
        assert_eq!(
            additions(yes),
            1,
            "increment occurs only below current < until"
        );
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
