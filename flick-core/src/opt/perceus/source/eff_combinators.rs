//! Exact cold task bridges, implemented only with ordinary Task/Await code.
//! Arguments remain language values; no host export or generic FFI exemption.
use crate::{
    ir::{Expr, ExprKind, Param, Pat, Stage, Stmt},
    source::Span,
};
use flick_ir::{Adapter, Copies, Foreign, ForeignMode, ResultAdapter, Shape};
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Succeed,
    Map,
    FlatMap,
    Map2,
}
impl Kind {
    fn arity(self) -> usize {
        match self {
            Self::Succeed => 1,
            Self::Map2 => 3,
            Self::Map | Self::FlatMap => 2,
        }
    }
}
fn kind(value: &Foreign) -> Option<Kind> {
    match value.binding.name.as_str() {
        "succeed" => Some(Kind::Succeed),
        "mapEff" => Some(Kind::Map),
        "flatMapEff" => Some(Kind::FlatMap),
        "map2Eff" => Some(Kind::Map2),
        _ => None,
    }
}
fn task_shape() -> Shape {
    Shape::Eff(Box::new(Shape::Data))
}
fn adapter(kind: Kind) -> Adapter {
    let arity = kind.arity();
    let mut callbacks = vec![None; arity];
    let mut copies = vec![None; arity];
    let mut args = vec![false; arity];
    let mut shapes = vec![Shape::Data; arity];
    if kind == Kind::Succeed {
        args[0] = true;
    } else {
        callbacks[0] = Some(if kind == Kind::Map2 { 2 } else { 1 });
        copies[0] = Some((vec![], false));
        for shape in &mut shapes[1..] {
            *shape = task_shape();
        }
    }
    if kind == Kind::FlatMap {
        shapes[0] = Shape::Fun {
            args: vec![Shape::Data],
            result: Box::new(task_shape()),
            runtime: false,
            sync: false,
        };
    }
    Adapter {
        optional: vec![],
        owned: vec![],
        result: ResultAdapter {
            nullable: false,
            unit: false,
        },
        callbacks,
        copies: Copies {
            args,
            result: false,
            callbacks: copies,
        },
        shapes,
        result_shape: task_shape(),
        words: vec![],
    }
}
fn supports(value: &Foreign, kind: Kind) -> bool {
    value.binding.from.as_deref() == Some("flick:rt")
        && value.mode == ForeignMode::Pure
        && value.arity == kind.arity()
        && value.dicts == 0
        && value.stages == vec![false; kind.arity()]
        && value.catch.is_none()
        && value.adapter == adapter(kind)
}
pub(super) fn expression(value: &Foreign, span: Span) -> Option<Expr> {
    let kind = kind(value)?;
    supports(value, kind).then(|| Builder(span).expression(kind))
}
struct Builder(Span);
impl Builder {
    fn at(&self, kind: ExprKind) -> Expr {
        Expr::new(kind, self.0)
    }
    fn local(&self, name: &str) -> Expr {
        self.at(ExprKind::Local(format!("$eff.{name}")))
    }
    fn bind(name: &str) -> Pat {
        Pat::Bind(format!("$eff.{name}"))
    }
    fn await_(&self, name: &str) -> Expr {
        self.at(ExprKind::Await(Box::new(self.local(name))))
    }
    fn apply(&self, args: &[&str]) -> Expr {
        args.iter().fold(self.local("callback"), |func, arg| {
            self.at(ExprKind::App {
                func: Box::new(func),
                arg: Box::new(self.local(arg)),
                stage: Stage::Pure,
                arg_first: false,
            })
        })
    }
    fn expression(&self, kind: Kind) -> Expr {
        let (params, body) = match kind {
            Kind::Succeed => (vec!["value"], self.local("value")),
            Kind::Map | Kind::FlatMap => {
                let applied = self.apply(&["value"]);
                let result = if kind == Kind::FlatMap {
                    self.at(ExprKind::Await(Box::new(applied)))
                } else {
                    applied
                };
                (
                    vec!["callback", "input"],
                    self.at(ExprKind::Block(
                        vec![Stmt::let_bind(Self::bind("value"), self.await_("input"))],
                        Box::new(result),
                    )),
                )
            }
            Kind::Map2 => (
                vec!["callback", "left", "right"],
                self.at(ExprKind::Block(
                    vec![
                        Stmt::let_bind(Self::bind("leftValue"), self.await_("left")),
                        Stmt::let_bind(Self::bind("rightValue"), self.await_("right")),
                    ],
                    Box::new(self.apply(&["leftValue", "rightValue"])),
                )),
            ),
        };
        // Lambda parameters evaluate once at construction. The entire computation,
        // including BOTH awaits before map2's first callback stage, stays inside Task.
        self.at(ExprKind::Lambda {
            params: params
                .into_iter()
                .map(|name| Param {
                    pat: Self::bind(name),
                    stage: Stage::Pure,
                    ty: None,
                    binder_types: Vec::new(),
                })
                .collect(),
            body: Box::new(self.at(ExprKind::Task(Box::new(body)))),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Builder, Kind, adapter, expression, kind, supports};
    use crate::{
        build,
        ir::{Expr, ExprKind, Foreign, ItemKind, Lit, Stage, Stmt, memory},
        opt::perceus::{LowerError, lower},
        source::{FileId, Span},
    };
    use flick_ir::{ForeignMode, Shape};
    type Mutation = Box<dyn Fn(&mut Foreign)>;
    fn mutations(original: &Foreign) -> Vec<Mutation> {
        let mut edits: Vec<Mutation> = vec![
            Box::new(|v| v.binding.from = None),
            Box::new(|v| v.binding.name.push('!')),
            Box::new(|v| v.mode = ForeignMode::Sync),
            Box::new(|v| v.mode = ForeignMode::Async),
            Box::new(|v| v.mode = ForeignMode::Effect),
            Box::new(|v| v.arity = 0),
            Box::new(|v| v.arity += 1),
            Box::new(|v| v.dicts = 1),
            Box::new(|v| v.stages.clear()),
            Box::new(|v| v.stages.push(false)),
            Box::new(|v| {
                v.catch = Some(Expr::new(
                    ExprKind::Lit(Lit::Unit),
                    Span::new(FileId::new(0), 0, 0),
                ));
            }),
            Box::new(|v| {
                v.adapter.words.push(flick_ir::AdapterWord {
                    name: "this".into(),
                    args: vec![],
                    span: Span::new(FileId::new(0), 0, 0),
                });
            }),
            Box::new(|v| {
                v.adapter.words.push(flick_ir::AdapterWord {
                    name: "cancel".into(),
                    args: vec![flick_ir::AdapterArg::Ident("signal".into())],
                    span: Span::new(FileId::new(0), 0, 0),
                });
            }),
            Box::new(|v| v.adapter.optional.push(0)),
            Box::new(|v| v.adapter.owned.push(0)),
            Box::new(|v| {
                v.adapter.words.push(flick_ir::AdapterWord {
                    name: "variadic".into(),
                    args: vec![],
                    span: Span::new(FileId::new(0), 0, 0),
                });
            }),
            Box::new(|v| v.adapter.result.unit = true),
            Box::new(|v| v.adapter.result.nullable = true),
            Box::new(|v| v.adapter.callbacks.clear()),
            Box::new(|v| v.adapter.callbacks.push(None)),
            Box::new(|v| v.adapter.copies.args.clear()),
            Box::new(|v| v.adapter.copies.args.push(false)),
            Box::new(|v| v.adapter.copies.result = true),
            Box::new(|v| v.adapter.copies.callbacks.clear()),
            Box::new(|v| v.adapter.copies.callbacks.push(None)),
            Box::new(|v| v.adapter.shapes.clear()),
            Box::new(|v| v.adapter.shapes.push(Shape::Data)),
            Box::new(|v| v.adapter.result_shape = Shape::Data),
            Box::new(|v| {
                v.adapter.result_shape = Shape::Eff(Box::new(Shape::Option(Box::new(Shape::Data))));
            }),
        ];
        for index in 0..original.arity {
            edits.push(Box::new(move |v| v.stages[index] = true));
            edits.push(Box::new(move |v| v.adapter.callbacks[index] = Some(9)));
            edits.push(Box::new(move |v| v.adapter.callbacks[index] = None));
            edits.push(Box::new(move |v| {
                v.adapter.copies.args[index] = !v.adapter.copies.args[index];
            }));
            edits.push(Box::new(move |v| {
                v.adapter.copies.callbacks[index] = Some((vec![true], true));
            }));
            edits.push(Box::new(move |v| v.adapter.copies.callbacks[index] = None));
            edits.push(Box::new(move |v| {
                v.adapter.shapes[index] = Shape::Option(Box::new(Shape::Data));
            }));
        }
        if original.binding.name == "flatMapEff" {
            for mode in 0..4 {
                edits.push(Box::new(move |v| {
                    let Shape::Fun {
                        args,
                        result,
                        runtime,
                        sync,
                    } = &mut v.adapter.shapes[0]
                    else {
                        unreachable!()
                    };
                    match mode {
                        0 => args.clear(),
                        1 => **result = Shape::Data,
                        2 => *runtime = true,
                        _ => *sync = true,
                    }
                }));
            }
        }
        edits.retain(|edit| {
            let mut altered = original.clone();
            edit(&mut altered);
            altered != *original
        });
        edits
    }
    #[test]
    fn all_four_genuine_task_abis_and_rejection_spans_are_pinned() {
        let module = build::builtin_modules()
            .iter()
            .find(|m| m.name == "Instances")
            .expect("Instances");
        let mut seen = vec![];
        for (index, item) in module.items.iter().enumerate() {
            let ItemKind::Foreign(original) = &item.kind else {
                continue;
            };
            let Some(kind) = kind(original) else { continue };
            seen.push(original.binding.name.clone());
            assert!(supports(original, kind), "{original:#?}");
            assert_eq!(original.adapter, adapter(kind));
            let entry = memory::Name::new("Instances", &original.name);
            assert!(lower([module], &entry).is_ok());
            let edits = mutations(original);
            eprintln!(
                "{} {} rejected ABI mutations",
                original.binding.name,
                edits.len()
            );
            for (number, edit) in edits.into_iter().enumerate() {
                let mut altered = module.clone();
                let ItemKind::Foreign(value) = &mut altered.items[index].kind else {
                    unreachable!()
                };
                edit(value);
                assert!(
                    expression(value, item.span).is_none(),
                    "{} mutation{number}",
                    original.name
                );
                assert!(
                    matches!(lower([&altered],&entry),Err(LowerError::Unsupported{span,..})if span==item.span),
                    "{} mutation{number}",
                    original.name
                );
            }
        }
        seen.sort();
        assert_eq!(seen, ["flatMapEff", "map2Eff", "mapEff", "succeed"]);
    }
    fn inspect(expr: &Expr, callbacks: &mut usize) {
        match &expr.kind {
            ExprKind::App {
                stage, arg_first, ..
            } => {
                assert_eq!(*stage, Stage::Pure);
                assert!(!arg_first);
            }
            ExprKind::Lambda { params, .. } => {
                assert!(params.iter().all(|p| p.stage == Stage::Pure));
            }
            ExprKind::Local(name) if name == "$eff.callback" => *callbacks += 1,
            ExprKind::Global { .. } | ExprKind::Perform { .. } | ExprKind::Handle(_) => {
                panic!("no host export or provider captured by template")
            }
            _ => {}
        }
        expr.each_child(&mut |child| inspect(child, callbacks));
    }
    #[test]
    fn computations_are_inside_task_and_both_awaits_precede_the_first_callback_stage() {
        for kind in [Kind::Succeed, Kind::Map, Kind::FlatMap, Kind::Map2] {
            let template = Builder(Span::new(FileId::new(0), 0, 0)).expression(kind);
            assert!(super::super::free::free(&template).is_empty());
            let mut callbacks = 0;
            inspect(&template, &mut callbacks);
            assert_eq!(callbacks, usize::from(kind != Kind::Succeed));
            let ExprKind::Lambda { params, body } = &template.kind else {
                panic!("ordinary stages")
            };
            assert_eq!(params.len(), kind.arity());
            assert!(matches!(&body.kind, ExprKind::Task(_)));
            if kind == Kind::Map2 {
                let ExprKind::Task(body) = &body.kind else {
                    unreachable!()
                };
                let ExprKind::Block(steps, callback) = &body.kind else {
                    panic!("explicit sequential awaits")
                };
                assert_eq!(steps.len(), 2);
                for (step, name) in steps.iter().zip(["$eff.left", "$eff.right"]) {
                    let Stmt::Let { value, .. } = step else {
                        panic!("await binding")
                    };
                    let ExprKind::Await(input) = &value.kind else {
                        panic!("await before callback")
                    };
                    assert!(matches!(&input.kind,ExprKind::Local(actual)if actual==name));
                }
                assert!(matches!(&callback.kind, ExprKind::App { .. }));
            }
        }
    }
}
