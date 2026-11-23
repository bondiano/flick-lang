//! Folding matches whose scrutinee is a tuple the code builds right there.
//!
//! `match (a, b) with (x, _) -> x` builds a tuple, takes it apart and keeps
//! one field. When the tuple pattern cannot fail — every element is a name,
//! a wildcard or a nested tuple of those — the match is the destructuring of
//! values that are already at hand, so the pass rewrites it into a block of
//! `let`s. The elements are bound in the order the tuple evaluates them, so
//! effects keep their order and none is dropped or repeated.
//!
//! The pass is pure over one [`Module`]: it reads and rewrites the IR only,
//! and leaves syntax, type checking and code generation untouched.

use super::types::filter_binder_types;
use flick_ir::{Expr, ExprKind, ItemKind, Module, Pat, Span, Stmt, TypeRef};
use std::mem;

/// Folds every foldable tuple match of `module`, giving how many it folded.
pub fn fold_tuple_matches(module: &mut Module) -> usize {
    let mut folded = 0;
    for item in &mut module.items {
        match &mut item.kind {
            ItemKind::Value { body, .. } | ItemKind::Destructure { body, .. } => {
                folded += fold(body);
            }
            ItemKind::Instance { methods, .. } => {
                for (_, body) in methods {
                    folded += fold(body);
                }
            }
            ItemKind::Foreign(foreign) => {
                if let Some(catch) = &mut foreign.catch {
                    folded += fold(catch);
                }
            }
            ItemKind::Constructor(_) | ItemKind::Service { .. } => {}
        }
    }
    folded
}

/// Folds the tuple matches of `expr` and of everything inside it, giving
/// how many it folded.
pub fn fold(expr: &mut Expr) -> usize {
    let mut folded = 0;
    expr.each_child_mut(&mut |child| folded += fold(child));
    if fold_here(expr) {
        folded += 1;
    }
    folded
}

/// Rewrites `expr` when it is a match on a tuple the code builds and its
/// first arm cannot fail. Returns whether it rewrote it.
fn fold_here(expr: &mut Expr) -> bool {
    let Some((pattern, binder_types, body)) = foldable(expr) else {
        return false;
    };
    let (span, ty) = (expr.span, expr.ty.clone());
    let ExprKind::Match(scrutinee, _) = mem::replace(&mut expr.kind, ExprKind::Tuple(Vec::new()))
    else {
        unreachable!("a foldable match was found above");
    };
    let scrutinee_ty = scrutinee.ty;
    let ExprKind::Tuple(items) = scrutinee.kind else {
        unreachable!("a tuple scrutinee was found above");
    };
    let mut stmts = Vec::new();
    bind_tuple(
        &pattern,
        &binder_types,
        items,
        span,
        scrutinee_ty,
        &mut stmts,
    );
    *expr = Expr {
        kind: ExprKind::Block(stmts, Box::new(body)),
        span,
        ty,
    };
    true
}

type FoldedArm = (Pat, Vec<(String, TypeRef)>, Expr);

/// The pattern, binder types and body a match on a tuple expression folds
/// to: the first arm, which no value of the tuple can escape and which has
/// no guard.
fn foldable(expr: &Expr) -> Option<FoldedArm> {
    let ExprKind::Match(scrutinee, arms) = &expr.kind else {
        return None;
    };
    let ExprKind::Tuple(_) = &scrutinee.kind else {
        return None;
    };
    let (arm, _) = arms.split_first()?;
    if arm.guard.is_some() || !irrefutable(&arm.pat) {
        return None;
    }
    Some((arm.pat.clone(), arm.binder_types.clone(), arm.body.clone()))
}

/// Whether `pattern` matches every value of its type. Wildcards and names
/// always match; a tuple pattern matches when each of its elements does,
/// since the checker has already given it the scrutinee's tuple type. A
/// literal, a constructor, a list, a record or an or-pattern may fail, so it
/// is not foldable.
fn irrefutable(pattern: &Pat) -> bool {
    match pattern {
        Pat::Wild | Pat::Bind(_) => true,
        Pat::Tuple(items) => items.iter().all(irrefutable),
        Pat::Lit(_) | Pat::List(..) | Pat::Record(_) | Pat::Con(..) | Pat::Or(_) => false,
    }
}

