//! References to top-level names in declarations: every name that is not
//! bound locally by a parameter, pattern, `let` or `scope`; and the local
//! names nothing refers to.

use super::defs::{binders, is_pattern};
use super::{Module, Namespace};
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

pub(super) fn references(module: &Module) -> (Vec<Reference>, Vec<Unused>) {
    let mut walker = Walker {
        text: module.text,
        file: module.file,
        generated: module.generated,
        locals: Vec::new(),
        module_locals: Vec::new(),
        out: Vec::new(),
        unused: Vec::new(),
        used: Vec::new(),
        top: 0,
    };
    for (i, decl) in module.tree.nodes().enumerate() {
        walker.top = i;
        walker.walk(decl);
    }
    // a pattern a block repeats binds its names once a copy: used if any copy is
    let Walker {
        out,
        mut unused,
        used,
        ..
    } = walker;
    let mut seen = Vec::new();
    unused.retain(|u| {
        let first = !seen.contains(&u.span);
        seen.push(u.span);
        first && !used.contains(&u.span.start)
    });
    (out, unused)
}

struct Local<'a> {
    name: &'a str,
    /// Whether a plugin generated it: only generated names see it.
    generated: bool,
    /// `None` for a name whose uses are not counted: a top-level `let`, a
    /// region of `scope`.
    binder: Option<(Token, bool)>,
    used: bool,
}

struct Walker<'a> {
    text: &'a str,
    file: FileId,
    generated: u32,
    locals: Vec<Local<'a>>,
    module_locals: Vec<&'a str>,
    out: Vec<Reference>,
    unused: Vec<Unused>,
    /// Where each binder that was used is written.
    used: Vec<u32>,
    top: usize,
}

