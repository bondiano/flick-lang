//! Calls in parentheses that the lines under a call may hold without
//! them, one a line, where those lines are the call's arguments.

use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

/// The call inside the parentheses `n`, when it may stand without them on
/// a line under a call: a call of a name. A line that starts with `-` goes
/// on with the line before it, and `if`, `match` or `fun` take the lines
/// after it.
pub(super) fn child(n: &Node) -> Option<&Node> {
    let mut inner = n.nodes();
    let (Some(call), None) = (inner.next(), inner.next()) else {
        return None;
    };
    let named = call.nodes().next().is_some_and(|h| h.kind == K::NameExpr);
    (n.kind == K::ParenExpr && call.kind == K::AppExpr && named).then_some(call)
}

/// How many of the last arguments of a call may go on lines of their own
/// under it: calls in parentheses, with no comment among them.
pub(super) fn children(n: &Node, head: &Node, args: &[&Node]) -> usize {
    let count = args.iter().rev().take_while(|a| child(a).is_some()).count();
    let Some(last) = args.last().filter(|_| count > 0) else {
        return 0;
    };
    let from = args.len() - count;
    let start = from.checked_sub(1).map_or(head, |i| args[i]).range().1;
    if commented(n, start, last.range().1) {
        0
    } else {
        count
    }
}

/// Whether a comment under `n` starts between `start` and `end`.
pub(super) fn commented(n: &Node, start: u32, end: u32) -> bool {
    (n.tokens().iter()).any(|t| t.kind.is_comment() && (start..end).contains(&t.start))
}
