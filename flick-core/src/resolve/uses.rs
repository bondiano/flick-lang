//! References to top-level names in declarations: every name that is not
//! bound locally by a parameter, pattern, `let` or `scope`; and the local
//! names nothing refers to.

use super::Namespace;
use super::defs::{binders, is_pattern};
use crate::source::{FileId, Span};
use crate::syntax::cst::{Node, Token};
use crate::syntax::kind::SyntaxKind as K;

#[derive(Debug)]
pub(super) struct Reference {
    pub namespace: Namespace,
    /// `M` of `M.name`, `M.N` of `M.N.name`.
    pub qualifier: Option<String>,
    pub name: String,
    pub span: Span,
    /// Index of the top-level declaration it is in.
    pub top: usize,
}

/// A local name bound and never used.
#[derive(Debug)]
pub(super) struct Unused {
    pub name: String,
    pub span: Span,
    /// Bound by a record pattern field without `=`: `{ name }`.
    pub punned: bool,
}

pub(super) fn references(tree: &Node, text: &str, file: FileId) -> (Vec<Reference>, Vec<Unused>) {
    let mut walker = Walker {
        text,
        file,
        locals: Vec::new(),
        out: Vec::new(),
        unused: Vec::new(),
        top: 0,
    };
    for (i, decl) in tree.nodes().enumerate() {
        walker.top = i;
        walker.walk(decl);
    }
    (walker.out, walker.unused)
}

struct Local<'a> {
    name: &'a str,
    /// `None` for a name whose uses are not counted: a top-level `let`, a
    /// region of `scope`.
    binder: Option<(Token, bool)>,
    used: bool,
}

struct Walker<'a> {
    text: &'a str,
    file: FileId,
    locals: Vec<Local<'a>>,
    out: Vec<Reference>,
    unused: Vec<Unused>,
    top: usize,
}

