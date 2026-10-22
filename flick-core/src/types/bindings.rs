//! `let` bindings, signatures and blocks (§3): every named function has its
//! full signature right above it; a signature is checked, not trusted, and
//! does not lift the generalization rules (§5 "Generalization при наличии Ref").

use std::mem;

use super::convert::{TyVar, is_type};
use super::patterns::is_pattern;
use super::ty::Type;
use super::{Checker, Local, Rec};
use crate::diagnostics::{Diagnostic, Edit, codes};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

/// The parts of a `let`.
pub struct LetParts<'n> {
    /// `Some` for `let name …`, `None` for `let pattern = …`.
    pub name: Option<String>,
    pub pattern: Option<&'n Node>,
    pub params: Vec<&'n Node>,
    /// The `: T` after the parameters.
    pub header: Option<&'n Node>,
    pub body: Option<&'n Node>,
    pub rec: bool,
}

impl LetParts<'_> {
    /// A syntactic function: parameters, or a `fun` right after `=`.
    pub fn is_function(&self) -> bool {
        !self.params.is_empty()
            || self.body.is_some_and(|b| {
                let mut statements = b.nodes();
                matches!(
                    (statements.next(), statements.next()),
                    (Some(s), None) if s.kind == K::FunExpr
                )
            })
    }
}

pub fn let_parts<'n>(decl: &'n Node, text: &str) -> LetParts<'n> {
    let pattern = decl.nodes().find(|n| is_pattern(n.kind));
    let name = pattern
        .filter(|p| p.kind == K::VarPat)
        .and_then(|p| p.child_tokens().find(|t| t.kind == K::Ident))
        .map(|t| t.text(text).to_owned());
    LetParts {
        name,
        pattern,
        params: decl
            .nodes()
            .filter(|n| n.kind == K::Params)
            .flat_map(Node::nodes)
            .collect(),
        header: decl.nodes().find(|n| is_type(n.kind)),
        body: decl.nodes().find(|n| n.kind == K::Block),
        rec: decl.has_token(K::RecKw),
    }
}

/// The name a signature declares.
pub fn signature_name<'t>(sig: &Node, text: &'t str) -> Option<&'t str> {
    sig.child_tokens()
        .find(|t| t.kind == K::Ident)
        .map(|t| t.text(text))
}

