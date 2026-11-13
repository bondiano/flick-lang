//! A list a tail-recursive loop walks: the loop keeps the array and an
//! offset into it, so passing `rest` of `[x, ...rest]` on to the next
//! iteration moves the offset instead of copying the array.

use std::mem;

use super::Gen;
use crate::codegen::tail::{Calls, Target, has_tail_call, self_call};
use crate::ir::{Arm, BinOp, Expr, ExprKind, Param, Pat};

/// The loop positions, up to the largest arity, that hold a walked list:
/// in every member that has the position, its parameter is a name used only
/// as the subject of matches in tail position, and the `...rest` those bind
/// only as the same position of a jump.
pub(super) fn positions(bodies: &[(&[Param], &Expr)], members: &[(Target, usize)]) -> Vec<bool> {
    let arity = bodies.iter().map(|(p, _)| p.len()).max().unwrap_or(0);
    (0..arity)
        .map(|i| {
            bodies
                .iter()
                .filter(|(params, _)| params.len() > i)
                .all(|(params, body)| walked(params, body, i, members))
        })
        .collect()
}

fn walked(params: &[Param], body: &Expr, i: usize, members: &[(Target, usize)]) -> bool {
    let Pat::Bind(list) = &params[i].pat else {
        return false;
    };
    let mut matches = Vec::new();
    let mut leaves = Vec::new();
    spine(body, members, &mut matches, &mut leaves);
    let mut subjects = 0;
    let mut rests = Vec::new();
    for (subject, arms) in matches {
        if !matches!(&subject.kind, ExprKind::Local(n) if n == list) {
            continue;
        }
        subjects += 1;
        for arm in arms {
            match &arm.pat {
                Pat::List(_, Some(rest)) if matches!(&**rest, Pat::Bind(_)) => {
                    if let Pat::Bind(r) = &**rest {
                        rests.push(r);
                    }
                }
                pat if fits(pat) => {}
                _ => return false,
            }
        }
    }
    if subjects == 0 || body.uses(list) != subjects {
        return false;
    }
    rests.iter().all(|r| {
        let jumps = leaves
            .iter()
            .filter_map(|leaf| self_call(leaf, members, Calls::Pure))
            .filter(|(_, args)| matches!(args.get(i).map(|a| &a.kind), Some(ExprKind::Local(n)) if n == *r))
            .count();
        body.uses(r) == jumps
    })
}

impl Gen<'_> {
    /// Runs `f` with the walked lists among `heads` in scope.
    pub(super) fn walk(
        &mut self,
        heads: &[String],
        offsets: &[Option<String>],
        f: impl FnOnce(&mut Self),
    ) {
        let saved = (self.views.clone(), mem::take(&mut self.rests));
        for (i, (head, offset)) in heads.iter().zip(offsets).enumerate() {
            if let Some(offset) = offset {
                self.views.insert(head.clone(), (offset.clone(), i));
            }
        }
        f(self);
        (self.views, self.rests) = saved;
    }
}

/// Whether a pattern of the walked list binds no `rest` but at its top.
fn fits(pat: &Pat) -> bool {
    match pat {
        Pat::Wild => true,
        Pat::List(_, rest) => rest.as_deref().is_none_or(|r| matches!(r, Pat::Wild)),
        Pat::Or(alternatives) => alternatives.iter().all(fits),
        _ => false,
    }
}

/// The matches and the other expressions in tail position of `body`, as
/// `tail_expr` emits them: the right operand of `&&` and `||` only when it
/// jumps.
fn spine<'e>(
    body: &'e Expr,
    members: &[(Target, usize)],
    matches: &mut Vec<(&'e Expr, &'e [Arm])>,
    leaves: &mut Vec<&'e Expr>,
) {
    let mut pending = vec![body];
    while let Some(e) = pending.pop() {
        match &e.kind {
            ExprKind::Block(_, result) => pending.push(result),
            ExprKind::If(_, t, f) => pending.extend([&**t, &**f]),
            ExprKind::Match(s, arms) => {
                matches.push((&**s, &arms[..]));
                pending.extend(arms.iter().map(|a| &a.body));
            }
            ExprKind::Binary(BinOp::And | BinOp::Or, _, r)
                if has_tail_call(r, members, Calls::Pure) =>
            {
                pending.push(r);
            }
            _ => leaves.push(e),
        }
    }
}
