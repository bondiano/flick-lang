use super::*;
use flick_ir::{FileId, Span};

fn walk_assigns(
    e: &Expr,
    found_inner: &mut bool,
    found_inner_step: &mut bool,
    found_outer: &mut bool,
    found_outer_step: &mut bool,
) {
    if let ExprKind::Block(stmts, _) = &e.kind {
        for stmt in stmts {
            if let Stmt::Assign(name, _) = stmt {
                if name == "inner" {
                    *found_inner = true;
                }
                if name == "inner$step" {
                    *found_inner_step = true;
                }
                if name == "outer" {
                    *found_outer = true;
                }
                if name == "outer$step" {
                    *found_outer_step = true;
                }
            }
        }
    }
    e.each_child(&mut |c| {
        walk_assigns(
            c,
            found_inner,
            found_inner_step,
            found_outer,
            found_outer_step,
        );
    });
}

fn find_exit_block(e: &Expr, out: &mut Vec<String>) {
    if let ExprKind::Block(stmts, _) = &e.kind {
        let has_inner_step = stmts
            .iter()
            .any(|s| matches!(s, Stmt::Assign(n, _) if n == "inner$step"));
        if has_inner_step {
            for s in stmts {
                if let Stmt::Assign(n, _) = s {
                    out.push(n.clone());
                }
            }
            return;
        }
    }
    e.each_child(&mut |c| find_exit_block(c, out));
}

#[test]
fn multi_level_exit_clears_intervening_and_target_flags() {
    let span = Span::new(FileId::new(0), 0, 0);
    let known = Known::of(&[]);
    let inner_exit = Expr::new(
        ExprKind::LoopExit {
            id: "outer".into(),
            value: Some(Box::new(boolean(true, span))),
        },
        span,
    );
    let inner_loop = Expr::new(
        ExprKind::ControlLoop {
            id: "inner".into(),
            condition: None,
            body: Box::new(inner_exit),
            iterable: false,
        },
        span,
    );
    let lowered = lower(&known, "outer", None, &inner_loop, false, span);

    let mut found_inner = false;
    let mut found_inner_step = false;
    let mut found_outer = false;
    let mut found_outer_step = false;

    walk_assigns(
        &lowered,
        &mut found_inner,
        &mut found_inner_step,
        &mut found_outer,
        &mut found_outer_step,
    );

    assert!(found_inner, "inner loop flag must be cleared");
    assert!(found_inner_step, "inner loop step flag must be cleared");
    assert!(found_outer, "outer loop flag must be cleared");
    assert!(found_outer_step, "outer loop step flag must be cleared");
}

#[test]
fn multi_level_continue_clears_intervening_loop_but_retains_target_flag() {
    let span = Span::new(FileId::new(0), 0, 0);
    let known = Known::of(&[]);
    let inner_continue = Expr::new(
        ExprKind::LoopExit {
            id: "outer".into(),
            value: None,
        },
        span,
    );
    let inner_loop = Expr::new(
        ExprKind::ControlLoop {
            id: "inner".into(),
            condition: None,
            body: Box::new(inner_continue),
            iterable: false,
        },
        span,
    );
    let lowered = lower(&known, "outer", None, &inner_loop, false, span);

    let mut exit_block_assigns = Vec::new();
    find_exit_block(&lowered, &mut exit_block_assigns);

    assert!(
        exit_block_assigns.contains(&"inner".to_string()),
        "exit block must clear inner loop: {exit_block_assigns:?}"
    );
    assert!(
        exit_block_assigns.contains(&"inner$step".to_string()),
        "exit block must clear inner$step: {exit_block_assigns:?}"
    );
    assert!(
        !exit_block_assigns.contains(&"outer".to_string()),
        "exit block for continue must NOT clear outer: {exit_block_assigns:?}"
    );
    assert!(
        exit_block_assigns.contains(&"outer$step".to_string()),
        "exit block must clear outer$step: {exit_block_assigns:?}"
    );
}
