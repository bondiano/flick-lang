//! Checks on the finished tree that need more than one token of context:
//! the names a pattern binds. Like Rust, every alternative of an or-pattern
//! binds the same names, and no name is bound twice by one pattern or by the
//! parameters of one function.

use super::cst::{Node, Token};
use super::kind::SyntaxKind as K;
use crate::diagnostics::{Diagnostic, codes};
use crate::source::{FileId, Span};

pub fn check(tree: &Node, src: &str, file: FileId) -> Vec<Diagnostic> {
    let mut v = Validator {
        src,
        file,
        errors: Vec::new(),
    };
    v.walk(tree);
    v.errors
}

struct Validator<'a> {
    src: &'a str,
    file: FileId,
    errors: Vec<Diagnostic>,
}

fn is_pattern(kind: K) -> bool {
    (K::WildcardPat..=K::OrPat).contains(&kind)
}

impl Validator<'_> {
    fn walk(&mut self, node: &Node) {
        match node.kind {
            K::MatchArm => {
                if let Some(pattern) = node.nodes().find(|n| is_pattern(n.kind)) {
                    self.group(&[pattern]);
                }
            }
            K::FunExpr | K::Params => {
                let params: Vec<&Node> = node.nodes().filter(|n| is_pattern(n.kind)).collect();
                self.group(&params);
            }
            // `let name params`: the name is not a pattern binding next to its params
            K::LetDecl if !node.nodes().any(|n| n.kind == K::Params) => {
                if let Some(pattern) = node.nodes().find(|n| is_pattern(n.kind)) {
                    self.group(&[pattern]);
                }
            }
            _ => {}
        }
        for child in node.nodes() {
            self.walk(child);
        }
    }

    /// Patterns whose bindings share one scope.
    fn group(&mut self, patterns: &[&Node]) {
        let mut names: Vec<Token> = Vec::new();
        for pattern in patterns {
            for name in self.bindings(pattern) {
                let text = name.text(self.src);
                if names.iter().any(|n| n.text(self.src) == text) {
                    self.error(
                        name.start,
                        name.end,
                        format!("`{text}` is bound more than once in the same pattern"),
                    );
                } else {
                    names.push(name);
                }
            }
        }
    }

    /// Names bound by a pattern, in source order.
    fn bindings(&mut self, node: &Node) -> Vec<Token> {
        match node.kind {
            K::VarPat => idents(node).take(1).collect(),
            // `{ field }` binds `field`; `{ field = p }` binds what `p` binds
            K::RecordPatField if !node.has_token(K::Eq) => idents(node).take(1).collect(),
            K::OrPat => self.alternatives(node),
            _ => node
                .nodes()
                .filter(|n| is_pattern(n.kind) || n.kind == K::RecordPatField)
                .flat_map(|n| self.bindings(n))
                .collect(),
        }
    }

    fn alternatives(&mut self, node: &Node) -> Vec<Token> {
        let alternatives: Vec<&Node> = node.nodes().collect();
        let sets: Vec<Vec<Token>> = alternatives.iter().map(|a| self.bindings(a)).collect();
        let first = sets.first().cloned().unwrap_or_default();
        for (alternative, set) in alternatives.iter().zip(&sets).skip(1) {
            let missing: Vec<&str> = sets
                .iter()
                .flatten()
                .map(|t| t.text(self.src))
                .filter(|name| !set.iter().any(|t| t.text(self.src) == *name))
                .collect();
            let extra = set
                .iter()
                .map(|t| t.text(self.src))
                .find(|name| !first.iter().any(|t| t.text(self.src) == *name));
            if let Some(name) = missing.first().copied().or(extra) {
                let (start, end) = alternative.range();
                self.error(
                    start,
                    end,
                    format!(
                        "every alternative of an or-pattern must bind the same names; \
                         `{name}` is not bound by all of them"
                    ),
                );
            }
        }
        first
    }

    fn error(&mut self, start: u32, end: u32, message: String) {
        self.errors.push(Diagnostic::error(
            codes::S012,
            message,
            Span::new(self.file, start, end),
        ));
    }
}

fn idents(node: &Node) -> impl Iterator<Item = Token> + '_ {
    node.child_tokens().filter(|t| t.kind == K::Ident)
}
