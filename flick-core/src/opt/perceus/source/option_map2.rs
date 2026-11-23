//! The exact rank-two Option bridge, discharged by closed ordinary Flick code.
//! No callback or data is exported to a host: copy bits are not erased for FFI.
use crate::{
    ir::{Arm, Expr, ExprKind, Param, Pat, Stage},
    source::Span,
};
use flick_ir::{Adapter, Constructor, Copies, Foreign, ForeignMode, Rep, ResultAdapter, Shape};

fn supports(value: &Foreign) -> bool {
    value.binding.from.as_deref() == Some("flick:rt")
        && value.binding.name == "optionMap2"
        && value.mode == ForeignMode::Pure
        && value.arity == 3
        && value.dicts == 0
        && value.stages == [false, false, false]
        && value.catch.is_none()
        && value.adapter
            == Adapter {
                optional: vec![],
                owned: vec![],
                result: ResultAdapter {
                    nullable: false,
                    unit: false,
                },
                callbacks: vec![Some(2), None, None],
                copies: Copies {
                    args: vec![false, true, true],
                    result: true,
                    callbacks: vec![Some((vec![], false)), None, None],
                },
                shapes: vec![
                    Shape::Data,
                    Shape::Option(Box::new(Shape::Data)),
                    Shape::Option(Box::new(Shape::Data)),
                ],
                result_shape: Shape::Option(Box::new(Shape::Data)),
                words: vec![],
            }
}

pub(super) fn expression(value: &Foreign, span: Span) -> Option<Expr> {
    supports(value).then(|| Builder(span).expression())
}

