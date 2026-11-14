//! Cartesian product through the existing ordinary collection templates.
//! Materialized tails and concatenation retain the quadratic-copy baseline.
use super::{BinOp, Builder, Expr, ExprKind, Stmt};
impl Builder {
    pub(super) fn map2(&self) -> Expr {
        let row_more = self.block(
            vec![
                Stmt::Let(
                    Self::bind("mapped"),
                    self.apply(
                        "callback",
                        vec![self.local("leftHead"), self.local("rightHead")],
                    ),
                ),
                Stmt::Let(
                    Self::bind("rest"),
                    self.apply("row", vec![self.local("leftHead"), self.local("rightTail")]),
                ),
            ],
            self.binary(
                BinOp::ListConcat,
                self.at(ExprKind::List(vec![self.local("mapped")])),
                self.local("rest"),
            ),
        );
        let row = self.at(ExprKind::Match(
            Box::new(self.local("items")),
            vec![
                Self::arm(Self::empty(), self.at(ExprKind::List(vec![]))),
                Self::arm(Self::cons("rightHead", "rightTail"), row_more),
            ],
        ));
        // Finish the current row BEFORE recursing on the outer tail. Ordinary
        // eager recursion as a callback argument would reverse callback order.
        let outer_more = self.block(
            vec![
                Stmt::Let(
                    Self::bind("rowValues"),
                    self.apply("row", vec![self.local("head"), self.local("right")]),
                ),
                Stmt::Let(
                    Self::bind("rest"),
                    self.apply("outer", vec![self.local("tail")]),
                ),
            ],
            self.binary(
                BinOp::ListConcat,
                self.local("rowValues"),
                self.local("rest"),
            ),
        );
        let outer = self.at(ExprKind::Match(
            Box::new(self.local("items")),
            vec![
                Self::arm(Self::empty(), self.at(ExprKind::List(vec![]))),
                Self::arm(Self::cons("head", "tail"), outer_more),
            ],
        ));
        // All three source argument expressions are already eager; loop over
        // their values, never their expressions, and leave partial calls cold.
        self.lambda(
            &["callback", "left", "right"],
            self.block(
                vec![
                    self.function("row", &["leftHead", "items"], row),
                    self.function("outer", &["items"], outer),
                ],
                self.apply("outer", vec![self.local("left")]),
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        Builder, Collection, expression,
        tests::{callback_mentions, mutations, pure},
    };
    use crate::{
        build,
        ir::{Expr, ExprKind, ItemKind, Stmt, memory},
        opt::perceus::{LowerError, lower},
    };
    use flick_ir::{Foreign, ForeignMode};
    #[test]
    fn exact_rank_two_list_abi_rejects_every_metadata_dimension_with_original_span() {
        let module = build::builtin_modules()
            .iter()
            .find(|m| m.name == "Instances")
            .expect("genuine Instances");
        let index = module
            .items
            .iter()
            .position(|i| matches!(&i.kind,ItemKind::Foreign(f) if f.name=="map2_list"))
            .expect("genuine bridge");
        let item = &module.items[index];
        let ItemKind::Foreign(original) = &item.kind else {
            unreachable!()
        };
        assert!(expression(original, item.span).is_some());
        let mut edits = mutations(original);
        let extra: Vec<fn(&mut Foreign)> = vec![
            |f| f.mode = ForeignMode::Async,
            |f| f.mode = ForeignMode::Effect,
            |f| f.arity = 2,
            |f| f.arity = 4,
            |f| f.adapter.callbacks[0] = Some(1),
            |f| f.adapter.callbacks[0] = Some(3),
        ];
        edits.extend(
            extra
                .into_iter()
                .map(|f| Box::new(f) as Box<dyn Fn(&mut Foreign)>),
        );
        assert!(edits.len() >= 49, "{} dimensions", edits.len());
        eprintln!("listMap2 {} rejected ABI mutations", edits.len());
        let entry = memory::Name::new("Instances", "map2_list");
        assert!(lower([module], &entry).is_ok());
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
            assert!(expression(value, item.span).is_none(), "mutation {index}");
            assert!(
                matches!(lower([&altered],&entry),Err(LowerError::Unsupported{span,..}) if span==item.span),
                "mutation {index}"
            );
        }
    }
    fn callee(expr: &Expr) -> &str {
        match &expr.kind {
            ExprKind::Local(name) => name,
            ExprKind::App { func, .. } => callee(func),
            _ => panic!("local application"),
        }
    }
    #[test]
    fn row_and_pair_results_are_bound_before_their_respective_tail_recursions() {
        let module = build::builtin_modules()
            .iter()
            .find(|m| m.name == "Instances")
            .expect("Instances");
        let item = module
            .items
            .iter()
            .find(|i| matches!(&i.kind,ItemKind::Foreign(f) if f.name=="map2_list"))
            .expect("bridge");
        let template = Builder(item.span).collection(Collection::Map2);
        assert!(super::super::super::free::free(&template).is_empty());
        pure(&template);
        assert_eq!(
            callback_mentions(&template),
            1,
            "one callback spine per ordered pair, not per distinct value"
        );
        let ExprKind::Lambda { params, body } = &template.kind else {
            panic!("three source stages")
        };
        assert_eq!(params.len(), 3);
        let ExprKind::Block(functions, _) = &body.kind else {
            panic!("row and outer loops")
        };
        assert_eq!(functions.len(), 2);
        for (function, first, second) in [
            (&functions[0], "$collection.callback", "$collection.row"),
            (&functions[1], "$collection.row", "$collection.outer"),
        ] {
            let Stmt::Fun { value, .. } = function else {
                panic!("ordinary recursive loop")
            };
            let ExprKind::Lambda { body, .. } = &value.kind else {
                panic!("loop parameters")
            };
            let ExprKind::Match(_, arms) = &body.kind else {
                panic!("list match")
            };
            let ExprKind::Block(steps, _) = &arms[1].body.kind else {
                panic!("explicit sequencing")
            };
            assert_eq!(steps.len(), 2);
            for (step, expected) in steps.iter().zip([first, second]) {
                let Stmt::Let(_, value) = step else {
                    panic!("explicit result binding")
                };
                assert_eq!(callee(value), expected);
            }
        }
    }
}
