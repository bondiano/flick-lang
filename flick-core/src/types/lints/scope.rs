//! Names and arms that hide something: R015 a `let` or `<-` that shadows a
//! local, T054 a local function of several lines, not `rec`, that uses
//! nothing around it, and T055 an
//! `_` arm over a type of this module, which hides the constructors added
//! to it later.

use super::super::Checker;
use super::super::env::Form;
use super::super::let_parts;
use super::super::sugar_lints::unparen;
use super::super::ty::Type;
use super::super::typing::Ref;
use crate::diagnostics::codes;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

/// The locals in scope, innermost last.
type Scope = Vec<String>;

impl Checker<'_> {
    /// The scope lints of each top-level function of the module `file`.
    pub(super) fn scope_lints(&mut self, file: &Node) {
        for decl in file.nodes().filter(|n| n.kind == K::LetDecl) {
            let parts = let_parts(decl, self.text);
            let mut scope = Scope::new();
            for param in &parts.params {
                self.bind_locals(param, &mut scope, None);
            }
            if let Some(body) = parts.body {
                self.scope_walk(body, &mut scope);
            }
        }
    }

    fn scope_walk(&mut self, node: &Node, scope: &mut Scope) {
        let mark = scope.len();
        match node.kind {
            K::Block => {
                for statement in node.nodes() {
                    self.scope_statement(statement, scope);
                }
            }
            K::ForExpr => {
                // what its generators bind, its `yield` sees
                for child in node.nodes() {
                    match child.kind {
                        K::Block => child.nodes().for_each(|s| self.scope_statement(s, scope)),
                        _ => self.scope_walk(child, scope),
                    }
                }
            }
            K::FunExpr => {
                for child in node.nodes() {
                    // a parameter named like a local is the usual way to write a `fun`
                    if super::super::is_pattern(child.kind) {
                        let names = self.pattern_variables(child).into_iter();
                        scope.extend(names.map(|(n, _)| n));
                    } else {
                        self.scope_walk(child, scope);
                    }
                }
            }
            K::MatchExpr => {
                let mut parts = node.nodes();
                let scrutinee = parts.next();
                if let Some(s) = scrutinee {
                    self.scope_walk(s, scope);
                }
                let arms: Vec<&Node> = parts.filter(|n| n.kind == K::MatchArm).collect();
                for arm in &arms {
                    let mut inner = arm.nodes();
                    // an arm names the parts of what it matches as it likes
                    if let Some(pattern) = inner.next() {
                        let names = self.pattern_variables(pattern).into_iter();
                        scope.extend(names.map(|(n, _)| n));
                    }
                    inner.for_each(|n| self.scope_walk(n, scope));
                    scope.truncate(mark);
                }
                if let Some(s) = scrutinee {
                    self.wildcard_arm(s, &arms);
                }
            }
            K::BinExpr => {
                // a chain of operators binds nothing: its operands, in a loop
                let (spine, left) = node.left_spine();
                self.scope_walk(left, scope);
                for child in spine.iter().rev().flat_map(|n| n.nodes().skip(1)) {
                    self.scope_walk(child, scope);
                }
            }
            _ => node.nodes().for_each(|n| self.scope_walk(n, scope)),
        }
        scope.truncate(mark);
    }

    /// A statement of a block: what it binds stays in scope after it.
    fn scope_statement(&mut self, node: &Node, scope: &mut Scope) {
        match node.kind {
            K::LetDecl => {
                let parts = let_parts(node, self.text);
                if parts.params.is_empty() {
                    if let Some(body) = parts.body {
                        self.scope_walk(body, scope);
                    }
                    if let Some(pattern) = parts.pattern {
                        let value = parts.body.map(|b| self.source_of(b).to_owned());
                        self.bind_locals(pattern, scope, value.as_deref());
                    }
                    return;
                }
                self.uses_nothing(node, scope);
                let mark = scope.len();
                for param in &parts.params {
                    self.bind_locals(param, scope, None);
                }
                if let Some(body) = parts.body {
                    self.scope_walk(body, scope);
                }
                scope.truncate(mark);
                if let Some(pattern) = parts.pattern {
                    self.bind_locals(pattern, scope, None);
                }
            }
            K::BindStmt => {
                let mut parts = node.nodes();
                let (pattern, value) = (parts.next(), parts.next());
                if let Some(v) = value {
                    self.scope_walk(v, scope);
                }
                if let Some(p) = pattern {
                    let source = value.map(|v| self.source_of(v).to_owned());
                    self.bind_locals(p, scope, source.as_deref());
                }
            }
            _ => self.scope_walk(node, scope),
        }
    }

    /// Brings the variables of `pattern` into scope. R015 at each one that
    /// shadows a local, unless `from`, what it is bound from, names it: a
    /// value rebound from itself.
    fn bind_locals(&mut self, pattern: &Node, scope: &mut Scope, from: Option<&str>) {
        for (name, at) in self.pattern_variables(pattern) {
            let rebound = from.is_some_and(|f| mentions(f, &name));
            if scope.contains(&name) && !rebound && !name.starts_with('_') {
                self.lint(
                    codes::R015,
                    format!("`{name}` shadows the local `{name}` bound above"),
                    at,
                    |_| ("give it a name of its own", Vec::new(), false),
                );
            }
            scope.push(name);
        }
    }

    fn pattern_variables(&self, pattern: &Node) -> Vec<(String, Span)> {
        let mut out = Vec::new();
        if pattern.kind == K::VarPat
            && let Some(t) = pattern.child_tokens().find(|t| t.kind == K::Ident)
        {
            out.push((t.text(self.text).to_owned(), self.token_span(t)));
        }
        for child in pattern.nodes() {
            out.extend(self.pattern_variables(child));
        }
        out
    }

    /// T054: a local function that uses no local around it.
    fn uses_nothing(&mut self, decl: &Node, scope: &Scope) {
        let parts = let_parts(decl, self.text);
        let (Some(name), Some(pattern)) = (parts.name, parts.pattern) else {
            return;
        };
        // a recursive worker and a one-line helper read best where they are used
        if parts.rec || parts.body.is_none_or(|b| self.one_line(b)) {
            return;
        }
        let own: Vec<String> = decl
            .nodes()
            .filter(|n| n.kind != K::VarPat || n.range() != pattern.range())
            .flat_map(|n| self.pattern_variables(n))
            .map(|(n, _)| n)
            .collect();
        if self.captures_local(decl, scope, &own) {
            return;
        }
        self.lint(
            codes::T054,
            format!("the local function `{name}` uses no local around it: it can be a top-level function"),
            self.span(pattern),
            |_| ("move it to the top level", Vec::new(), false),
        );
    }

    /// Whether `node` refers to a local of `scope` that `own` does not bind.
    fn captures_local(&self, node: &Node, scope: &Scope, own: &[String]) -> bool {
        if node.kind == K::NameExpr
            && matches!(self.typing.names.get(&node.range()), Some(Ref::Local))
        {
            let name = self.source_of(node).trim();
            if scope.iter().any(|s| s == name) && !own.iter().any(|o| o == name) {
                return true;
            }
        }
        node.nodes().any(|n| self.captures_local(n, scope, own))
    }

    /// T055: an `_` arm in a `match` on a variant type of this module.
    fn wildcard_arm(&mut self, scrutinee: &Node, arms: &[&Node]) {
        let Some(wildcard) = arms.iter().find_map(|arm| {
            let mut parts = arm.nodes();
            let pattern = parts.next()?;
            let guarded = arm.nodes().any(|n| n.kind == K::Guard);
            (pattern.kind == K::WildcardPat && !guarded).then_some(pattern)
        }) else {
            return;
        };
        // the types of expressions are settled once the module is
        let at = unparen(scrutinee).range();
        let Some((_, ty)) = self.typing.pending_types.iter().find(|(r, _)| *r == at) else {
            return;
        };
        let ty = self.zonk(ty);
        let Type::Con { module, name } = ty.spine().0 else {
            return;
        };
        if *module != self.env.module {
            return;
        }
        let Some(Form::Adt { constructors, .. }) = self.env.type_def(module, name).map(|d| &d.form)
        else {
            return;
        };
        let written: Vec<&str> = arms
            .iter()
            .filter_map(|arm| arm.nodes().next())
            .flat_map(Node::tokens)
            .filter(|t| t.kind == K::Con)
            .map(|t| t.text(self.text))
            .collect();
        let hidden: Vec<String> = constructors
            .iter()
            .filter(|c| !written.contains(&c.as_str()) && self.builds(module, name, c, &ty))
            .map(|c| format!("`{c}`"))
            .collect();
        if hidden.is_empty() {
            return;
        }
        let at = self.span(wildcard);
        self.lint(
            codes::T055,
            format!(
                "`_` stands for {} of `{name}`: a constructor added to it later lands here unchecked",
                hidden.join(", ")
            ),
            at,
            |_| ("write an arm for each constructor", Vec::new(), false),
        );
    }
}

/// Whether `source` names `name` as a whole word.
fn mentions(source: &str, name: &str) -> bool {
    source
        .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '\''))
        .any(|w| w == name)
}