struct Builder(Span);
impl Builder {
    fn at(&self, kind: ExprKind) -> Expr {
        Expr::new(kind, self.0)
    }
    fn bind(name: &str) -> Pat {
        Pat::Bind(format!("$optionMap2.{name}"))
    }
    fn local(&self, name: &str) -> Expr {
        self.at(ExprKind::Local(format!("$optionMap2.{name}")))
    }
    fn constructor(name: &str) -> Constructor {
        Constructor {
            module: "Prelude".into(),
            ty: "Option".into(),
            name: name.into(),
            arity: usize::from(name == "Some"),
            dicts: 0,
            rep: Rep::Tagged,
            fields: Vec::new(),
            type_args: Vec::new(),
        }
    }
    fn apply(&self, func: Expr, arg: Expr) -> Expr {
        self.at(ExprKind::App {
            func: Box::new(func),
            arg: Box::new(arg),
            stage: Stage::Pure,
            arg_first: false,
        })
    }
    fn none(&self) -> Expr {
        self.at(ExprKind::Con(Self::constructor("None")))
    }
    fn matching(&self, input: &str, payload: &str, some: Expr) -> Expr {
        self.at(ExprKind::Match(
            Box::new(self.local(input)),
            vec![
                Arm {
                    pat: Pat::Con(Self::constructor("None"), vec![]),
                    binder_types: Vec::new(),
                    guard: None,
                    body: self.none(),
                },
                Arm {
                    pat: Pat::Con(Self::constructor("Some"), vec![Self::bind(payload)]),
                    binder_types: Vec::new(),
                    guard: None,
                    body: some,
                },
            ],
        ))
    }
    fn expression(&self) -> Expr {
        let result = self.apply(
            self.apply(self.local("callback"), self.local("leftValue")),
            self.local("rightValue"),
        );
        let some = self.apply(self.at(ExprKind::Con(Self::constructor("Some"))), result);
        let right = self.matching("right", "rightValue", some);
        let body = self.matching("left", "leftValue", right);
        // All three source arguments have already evaluated, left to right,
        // before these matches run. None skips only callback applications.
        self.at(ExprKind::Lambda {
            params: ["callback", "left", "right"]
                .into_iter()
                .map(|name| Param {
                    pat: Self::bind(name),
                    stage: Stage::Pure,
                    ty: None,
                    binder_types: Vec::new(),
                })
                .collect(),
            body: Box::new(body),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{expression, supports};
    use crate::{
        build,
        ir::{Expr, ExprKind, Foreign, ItemKind, Lit, Module, Stage, memory},
        opt::perceus::{LowerError, lower},
        source::{FileId, Span},
    };
    use flick_ir::{ForeignMode, Shape};
    type Mutation = Box<dyn Fn(&mut Foreign)>;
    fn mutations() -> Vec<Mutation> {
        let mut changes: Vec<Mutation> = vec![
            Box::new(|v| v.binding.from = None),
            Box::new(|v| v.binding.name.push('!')),
            Box::new(|v| v.mode = ForeignMode::Sync),
            Box::new(|v| v.mode = ForeignMode::Async),
            Box::new(|v| v.mode = ForeignMode::Effect),
            Box::new(|v| v.arity = 0),
            Box::new(|v| v.arity = 2),
            Box::new(|v| v.arity = 4),
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
                v.adapter.words.push(flick_ir::BackendAdapter {
                    name: "this".into(),
                    args: vec![],
                    span: flick_ir::Span::new(flick_ir::FileId::new(0), 0, 0),
                });
            }),
            Box::new(|v| {
                v.adapter.words.push(flick_ir::BackendAdapter {
                    name: "cancel".into(),
                    args: vec![flick_ir::AdapterArg::Word("signal".into())],
                    span: flick_ir::Span::new(flick_ir::FileId::new(0), 0, 0),
                });
            }),
            Box::new(|v| v.adapter.optional.push(0)),
            Box::new(|v| {
                v.adapter.words.push(flick_ir::BackendAdapter {
                    name: "variadic".into(),
                    args: vec![],
                    span: flick_ir::Span::new(flick_ir::FileId::new(0), 0, 0),
                });
            }),
            Box::new(|v| v.adapter.result.nullable = true),
            Box::new(|v| v.adapter.result.unit = true),
            Box::new(|v| v.adapter.callbacks.clear()),
            Box::new(|v| v.adapter.callbacks.push(None)),
            Box::new(|v| v.adapter.callbacks[0] = Some(1)),
            Box::new(|v| v.adapter.callbacks[0] = Some(3)),
            Box::new(|v| v.adapter.callbacks[0] = None),
            Box::new(|v| v.adapter.callbacks[1] = Some(1)),
            Box::new(|v| v.adapter.callbacks[2] = Some(1)),
            Box::new(|v| v.adapter.copies.args.clear()),
            Box::new(|v| v.adapter.copies.args.push(true)),
            Box::new(|v| v.adapter.copies.result = false),
            Box::new(|v| v.adapter.copies.callbacks.clear()),
            Box::new(|v| v.adapter.copies.callbacks.push(None)),
            Box::new(|v| v.adapter.copies.callbacks[0] = None),
            Box::new(|v| v.adapter.copies.callbacks[0] = Some((vec![], true))),
            Box::new(|v| v.adapter.copies.callbacks[0] = Some((vec![true, true], false))),
            Box::new(|v| v.adapter.copies.callbacks[1] = Some((vec![], false))),
            Box::new(|v| v.adapter.copies.callbacks[2] = Some((vec![], false))),
            Box::new(|v| v.adapter.shapes.clear()),
            Box::new(|v| v.adapter.shapes.push(Shape::Data)),
            Box::new(|v| v.adapter.shapes[0] = Shape::Option(Box::new(Shape::Data))),
            Box::new(|v| v.adapter.shapes[1] = Shape::Data),
            Box::new(|v| v.adapter.shapes[2] = Shape::Data),
            Box::new(|v| {
                v.adapter.shapes[1] = Shape::Option(Box::new(Shape::Option(Box::new(Shape::Data))));
            }),
            Box::new(|v| v.adapter.result_shape = Shape::Data),
            Box::new(|v| {
                v.adapter.result_shape =
                    Shape::Option(Box::new(Shape::Option(Box::new(Shape::Data))));
            }),
        ];
        for index in 0..3 {
            changes.push(Box::new(move |v| v.stages[index] = true));
            changes.push(Box::new(move |v| {
                v.adapter.copies.args[index] = !v.adapter.copies.args[index];
            }));
        }
        changes
    }
    fn genuine() -> (&'static Module, usize) {
        let module = build::builtin_modules()
            .iter()
            .find(|m| m.name == "Instances")
            .expect("genuine Instances");
        let index = module
            .items
            .iter()
            .position(|i| matches!(&i.kind,ItemKind::Foreign(v) if v.name=="map2_option"))
            .expect("genuine rank-two bridge");
        (module, index)
    }
    #[test]
    fn genuine_rank_two_abi_and_every_adapter_dimension_are_pinned_with_original_spans() {
        let (module, index) = genuine();
        let item = &module.items[index];
        let ItemKind::Foreign(original) = &item.kind else {
            unreachable!()
        };
        assert!(supports(original), "{original:#?}");
        let entry = memory::Name::new("Instances", "map2_option");
        let program = lower([module], &entry).expect("closed ordinary template lowers");
        assert_eq!(memory::validate_program(&program), Ok(()));
        let changes = mutations();
        assert_eq!(changes.len(), 49);
        for (mutation, change) in changes.into_iter().enumerate() {
            let mut altered = module.clone();
            let ItemKind::Foreign(value) = &mut altered.items[index].kind else {
                unreachable!()
            };
            change(value);
            assert!(
                expression(value, item.span).is_none(),
                "mutation {mutation}"
            );
            assert!(
                matches!(lower([&altered],&entry),Err(LowerError::Unsupported{span,..}) if span==item.span),
                "mutation {mutation} preserves original declaration span"
            );
        }
    }
    fn inspect(expr: &Expr, callback_mentions: &mut usize) {
        match &expr.kind {
            ExprKind::Local(name) if name == "$optionMap2.callback" => *callback_mentions += 1,
            ExprKind::App {
                stage, arg_first, ..
            } => {
                assert_eq!(*stage, Stage::Pure);
                assert!(!arg_first);
            }
            ExprKind::Lambda { params, .. } => {
                assert!(params.iter().all(|p| p.stage == Stage::Pure));
            }
            ExprKind::Con(constructor) => {
                assert_eq!(constructor.module, "Prelude");
                assert_eq!(constructor.ty, "Option");
                assert!(matches!(constructor.name.as_str(), "Some" | "None"));
                assert_eq!(constructor.dicts, 0);
                assert_eq!(constructor.arity, usize::from(constructor.name == "Some"));
            }
            ExprKind::Global { .. }
            | ExprKind::Task(_)
            | ExprKind::Await(_)
            | ExprKind::Within(..) => panic!("no host or runtime shortcut"),
            _ => {}
        }
        expr.each_child(&mut |child| inspect(child, callback_mentions));
    }
    #[test]
    fn closed_pure_template_has_one_callback_spine_only_below_both_some_patterns() {
        let (module, index) = genuine();
        let item = &module.items[index];
        let ItemKind::Foreign(original) = &item.kind else {
            unreachable!()
        };
        let template = expression(original, item.span).expect("real template");
        assert!(super::super::free::free(&template).is_empty());
        let mut count = 0;
        inspect(&template, &mut count);
        assert_eq!(
            count, 1,
            "one callback invocation site; no duplicate application"
        );
        let ExprKind::Lambda { params, body } = &template.kind else {
            panic!("three cold curried parameters")
        };
        assert_eq!(params.len(), 3);
        let ExprKind::Match(_, left_arms) = &body.kind else {
            panic!("left Option match")
        };
        assert!(matches!(&left_arms[0].body.kind,ExprKind::Con(c) if c.name=="None"));
        let ExprKind::Match(_, right_arms) = &left_arms[1].body.kind else {
            panic!("right Option match below left Some")
        };
        assert!(matches!(&right_arms[0].body.kind,ExprKind::Con(c) if c.name=="None"));
    }
}
