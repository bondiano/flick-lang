//! Owned left-to-right rows, using the collection templates' copy baseline.
use super::{BinOp, Builder, Expr, ExprKind, Stmt};
impl Builder {
    pub(super) fn flat_map(&self) -> Expr {
        let more = self.block(
            vec![
                Stmt::let_bind(
                    Self::bind("row"),
                    self.apply(
                        "normalize",
                        vec![self.apply("callback", vec![self.local("head")])],
                    ),
                ),
                Stmt::let_bind(
                    Self::bind("rest"),
                    self.apply("loop", vec![self.local("tail")]),
                ),
            ],
            self.binary(BinOp::ListConcat, self.local("row"), self.local("rest")),
        );
        let body = self.at(ExprKind::Match(
            Box::new(self.local("items")),
            vec![
                Self::arm(Self::empty(), self.at(ExprKind::List(vec![]))),
                Self::arm(Self::cons("head", "tail"), more),
            ],
        ));
        self.lambda(
            &["normalize", "callback"],
            self.block(
                vec![self.function("loop", &["items"], body)],
                self.lambda(
                    &["input"],
                    self.apply(
                        "loop",
                        vec![self.apply("normalize", vec![self.local("input")])],
                    ),
                ),
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
        ir::{Expr, ExprKind, Foreign, ItemKind, Stmt, memory},
        opt::perceus::{LowerError, lower},
    };
    use flick_ir::ForeignMode;
    #[test]
    fn exact_flat_map_contract_rejects_every_metadata_change_with_original_span() {
        let module = build::builtin_modules()
            .iter()
            .find(|m| m.name == "Instances")
            .expect("Instances");
        let item = module
            .items
            .iter()
            .find(|i| matches!(&i.kind,ItemKind::Foreign(f)if f.name=="flat_map_list"))
            .expect("genuine FlatMap");
        let ItemKind::Foreign(original) = &item.kind else {
            unreachable!()
        };
        assert!(expression(original, item.span).is_some());
        let entry = memory::Name::new("Instances", "flat_map_list");
        assert!(lower([module], &entry).is_ok());
        let mut edits = mutations(original);
        let extra: Vec<fn(&mut Foreign)> = vec![
            |v| v.mode = ForeignMode::Async,
            |v| v.mode = ForeignMode::Effect,
            |v| v.arity = 1,
            |v| v.arity = 3,
        ];
        edits.extend(
            extra
                .into_iter()
                .map(|f| Box::new(f) as Box<dyn Fn(&mut Foreign)>),
        );
        eprintln!("listFlatMap {} rejected ABI mutations", edits.len());
        for (number, edit) in edits.into_iter().enumerate() {
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
            assert!(expression(value, item.span).is_none(), "mutation{number}");
            assert!(
                matches!(lower([&altered],&entry),Err(LowerError::Unsupported{span,..})if span==item.span),
                "mutation{number}"
            );
        }
    }
    fn callee(expr: &Expr) -> &str {
        match &expr.kind {
            ExprKind::Local(name) => name,
            ExprKind::App { func, .. } => callee(func),
            _ => panic!("local call"),
        }
    }
    #[test]
    fn completed_normalized_row_precedes_the_tail_and_normalizer_is_hidden() {
        let module = build::builtin_modules()
            .iter()
            .find(|m| m.name == "Instances")
            .expect("Instances");
        let item = module
            .items
            .iter()
            .find(|i| matches!(&i.kind,ItemKind::Foreign(f)if f.name=="flat_map_list"))
            .expect("bridge");
        let template = Builder(item.span).collection(Collection::FlatMap);
        assert!(super::super::super::free::free(&template).is_empty());
        pure(&template);
        assert_eq!(callback_mentions(&template), 1);
        let ExprKind::Lambda { params, body } = &template.kind else {
            panic!("normalizer then callback")
        };
        assert_eq!(params.len(), 2);
        let ExprKind::Block(functions, _) = &body.kind else {
            panic!("loop")
        };
        let Stmt::Fun { value, .. } = &functions[0] else {
            panic!("loop")
        };
        let ExprKind::Lambda { body, .. } = &value.kind else {
            panic!("items")
        };
        let ExprKind::Match(_, arms) = &body.kind else {
            panic!("list match")
        };
        let ExprKind::Block(steps, _) = &arms[1].body.kind else {
            panic!("strict row before recursion")
        };
        let Stmt::Let { value: row, .. } = &steps[0] else {
            panic!("row")
        };
        assert_eq!(callee(row), "$collection.normalize");
        let ExprKind::App { arg, .. } = &row.kind else {
            unreachable!()
        };
        assert_eq!(callee(arg), "$collection.callback");
        let Stmt::Let { value: rest, .. } = &steps[1] else {
            panic!("tail")
        };
        assert_eq!(callee(rest), "$collection.loop");
    }
}