impl Walker<'_> {
    fn walk(&mut self, node: &Node) {
        let mark = self.locals.len();
        match node.kind {
            K::ModuleSignatureArrow => {
                self.module_signature(node);
                return;
            }
            K::ModuleFunctorExpr | K::InlineModuleDecl
                if node.kind == K::ModuleFunctorExpr
                    || node.nodes().any(|n| n.kind == K::ModuleParamGroup) =>
            {
                self.module_functor(node);
                return;
            }
            K::ImportDecl => return,
            K::ModuleHeader => {
                for child in node
                    .nodes()
                    .filter(|n| matches!(n.kind, K::ModuleParamGroup | K::ModuleResult))
                {
                    self.walk(child);
                }
                return;
            }
            K::NameExpr | K::ConPat => self.reference(node, Namespace::Constructor),
            K::NameType => self.reference(node, Namespace::Type),
            K::DerivingClause => {
                self.deriving(node);
                return;
            }
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
            K::UnpackExpr => {
                let mut children = node.nodes();
                if let Some(source) = children.next() {
                    self.walk(source);
                }
                let mut tokens = node.child_tokens();
                let payload = tokens
                    .find(|t| t.kind == K::Comma)
                    .and_then(|_| tokens.find(|t| t.kind == K::Ident));
                if let Some(payload) = payload {
                    self.locals.push(Local {
                        name: payload.text(self.text),
                        generated: payload.start >= self.generated,
                        binder: Some((payload, false)),
                        used: false,
                    });
                }
                for child in children {
                    self.walk(child);
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
                    generated: t.start >= self.generated,
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

    fn module_functor(&mut self, node: &Node) {
        let mark = self.locals.len();
        let modules = self.module_locals.len();
        for parameter in node
            .nodes()
            .filter(|n| n.kind == K::ModuleParamGroup)
            .flat_map(Node::nodes)
        {
            for signature in parameter.nodes() {
                self.walk(signature);
            }
            if let Some(name) = parameter.child_tokens().find(|t| t.kind == K::Con) {
                self.module_locals.push(name.text(self.text));
            }
        }
        for result in node.nodes().filter(|n| n.kind == K::ModuleResult) {
            self.walk(result);
        }
        if let Some(body) = node.nodes().find(|n| {
            matches!(
                n.kind,
                K::Members
                    | K::ModuleApplyExpr
                    | K::ModuleFunctorExpr
                    | K::ModuleParenExpr
                    | K::ModuleAnnotExpr
            ) || (node.kind == K::ModuleFunctorExpr && n.kind == K::ModulePath)
        }) {
            for declaration in body.nodes().filter(|_| body.kind == K::Members) {
                if declaration.kind == K::ImportDecl {
                    let qualifier = declaration
                        .child_tokens()
                        .find(|t| t.kind == K::Con)
                        .or_else(|| {
                            declaration
                                .nodes()
                                .find(|n| n.kind == K::ModulePath)
                                .and_then(|p| p.child_tokens().find(|t| t.kind == K::Con))
                        });
                    if let Some(qualifier) = qualifier {
                        self.module_locals.push(qualifier.text(self.text));
                    }
                    continue;
                }
                let names = declaration
                    .child_tokens()
                    .filter(|t| matches!(t.kind, K::Ident | K::Con));
                self.locals.extend(names.map(|t| Local {
                    name: t.text(self.text),
                    generated: t.start >= self.generated,
                    binder: None,
                    used: false,
                }));
                if let Some(pattern) = declaration.nodes().find(|n| is_pattern(n.kind)) {
                    let mut names = Vec::new();
                    binders(pattern, &mut names);
                    self.locals.extend(names.into_iter().map(|t| Local {
                        name: t.text(self.text),
                        generated: t.start >= self.generated,
                        binder: None,
                        used: false,
                    }));
                }
            }
            self.walk(body);
        }
        self.module_locals.truncate(modules);
        self.locals.truncate(mark);
    }

    fn module_signature(&mut self, node: &Node) {
        let mark = self.module_locals.len();
        let mut parts = node.nodes();
        if let Some(input) = parts.next() {
            if input.kind == K::ModuleSignatureParen
                && input.nodes().all(|n| n.kind == K::ModuleSignatureBinder)
            {
                for binder in input.nodes() {
                    for signature in binder.nodes() {
                        self.walk(signature);
                    }
                    if let Some(name) = binder.child_tokens().find(|t| t.kind == K::Con) {
                        self.module_locals.push(name.text(self.text));
                    }
                }
            } else {
                self.walk(input);
            }
        }
        for result in parts {
            self.walk(result);
        }
        self.module_locals.truncate(mark);
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
            if let (Some((token, _)), true) = (local.binder, local.used) {
                self.used.push(token.start);
            }
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
            generated: t.start >= self.generated,
            binder: Some((t, puns.contains(&t.start))),
            used: false,
        }));
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
                generated: t.start >= self.generated,
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
    /// The classes a `deriving` clause names, each a path between commas.
    fn deriving(&mut self, clause: &Node) {
        let tokens: Vec<_> = (clause.child_tokens())
            .filter(|t| matches!(t.kind, K::Con | K::Comma))
            .collect();
        for path in tokens.split(|t| t.kind == K::Comma) {
            let (Some(first), Some(last)) = (path.first(), path.last()) else {
                continue;
            };
            let parts: Vec<&str> = path.iter().map(|t| t.text(self.text)).collect();
            let Some((&name, qualifier)) = parts.split_last() else {
                continue;
            };
            self.out.push(Reference {
                namespace: Namespace::Type,
                qualifier: (!qualifier.is_empty()).then(|| qualifier.join(".")),
                name: name.to_owned(),
                span: Span::new(self.file, first.start, last.end),
                top: self.top,
            });
        }
    }

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
        if qualifier
            .first()
            .is_some_and(|name| self.module_locals.contains(name))
        {
            return;
        }
        // a companion `T$schema` of generated code is a value, its token one
        let namespace = if last.kind == K::Con {
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
            let generated = first.start >= self.generated;
            let seen = |l: &&mut Local| l.name == name && l.generated == generated;
            for local in self.locals.iter_mut().filter(seen) {
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