impl Checker<'_> {
    pub fn block(&mut self, node: &Node, expected: Option<&Type>) -> Type {
        let mark = self.locals.len();
        let statements: Vec<&Node> = node.nodes().collect();
        let mut result = None;
        let mut i = 0;
        while i < statements.len() {
            let statement = statements[i];
            let last = i + 1 == statements.len();
            match statement.kind {
                K::Signature => {
                    let name = signature_name(statement, self.text);
                    let next = statements.get(i + 1).copied().filter(|n| {
                        n.kind == K::LetDecl && let_parts(n, self.text).name.as_deref() == name
                    });
                    match next {
                        Some(decl) => {
                            self.local_let(decl, Some(statement));
                            i += 1;
                        }
                        None => self.lone_signature(statement, false),
                    }
                }
                K::LetDecl => self.local_let(statement, None),
                _ if last => {
                    result = Some(match expected {
                        Some(expected) => {
                            self.check(statement, expected);
                            expected.clone()
                        }
                        None => self.infer(statement),
                    });
                }
                _ => self.statement(statement),
            }
            i += 1;
        }
        self.locals.truncate(mark);
        result.unwrap_or_else(|| {
            let end = node.range().1;
            self.error(
                codes::T019,
                Span::new(self.file, end, end),
                "a block must end with an expression: its value is the value of the block",
            );
            Type::Error
        })
    }

    /// An expression statement before the last one: its value must be `()`.
    fn statement(&mut self, node: &Node) {
        let ty = self.infer(node);
        if self.unify(&ty, &Type::prim("Unit")).is_err() {
            let shown = self.show(&ty);
            let span = self.span(node);
            self.diagnostics.push(
                Diagnostic::error(
                    codes::T011,
                    format!(
                        "the result of this expression has type `{shown}` and would be discarded; only `Unit` may be dropped"
                    ),
                    span,
                )
                .with_fix(
                    "discard it explicitly with `ignore`",
                    vec![Edit {
                        span: Span::new(span.file, span.start, span.start),
                        replacement: "ignore ".to_owned(),
                    }],
                ),
            );
        }
    }

    pub fn lone_signature(&mut self, sig: &Node, elsewhere: bool) {
        let name = signature_name(sig, self.text).unwrap_or_default();
        let message = if elsewhere {
            format!("the signature of `{name}` must be right above its definition")
        } else {
            format!("signature `{name}` has no definition below it")
        };
        self.error(codes::T006, self.span(sig), message);
    }

    fn local_let(&mut self, decl: &Node, sig: Option<&Node>) {
        let parts = let_parts(decl, self.text);
        let Some(name) = parts.name.clone() else {
            self.pattern_let(decl, &parts);
            return;
        };
        let signature = sig
            .and_then(|s| s.nodes().find(|n| is_type(n.kind)))
            .map(|t| self.signature(t));
        if let Some((scheme, explicit)) = signature {
            let rec = parts.rec.then(|| {
                let rec = (!explicit).then(|| self.new_rec(&name));
                self.locals.push(Local {
                    name: name.clone(),
                    ty: scheme.clone(),
                    rec,
                });
                rec
            });
            let mark = self.locals.len();
            let runtime = self.check_signed(decl, &parts, &scheme, rec.flatten());
            self.locals.truncate(mark - usize::from(rec.is_some()));
            if let Some(Some(rec)) = rec {
                self.finish_rec(rec);
            }
            self.polymorphic_runtime(&name, &scheme, runtime, decl);
            self.push_local(&name, scheme);
        } else {
            if parts.rec {
                let ty = self.fresh();
                self.push_local(&name, ty);
            }
            let ty = self.unsigned(decl, &parts, &name);
            if parts.rec {
                self.locals.pop();
            }
            self.push_local(&name, ty);
        }
    }

    /// `let (a, b) = …`: the names are monomorphic.
    pub fn pattern_let(&mut self, decl: &Node, parts: &LetParts) {
        self.store.level += 1;
        let ty = parts.body.map_or(Type::Error, |b| self.infer(b));
        self.store.level -= 1;
        self.store.lower(&ty);
        if let Some(pattern) = parts.pattern {
            let pat = self.bind_pattern(pattern, &ty);
            self.irrefutable(&pat, pattern);
        } else {
            let _ = decl;
        }
    }

    pub fn new_rec(&mut self, name: &str) -> usize {
        self.recs.push(Rec {
            name: name.to_owned(),
            target: None,
            uses: Vec::new(),
        });
        self.recs.len() - 1
    }

    /// Recursive uses of a function without an explicit `forall` must have
    /// the very type its body is checked against (§3).
    pub fn finish_rec(&mut self, rec: usize) {
        let Rec { name, target, uses } = mem::replace(
            &mut self.recs[rec],
            Rec {
                name: String::new(),
                target: None,
                uses: Vec::new(),
            },
        );
        let Some(target) = target else { return };
        for (ty, span) in uses {
            if self.unify(&ty, &target).is_err() {
                let (used, own) = (self.show(&ty), self.show(&target));
                self.diagnostics.push(
                    Diagnostic::error(
                        codes::T015,
                        format!(
                            "`{name}` is used at type `{used}` inside its own definition, which has type `{own}`: that is polymorphic recursion"
                        ),
                        span,
                    )
                    .with_fix(
                        format!("quantify every variable of the signature explicitly: `{name} : forall … .`"),
                        Vec::new(),
                    ),
                );
            }
        }
    }

    /// Checks the definition against its signature; returns whether the
    /// right-hand side ran a runtime call while being defined.
    pub fn check_signed(
        &mut self,
        decl: &Node,
        parts: &LetParts,
        scheme: &Type,
        rec: Option<usize>,
    ) -> bool {
        self.header_types(decl, parts, true);
        self.store.level += 1;
        let kinds = binders(scheme);
        let (body, skolems) = self.skolemize(scheme);
        let mark = self.tyvars.len();
        for (name, ty) in skolems {
            let kind = kinds
                .iter()
                .find(|b| b.name == name)
                .map_or(super::kinds::Kind::Type, |b| b.kind.clone());
            self.tyvars.push(TyVar { name, ty, kind });
        }
        if let Some(rec) = rec {
            self.recs[rec].target = Some(body.clone());
        }
        let saved = mem::take(&mut self.runtime);
        if parts.params.is_empty() {
            if let Some(b) = parts.body {
                self.check(b, &body);
            }
        } else {
            self.function(&parts.params, parts.body, Some(&body), decl);
        }
        let runtime = mem::replace(&mut self.runtime, saved);
        self.tyvars.truncate(mark);
        self.store.level -= 1;
        runtime
    }

    /// A polymorphic signature over a value computed by a runtime call is
    /// rejected: each use would share one result at different types (§5).
    pub fn polymorphic_runtime(&mut self, name: &str, scheme: &Type, runtime: bool, decl: &Node) {
        if runtime && matches!(scheme, Type::Forall { vars, .. } if !vars.is_empty()) {
            let shown = self.show(scheme);
            self.error(
                codes::T013,
                self.span(decl),
                format!(
                    "`{name}` is computed by a runtime call, so its value cannot have the polymorphic type `{shown}`; make it a function or build it with `task`"
                ),
            );
        }
    }

    /// A `let` without a signature: a value, whose type is inferred and
    /// generalized when it is built purely. A function here is an error.
    pub fn unsigned(&mut self, decl: &Node, parts: &LetParts, name: &str) -> Type {
        let function = parts.is_function();
        let header = self.header_types(decl, parts, false);
        if parts.rec && !parts.is_function() {
            self.error(
                codes::T012,
                self.span(decl),
                format!(
                    "`{name}` would be evaluated while it is being defined; only functions can be recursive"
                ),
            );
        }
        self.store.level += 1;
        let saved = mem::take(&mut self.runtime);
        let ty = match (parts.params.is_empty(), parts.header) {
            (false, _) => self.function(&parts.params, parts.body, None, decl),
            (true, Some(header)) => {
                let ty = self.annotation(header);
                if let Some(b) = parts.body {
                    self.check(b, &ty);
                }
                ty
            }
            (true, None) => parts.body.map_or(Type::Error, |b| self.infer(b)),
        };
        let runtime = mem::replace(&mut self.runtime, saved);
        self.store.level -= 1;
        let is_fun = matches!(self.whnf(&ty), Type::Fun { .. });
        if let (true, Some(written), true) = (is_fun, parts.header, parts.params.is_empty()) {
            let written = self.text_of(written);
            self.error(
                codes::T005,
                self.span(decl),
                format!(
                    "write the type of function `{name}` as a signature above the `let`: `{name} : {written}`"
                ),
            );
        } else if (function || is_fun) && !header {
            let suggestion = self.generalize_copy(&ty);
            let shown = self.show(&suggestion);
            self.diagnostics.push(
                Diagnostic::error(
                    codes::T004,
                    format!("function `{name}` needs a signature right above its `let`"),
                    self.span(decl),
                )
                .with_fix(format!("add `{name} : {shown}`"), Vec::new()),
            );
        }
        if runtime || function {
            self.store.lower(&ty);
            ty
        } else {
            self.generalize(&ty)
        }
    }

    /// Types written in a function's `let` header instead of its signature;
    /// `true` after a diagnostic.
    fn header_types(&mut self, decl: &Node, parts: &LetParts, signed: bool) -> bool {
        let annotated: Vec<&Node> = parts
            .params
            .iter()
            .copied()
            .filter(|p| p.kind == K::AnnotPat)
            .collect();
        if parts.params.is_empty() || (parts.header.is_none() && annotated.is_empty()) {
            return false;
        }
        let name = parts.name.clone().unwrap_or_default();
        let mut message = format!(
            "write the types of `{name}` in its signature above the `let`, not in the `let` header"
        );
        if !signed {
            let mut types: Vec<String> = parts
                .params
                .iter()
                .map(|p| match p.kind {
                    K::AnnotPat => p
                        .nodes()
                        .find(|n| is_type(n.kind))
                        .map_or("_".to_owned(), |t| self.text_of(t)),
                    K::UnitPat => "Unit".to_owned(),
                    _ => "_".to_owned(),
                })
                .collect();
            types.push(parts.header.map_or("_".to_owned(), |t| self.text_of(t)));
            message = format!("{message}: `{name} : {}`", types.join(" -> "));
        }
        self.error(codes::T005, self.span(decl), message);
        true
    }
}

/// The binders of a scheme's outer `forall`.
fn binders(scheme: &Type) -> Vec<super::ty::Binder> {
    match scheme {
        Type::Forall { vars, body, .. } => vars.iter().cloned().chain(binders(body)).collect(),
        _ => Vec::new(),
    }
}
