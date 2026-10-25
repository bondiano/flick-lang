//! Where a local name is bound: the nearest parameter, pattern or `let`
//! before its use in the scopes around it.

use super::tree::path_to;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::is_pattern;

/// The variable patterns of `pattern`.
pub fn binders(pattern: &Node, out: &mut Vec<(String, (u32, u32))>, text: &str) {
    if pattern.kind == K::VarPat {
        if let Some(t) = pattern.child_tokens().find(|t| t.kind == K::Ident) {
            out.push((t.text(text).to_owned(), (t.start, t.end)));
        }
        return;
    }
    for child in pattern.nodes() {
        binders(child, out, text);
    }
}

/// The names a `let` binds for the statements after it.
fn let_binders(decl: &Node, text: &str) -> Vec<(String, (u32, u32))> {
    let mut out = Vec::new();
    if let Some(first) = decl.nodes().next().filter(|n| is_pattern(n.kind)) {
        binders(first, &mut out, text);
    }
    out
}

/// Every name in scope at `offset`, innermost last: what completion offers.
pub fn in_scope(root: &Node, offset: u32, text: &str) -> Vec<(String, (u32, u32))> {
    let path = path_to(root, offset);
    let mut out = Vec::new();
    for pair in path.windows(2) {
        let (scope, child) = (pair[0], pair[1]);
        let before = |n: &&Node| n.range().1 <= child.range().0;
        match scope.kind {
            K::Block => {
                for statement in scope.nodes().filter(before) {
                    if matches!(statement.kind, K::LetDecl | K::BindStmt) {
                        out.extend(let_binders(statement, text));
                    }
                }
            }
            K::LetDecl => {
                // the name, then the parameters of `let f x y =` under their `Params` node
                let patterns: Vec<&Node> = scope
                    .nodes()
                    .flat_map(|n| {
                        if n.kind == K::Params {
                            n.nodes().collect()
                        } else {
                            vec![n]
                        }
                    })
                    .filter(|n| is_pattern(n.kind))
                    .collect();
                let own = patterns.first().is_some_and(|p| p.range() == child.range());
                if !own {
                    // a recursive local sees itself; its parameters follow its name
                    if scope.has_token(K::RecKw) {
                        out.extend(let_binders(scope, text));
                    }
                    for p in patterns.iter().skip(1).filter(|p| before(p)) {
                        binders(p, &mut out, text);
                    }
                }
            }
            K::FunExpr | K::MatchArm => {
                for p in scope.nodes().filter(|n| is_pattern(n.kind)).filter(before) {
                    binders(p, &mut out, text);
                }
            }
            _ => {}
        }
    }
    out
}

/// The binder of the local `name` used at `offset`.
pub fn binder(root: &Node, offset: u32, name: &str, text: &str) -> Option<(u32, u32)> {
    in_scope(root, offset, text)
        .into_iter()
        .rev()
        .find(|(n, _)| n == name)
        .map(|(_, r)| r)
}
