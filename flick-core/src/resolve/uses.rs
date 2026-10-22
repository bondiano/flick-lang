//! References to top-level names in declarations: every name that is not
//! bound locally by a parameter, pattern, `let` or `scope`.

use super::Namespace;
use super::defs::{binders, is_pattern};
use crate::source::{FileId, Span};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

#[derive(Debug)]
pub(super) struct Reference {
    pub namespace: Namespace,
    /// `M` of `M.name`, `M.N` of `M.N.name`.
    pub qualifier: Option<String>,
    pub name: String,
    pub span: Span,
}

pub(super) fn references(tree: &Node, text: &str, file: FileId) -> Vec<Reference> {
    let mut walker = Walker {
        text,
        file,
        locals: Vec::new(),
        out: Vec::new(),
    };
    walker.walk(tree);
    walker.out
}

struct Walker<'a> {
    text: &'a str,
    file: FileId,
    locals: Vec<&'a str>,
    out: Vec<Reference>,
}

impl<'a> Walker<'a> {
    fn walk(&mut self, node: &Node) {
        let mark = self.locals.len();
        match node.kind {
            // shortcut: names inside `sql { }` are SQL-scoped; resolve them in SQL elaboration
            K::ModuleHeader | K::ImportDecl | K::SqlExpr => return,
            K::NameExpr | K::ConPat => self.reference(node, Namespace::Constructor),
            K::NameType => self.reference(node, Namespace::Type),
            K::LetDecl => {
                self.let_decl(node);
                self.locals.truncate(mark);
                return;
            }
            K::Block => {
                // a `let` scopes over the statements after it
                for child in node.nodes() {
                    if child.kind == K::LetDecl {
                        self.let_decl(child);
                    } else {
                        self.walk(child);
                    }
                }
                self.locals.truncate(mark);
                return;
            }
            K::FunExpr | K::MatchArm => self.bind_patterns(node),
            K::ScopeExpr => {
                let name = node.child_tokens().find(|t| t.kind == K::Ident);
                self.locals.extend(name.map(|t| t.text(self.text)));
            }
            _ => {}
        }
        for child in node.nodes() {
            self.walk(child);
        }
        self.locals.truncate(mark);
    }

    /// Walks a `let`, then leaves the names it binds in scope.
    fn let_decl(&mut self, node: &Node) {
        let mut names = Vec::new();
        if let Some(pattern) = node.nodes().find(|n| is_pattern(n.kind)) {
            binders(pattern, &mut names);
        }
        let names: Vec<&'a str> = names.iter().map(|t| t.text(self.text)).collect();
        let mark = self.locals.len();
        if node.has_token(K::RecKw) {
            self.locals.extend(&names);
        }
        for params in node.nodes().filter(|n| n.kind == K::Params) {
            self.bind_patterns(params);
        }
        for child in node.nodes() {
            self.walk(child);
        }
        self.locals.truncate(mark);
        self.locals.extend(names);
    }

    fn bind_patterns(&mut self, node: &Node) {
        let mut names = Vec::new();
        for pattern in node.nodes().filter(|n| is_pattern(n.kind)) {
            binders(pattern, &mut names);
        }
        self.locals.extend(names.iter().map(|t| t.text(self.text)));
    }

    /// `name`, `Con`, `M.name`, `M.N.Con`; a lowercase type name is a type
    /// variable and refers to nothing.
    fn reference(&mut self, node: &Node, con_namespace: Namespace) {
        let tokens: Vec<_> = node
            .child_tokens()
            .take_while(|t| matches!(t.kind, K::Con | K::Ident | K::Dot))
            .filter(|t| t.kind != K::Dot)
            .collect();
        let (Some(first), Some(last)) = (tokens.first(), tokens.last()) else {
            return;
        };
        let span = Span::new(self.file, first.start, last.end);
        let parts: Vec<&str> = tokens.iter().map(|t| t.text(self.text)).collect();
        let Some((&name, qualifier)) = parts.split_last() else {
            return;
        };
        let namespace = if name.starts_with(|c: char| c.is_ascii_uppercase()) {
            con_namespace
        } else if con_namespace == Namespace::Type {
            return;
        } else {
            Namespace::Value
        };
        if qualifier.is_empty() && self.locals.contains(&name) {
            return;
        }
        self.out.push(Reference {
            namespace,
            qualifier: (!qualifier.is_empty()).then(|| qualifier.join(".")),
            name: name.to_owned(),
            span,
        });
    }
}