impl Walker<'_> {
    fn walk(&mut self, node: &Node) {
        let mark = self.locals.len();
        match node.kind {
            K::ModuleHeader | K::ImportDecl => return,
            // names inside `sql { }` are SQL-scoped: only captures and tables refer to the program
            K::SqlExpr => {
                self.sql_names(node);
                return;
            }
            K::NameExpr | K::ConPat => self.reference(node, Namespace::Constructor),
            K::NameType => self.reference(node, Namespace::Type),
            K::LetDecl => {
                // a top-level or member `let`: its names are not locals
                self.let_decl(node);
                self.locals.truncate(mark);
                return;
            }
            K::Block => {
                self.statements(node);
                self.release(mark);
                return;
            }
            // the props, then the names of `setup` scope over `render`
            K::ComponentExpr => {
                self.bind_patterns(node);
                for block in node.nodes().filter(|n| n.kind == K::Block) {
                    self.statements(block);
                }
                self.release(mark);
                return;
            }
            K::ForExpr => {
                // the names of the clauses scope over the clauses after them and the element
                let mut parts = node.nodes();
                for clause in parts.next().into_iter().flat_map(Node::nodes) {
                    if clause.kind == K::LetDecl {
                        self.let_decl(clause);
                        continue;
                    }
                    for child in clause.nodes() {
                        self.walk(child);
                    }
                    if clause.kind == K::BindStmt {
                        self.bind_patterns(clause);
                    }
                }
                for rest in parts {
                    self.walk(rest);
                }
                self.release(mark);
                return;
            }
            K::FunExpr | K::MatchArm => self.bind_patterns(node),
            K::ScopeExpr => {
                let name = node.child_tokens().find(|t| t.kind == K::Ident);
                self.locals.extend(name.map(|t| Local {
                    name: t.text(self.text),
                    binder: None,
                    used: false,
                }));
            }
            _ => {}
        }
        for child in node.nodes() {
            self.walk(child);
        }
        self.release(mark);
    }

    /// The statements of a block: a `let` scopes over those after it.
    fn statements(&mut self, node: &Node) {
        for child in node.nodes() {
            if child.kind == K::LetDecl {
                self.let_decl(child);
            } else if child.kind == K::BindStmt {
                // `p <- e` of a `do` block: `e` cannot see `p`
                for part in child.nodes() {
                    self.walk(part);
                }
                self.bind_patterns(child);
            } else if child.kind == K::BindGroup {
                // `p <- a and q <- b`: neither value sees `p` or `q`
                for part in child.nodes().flat_map(Node::nodes) {
                    self.walk(part);
                }
                for bind in child.nodes() {
                    self.bind_patterns(bind);
                }
            } else {
                self.walk(child);
            }
        }
    }

    /// Leaves the scope of the locals above `mark`, noting those never used.
    fn release(&mut self, mark: usize) {
        for local in self.locals.drain(mark..) {
            if let (Some((token, punned)), false) = (local.binder, local.used)
                && !local.name.starts_with('_')
            {
                self.unused.push(Unused {
                    name: local.name.to_owned(),
                    span: Span::new(self.file, token.start, token.end),
                    punned,
                });
            }
        }
    }

    fn bind(&mut self, pattern: &Node) {
        let mut names = Vec::new();
        binders(pattern, &mut names);
        let mut puns = Vec::new();
        punned(pattern, &mut puns);
        self.locals.extend(names.into_iter().map(|t| Local {
            name: t.text(self.text),
            binder: Some((t, puns.contains(&t.start))),
            used: false,
        }));
    }

    /// The names of a `sql { … }` block that are names of the program: its
    /// `$name` captures, and its tables, `T` or `M.T` (columns and aliases are
    /// SQL's).
    fn sql_names(&mut self, node: &Node) {
        for child in node.nodes() {
            if child.kind == K::SqlCapture {
                self.walk(child);
            } else if child.kind == K::NameExpr && Self::is_table(child) {
                self.reference(child, Namespace::Type);
            } else {
                self.sql_names(child);
            }
        }
    }

    /// A capitalized name, maybe qualified: what a table is in SQL.
    fn is_table(node: &Node) -> bool {
        let tokens: Vec<_> = node.child_tokens().collect();
        tokens.last().is_some_and(|t| t.kind == K::Con)
            && tokens.iter().all(|t| matches!(t.kind, K::Con | K::Dot))
    }

    /// Walks a `let`, then leaves the names it binds in scope.
    fn let_decl(&mut self, node: &Node) {
        let pattern = node.nodes().find(|n| is_pattern(n.kind));
        let mark = self.locals.len();
        if node.has_token(K::RecKw)
            && let Some(pattern) = pattern
        {
            // a recursive function's own calls do not count as uses
            let mut names = Vec::new();
            binders(pattern, &mut names);
            self.locals.extend(names.into_iter().map(|t| Local {
                name: t.text(self.text),
                binder: None,
                used: false,
            }));
        }
        for params in node.nodes().filter(|n| n.kind == K::Params) {
            self.bind_patterns(params);
        }
        for child in node.nodes() {
            self.walk(child);
        }
        self.release(mark);
        if let Some(pattern) = pattern {
            self.bind(pattern);
        }
    }

    fn bind_patterns(&mut self, node: &Node) {
        for pattern in node.nodes().filter(|n| is_pattern(n.kind)) {
            self.bind(pattern);
        }
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
        if qualifier.is_empty() {
            // every binder of the name in scope counts as used: the branches
            // of an or-pattern bind it once each
            let mut found = false;
            for local in self.locals.iter_mut().filter(|l| l.name == name) {
                local.used = true;
                found = true;
            }
            if found {
                return;
            }
        }
        self.out.push(Reference {
            namespace,
            qualifier: (!qualifier.is_empty()).then(|| qualifier.join(".")),
            name: name.to_owned(),
            span,
            top: self.top,
        });
    }
}

/// Starts of the names bound by record pattern fields without `=`.
fn punned(pattern: &Node, out: &mut Vec<u32>) {
    if pattern.kind == K::RecordPatField && !pattern.has_token(K::Eq) {
        out.extend(
            pattern
                .child_tokens()
                .find(|t| t.kind == K::Ident)
                .map(|t| t.start),
        );
    }
    for child in pattern.nodes().filter(|n| is_pattern(n.kind)) {
        punned(child, out);
    }
}