/// Binds the tuple `items` to `pattern`.
fn bind_tuple(
    pattern: &Pat,
    binder_types: &[(String, TypeRef)],
    items: Vec<Expr>,
    span: Span,
    ty: Option<TypeRef>,
    out: &mut Vec<Stmt>,
) {
    if let Pat::Tuple(patterns) = pattern
        && patterns.len() == items.len()
    {
        for (pattern, item) in patterns.iter().zip(items) {
            bind_value(pattern, binder_types, item, out);
        }
        return;
    }
    // a pattern that does not take the tuple apart as a tuple still has to
    // see every element evaluated, in order
    match pattern {
        Pat::Wild => out.extend(items.into_iter().map(Stmt::Expr)),
        _ => out.push(Stmt::let_typed(
            pattern.clone(),
            filter_binder_types(pattern, binder_types),
            Expr {
                kind: ExprKind::Tuple(items),
                span,
                ty,
            },
        )),
    }
}

/// Binds `item` to `pattern`, flattening a nested tuple whose pattern is a
/// tuple too, so that no destructuring of a built tuple is left behind.
fn bind_value(pattern: &Pat, binder_types: &[(String, TypeRef)], item: Expr, out: &mut Vec<Stmt>) {
    match item {
        Expr {
            kind: ExprKind::Tuple(items),
            span,
            ty,
        } => match pattern {
            Pat::Tuple(patterns) if patterns.len() == items.len() => {
                for (pattern, item) in patterns.iter().zip(items) {
                    bind_value(pattern, binder_types, item, out);
                }
            }
            pattern => out.push(Stmt::let_typed(
                pattern.clone(),
                filter_binder_types(pattern, binder_types),
                Expr {
                    kind: ExprKind::Tuple(items),
                    span,
                    ty,
                },
            )),
        },
        item => out.push(Stmt::let_typed(
            pattern.clone(),
            filter_binder_types(pattern, binder_types),
            item,
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{fold, fold_tuple_matches};
    use crate::Known;
    use flick_ir::{Arm, Expr, ExprKind, FileId, Item, ItemKind, Lit, Module, Pat, Span, Stmt};
    use flick_ir::{Exports, Initialization};

    fn local(name: &str) -> Expr {
        Expr::new(
            ExprKind::Local(name.to_owned()),
            Span::new(FileId::default(), 0, 0),
        )
    }

    fn int(value: i32) -> Expr {
        Expr::new(
            ExprKind::Lit(Lit::Int(value)),
            Span::new(FileId::default(), 0, 0),
        )
    }

    fn tuple(items: Vec<Expr>) -> Expr {
        Expr::new(ExprKind::Tuple(items), Span::new(FileId::default(), 0, 0))
    }

    fn arm(pattern: Pat, body: Expr) -> Arm {
        Arm {
            pat: pattern,
            binder_types: Vec::new(),
            guard: None,
            body,
        }
    }

    fn match_on(scrutinee: Expr, arms: Vec<Arm>) -> Expr {
        Expr::new(
            ExprKind::Match(Box::new(scrutinee), arms),
            Span::new(FileId::default(), 0, 0),
        )
    }

    fn module_with(bodies: Vec<Expr>) -> Module {
        Module {
            name: "App".to_owned(),
            types: Vec::new(),
            items: (bodies.into_iter().enumerate())
                .map(|(i, body)| Item {
                    kind: ItemKind::Value {
                        name: format!("v{i}"),
                        type_params: Vec::new(),
                        dicts: Vec::new(),
                        dict_constraints: Vec::new(),
                        ty: None,
                        body,
                    },
                    span: Span::new(FileId::default(), 0, 0),
                })
                .collect(),
            labels: BTreeMap::new(),
            initialization: Initialization::default(),
            records: BTreeMap::new(),
            package: None,
            exports: Exports::default(),
        }
    }

    fn block(expr: &Expr) -> (&[Stmt], &Expr) {
        match &expr.kind {
            ExprKind::Block(stmts, body) => (stmts, body),
            other => panic!("expected a block, found {other:?}"),
        }
    }

    fn let_pattern(stmt: &Stmt) -> (&Pat, &Expr) {
        match stmt {
            Stmt::Let { pat, value, .. } => (pat, value),
            other => panic!("expected a let, found {other:?}"),
        }
    }

    #[test]
    fn a_partial_tuple_match_folds_into_one_binding_per_element_in_order() {
        let mut expr = match_on(
            tuple(vec![local("a"), local("b"), local("c")]),
            vec![arm(
                Pat::Tuple(vec![
                    Pat::Bind("x".into()),
                    Pat::Wild,
                    Pat::Bind("z".into()),
                ]),
                local("x"),
            )],
        );

        assert_eq!(fold(&mut expr), 1);

        let (stmts, body) = block(&expr);
        assert_eq!(stmts.len(), 3, "each element is evaluated once: {stmts:?}");
        let (pattern, value) = let_pattern(&stmts[0]);
        assert_eq!((pattern, value), (&Pat::Bind("x".into()), &local("a")));
        let (pattern, value) = let_pattern(&stmts[1]);
        assert_eq!((pattern, value), (&Pat::Wild, &local("b")));
        let (pattern, value) = let_pattern(&stmts[2]);
        assert_eq!((pattern, value), (&Pat::Bind("z".into()), &local("c")));
        assert_eq!(body, &local("x"), "the first arm's body is the result");
    }

    #[test]
    fn a_refutable_element_keeps_the_match() {
        let original = match_on(
            tuple(vec![local("a"), local("b")]),
            vec![arm(
                Pat::Tuple(vec![Pat::Lit(Lit::Int(0)), Pat::Bind("y".into())]),
                local("y"),
            )],
        );
        let mut expr = original.clone();

        assert_eq!(fold(&mut expr), 0);
        assert_eq!(expr, original);
    }

    #[test]
    fn another_refutable_pattern_keeps_the_match() {
        for pattern in [
            Pat::List(Vec::new(), None),
            Pat::Record(Vec::new()),
            Pat::Or(vec![Pat::Bind("x".into()), Pat::Wild]),
        ] {
            let original = match_on(
                tuple(vec![local("a"), local("b")]),
                vec![arm(pattern, local("a"))],
            );
            let mut expr = original.clone();
            assert_eq!(fold(&mut expr), 0);
            assert_eq!(expr, original);
        }
    }

    #[test]
    fn a_guard_keeps_the_match() {
        let mut guarded = arm(
            Pat::Tuple(vec![Pat::Bind("x".into()), Pat::Bind("y".into())]),
            local("x"),
        );
        guarded.guard = Some(local("g"));
        let original = match_on(tuple(vec![local("a"), local("b")]), vec![guarded]);
        let mut expr = original.clone();

        assert_eq!(fold(&mut expr), 0);
        assert_eq!(expr, original);
    }

    #[test]
    fn a_scrutinee_the_code_does_not_build_keeps_the_match() {
        let original = match_on(
            local("pair"),
            vec![arm(
                Pat::Tuple(vec![Pat::Bind("x".into()), Pat::Bind("y".into())]),
                local("x"),
            )],
        );
        let mut expr = original.clone();

        assert_eq!(fold(&mut expr), 0);
        assert_eq!(
            expr, original,
            "destructuring an existing tuple still needs the match"
        );
    }

    #[test]
    fn a_nested_tuple_pattern_folds_through_both_levels() {
        let mut expr = match_on(
            tuple(vec![local("a"), tuple(vec![local("b"), local("c")])]),
            vec![arm(
                Pat::Tuple(vec![
                    Pat::Bind("x".into()),
                    Pat::Tuple(vec![Pat::Bind("y".into()), Pat::Bind("z".into())]),
                ]),
                local("y"),
            )],
        );

        assert_eq!(fold(&mut expr), 1);

        let (stmts, body) = block(&expr);
        assert_eq!(stmts.len(), 3);
        let values: Vec<&Expr> = stmts.iter().map(|stmt| let_pattern(stmt).1).collect();
        assert_eq!(values, vec![&local("a"), &local("b"), &local("c")]);
        assert_eq!(body, &local("y"));
    }

    #[test]
    fn a_nested_tuple_of_another_arity_still_folds_at_its_own_level() {
        let mut expr = match_on(
            tuple(vec![
                local("a"),
                tuple(vec![local("b"), local("c")]),
                local("d"),
            ]),
            vec![arm(
                Pat::Tuple(vec![
                    Pat::Wild,
                    Pat::Tuple(vec![Pat::Bind("y".into()), Pat::Bind("z".into())]),
                    Pat::Bind("w".into()),
                ]),
                local("z"),
            )],
        );

        assert_eq!(fold(&mut expr), 1);

        let (stmts, body) = block(&expr);
        assert_eq!(stmts.len(), 4, "every element is evaluated once: {stmts:?}");
        let values: Vec<&Expr> = stmts.iter().map(|stmt| let_pattern(stmt).1).collect();
        assert_eq!(
            values,
            vec![&local("a"), &local("b"), &local("c"), &local("d")]
        );
        assert_eq!(body, &local("z"));
    }

    #[test]
    fn a_wildcard_pattern_evaluates_every_element_without_building_the_tuple() {
        let mut expr = match_on(
            tuple(vec![local("a"), int(2)]),
            vec![arm(Pat::Wild, int(0))],
        );

        assert_eq!(fold(&mut expr), 1);

        let (stmts, body) = block(&expr);
        assert_eq!(stmts.len(), 2);
        assert!(matches!(&stmts[0], Stmt::Expr(value) if value == &local("a")));
        assert!(matches!(&stmts[1], Stmt::Expr(value) if value == &int(2)));
        assert_eq!(body, &int(0));
    }

    #[test]
    fn arms_behind_the_irrefutable_one_are_unreachable_and_dropped() {
        let mut expr = match_on(
            tuple(vec![local("a"), local("b")]),
            vec![
                arm(
                    Pat::Tuple(vec![Pat::Bind("x".into()), Pat::Wild]),
                    local("x"),
                ),
                arm(Pat::Wild, int(0)),
            ],
        );

        assert_eq!(fold(&mut expr), 1);

        let (_, body) = block(&expr);
        assert_eq!(body, &local("x"));
    }

    #[test]
    fn a_second_fold_changes_nothing() {
        let mut expr = match_on(
            tuple(vec![local("a"), local("b")]),
            vec![arm(
                Pat::Tuple(vec![Pat::Bind("x".into()), Pat::Bind("y".into())]),
                local("x"),
            )],
        );

        assert_eq!(fold(&mut expr), 1);
        assert_eq!(fold(&mut expr), 0, "the pass is idempotent");
    }

    #[test]
    fn the_optimizer_pipeline_folds_the_match() {
        let mut module = module_with(vec![match_on(
            tuple(vec![local("a"), local("b")]),
            vec![arm(
                Pat::Tuple(vec![Pat::Bind("x".into()), Pat::Wild]),
                local("x"),
            )],
        )]);

        fold_tuple_matches(&mut module);
        super::super::optimize(
            &mut module,
            &|name| name == "v0",
            &|_, _| None,
            &|_, _| None,
            &Known::of(&[]),
            &|_| false,
        );

        // the pass turned the match into bindings the other passes then
        // simplify to the element it selected; a match the pass skipped
        // would still be here
        let ItemKind::Value { body, .. } = &module.items[0].kind else {
            panic!("the module's value");
        };
        assert_eq!(
            body,
            &local("a"),
            "the portable passes simplify what the pass folded"
        );
    }

    #[test]
    fn every_item_body_is_folded() {
        let foldable = || {
            match_on(
                tuple(vec![local("a"), local("b")]),
                vec![arm(
                    Pat::Tuple(vec![Pat::Bind("x".into()), Pat::Wild]),
                    local("x"),
                )],
            )
        };
        let mut module = module_with(vec![
            foldable(),
            Expr::new(
                ExprKind::Block(vec![], Box::new(foldable())),
                Span::new(FileId::default(), 0, 0),
            ),
        ]);

        assert_eq!(fold_tuple_matches(&mut module), 2);
        assert_eq!(fold_tuple_matches(&mut module), 0);
    }
}
