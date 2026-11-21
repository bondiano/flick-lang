//! `let` bindings, signatures and blocks: every named function has its
//! full signature right above it; a signature is checked, not trusted, and
//! does not lift the generalization rules.

use crate::types::Elided;
use std::mem;

use super::convert::{TyVar, is_type};
use super::patterns::is_pattern;
use super::ty::Type;
use super::{Checker, Local};
use crate::diagnostics::{Diagnostic, Edit, codes};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::patterns;
use crate::types::lang::Lang;

mod header;
mod signatures;
mod unsigned;

pub(super) use signatures::PendingSignature;

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
    /// `let assert`: the pattern may miss, and the program fails when it does.
    pub assert: bool,
}

impl LetParts<'_> {
    /// A syntactic function: parameters, a `fun`, or an application hole capture right after `=`.
    pub fn is_function(&self) -> bool {
        !self.params.is_empty()
            || self.body.is_some_and(|b| {
                let mut statements = b.nodes();
                matches!(
                    (statements.next(), statements.next()),
                    (Some(s), None) if s.kind == K::FunExpr || super::holes::has_hole(s)
                )
            })
    }
}

pub fn let_parts<'n>(decl: &'n Node, text: &str) -> LetParts<'n> {
    let pattern = decl.nodes().find(|n| is_pattern(n.kind));
    let assert = decl.has_token(K::AssertKw);
    // `let assert x = …` binds like a pattern, to be checked as one
    let name = pattern
        .filter(|p| p.kind == K::VarPat && !assert)
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
        assert,
    }
}

/// The name a signature declares.
pub fn signature_name<'t>(sig: &Node, text: &'t str) -> Option<&'t str> {
    sig.child_tokens()
        .find(|t| t.kind == K::Ident)
        .map(|t| t.text(text))
}

/// What follows the statements of a block as its value.
pub type Tail<'f, C> = &'f mut dyn FnMut(&mut C, Option<&Type>) -> Type;

impl Checker<'_> {
    pub fn block(&mut self, node: &Node, expected: Option<&Type>) -> Type {
        self.block_then(node, expected, None)
    }

    /// The block `node`, whose last statement `tail` checks in the scope of
    /// the others when given: the setup of a component and its render.
    pub fn block_then(
        &mut self,
        node: &Node,
        expected: Option<&Type>,
        mut tail: Option<Tail<'_, Self>>,
    ) -> Type {
        let mark = self.cx.locals.len();
        let statements: Vec<&Node> = node.nodes().collect();
        let mut result = None;
        let mut i = 0;
        while i < statements.len() {
            let statement = statements[i];
            let last = i + 1 == statements.len();
            match statement.kind {
                K::Signature | K::LetDecl => i += self.local_declaration(&statements, i),
                _ if last && tail.is_some() => {
                    let tail = tail.as_mut().expect("a tail");
                    result = Some(tail(self, expected));
                }
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
        self.cx.locals.truncate(mark);
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

    /// The `let` or the signature `statements[i]` of a block, in sight of the
    /// statements after it; a signature types the `let` after it of its
    /// name. How many statements after the `i`-th it took too.
    pub(super) fn local_declaration(&mut self, statements: &[&Node], i: usize) -> usize {
        let statement = statements[i];
        if statement.kind == K::LetDecl {
            self.local_let(statement, None, &statements[i + 1..]);
            return 0;
        }
        let name = signature_name(statement, self.text);
        let next = statements
            .get(i + 1)
            .copied()
            .filter(|n| n.kind == K::LetDecl && let_parts(n, self.text).name.as_deref() == name);
        if let Some(decl) = next {
            self.local_let(decl, Some(statement), &statements[i + 2..]);
            return 1;
        }
        self.lone_signature(statement, false);
        0
    }

    /// An expression statement before the last one: its value must be `()`.
    pub(super) fn statement(&mut self, node: &Node) {
        let ty = self.infer(node);
        if self.unify(&ty, &Type::lang(Lang::Unit)).is_err() {
            let shown = self.show(&ty);
            let span = self.span(node);
            self.diagnostics.push(
                Diagnostic::error(
                    codes::T011,
                    format!(
                        "the result of this expression has type `{shown}` and would be discarded; only `Unit` may be dropped"
                    ),
                    span,
                ).with_fix(|| ("discard it explicitly with `ignore`", vec![Edit {
                    span: Span::new(span.file, span.start, span.start),
                    replacement: "ignore ".to_owned(),
                }])),
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

    pub(super) fn local_let(&mut self, decl: &Node, sig: Option<&Node>, rest: &[&Node]) {
        let parts = let_parts(decl, self.text);
        let Some(name) = parts.name.clone() else {
            self.pattern_let(decl, &parts, Some(rest));
            return;
        };
        let binder = decl.nodes().find(|n| n.kind == K::VarPat);
        let at = binder.unwrap_or(decl).range().0;
        // `validate` reports the other places of `lazy` and `mut`, which bind as plain `let`
        if decl.has_token(K::LazyKw) && parts.params.is_empty() && !parts.rec {
            self.local_lazy(decl, &parts, &name, at);
            if let (Some(binder), Some(local)) = (binder, self.cx.locals.last()) {
                let ty = local.ty.clone();
                self.note_type(self.span(binder), &ty);
            }
            return;
        }
        if decl.has_token(K::MutKw) && parts.params.is_empty() && !parts.rec {
            self.local_mut(decl, sig, &parts, &name, at);
            if let (Some(binder), Some(local)) = (binder, self.cx.locals.last()) {
                let ty = local.ty.clone();
                self.note_type(self.span(binder), &ty);
            }
            return;
        }
        let signature = sig
            .and_then(|s| s.nodes().find(|n| is_type(n.kind)))
            .map(|t| self.signature(t));
        if let Some((scheme, explicit)) = signature {
            self.local_signed_let(decl, &parts, &name, at, scheme, explicit);
        } else {
            if parts.rec {
                let ty = self.fresh();
                self.push_local(&name, at, ty);
            }
            let ty = self.unsigned(decl, &parts, &name);
            if parts.rec {
                self.cx.locals.pop();
            } else if parts.params.is_empty() && parts.header.is_some() {
                #[cfg(not(target_arch = "wasm32"))]
                self.typing
                    .pending_local_schemes
                    .push((decl.range(), ty.clone()));
            }
            let alias = self.let_alias(&parts);
            self.push_local(&name, at, ty);
            self.proof_alias(at, alias);
            if let Some(local) = self.cx.locals.last_mut() {
                local.unsigned = parts.is_function();
                if !parts.is_function() && !parts.rec {
                    local.binding = super::mutation::Binding::Let;
                }
            }
        }
        if let (Some(binder), Some(local)) = (binder, self.cx.locals.last()) {
            let ty = local.ty.clone();
            self.note_type(self.span(binder), &ty);
        }
    }

    fn local_signed_let(
        &mut self,
        decl: &Node,
        parts: &LetParts,
        name: &str,
        at: u32,
        scheme: Type,
        explicit: bool,
    ) {
        #[cfg(not(target_arch = "wasm32"))]
        self.typing
            .pending_local_schemes
            .push((decl.range(), scheme.clone()));
        let rec = parts.rec.then(|| {
            let rec = (!explicit).then(|| self.new_rec(name));
            self.cx.locals.push(Local {
                name: name.to_owned(),
                at: Some(at),
                generated: at >= self.generated,
                ty: scheme.clone(),
                rec,
                unsigned: false,
                binding: super::mutation::Binding::Other,
            });
            rec
        });
        let mark = self.cx.locals.len();
        // what a local holds is inferred: its signature need not name the regions,
        // except a bare pure outer arrow whose later stage keeps its own bound
        let captures = self.infers_local_captures(&scheme).then(|| self.fresh());
        let runtime = self.check_signed(decl, parts, &scheme, rec.flatten(), captures.as_ref());
        self.cx.locals.truncate(mark - usize::from(rec.is_some()));
        if let Some(Some(rec)) = rec {
            self.finish_recs(&[rec]);
        }
        self.polymorphic_signature(name, &scheme, runtime, decl, parts);
        let mut scheme_head = &scheme;
        while let Type::Forall { body, .. }
        | Type::Captured { ty: body, .. }
        | Type::Bounded { ty: body, .. } = scheme_head
        {
            scheme_head = body;
        }
        let is_function_scheme = parts.is_function() || matches!(scheme_head, Type::Fun { .. });
        let local_scheme = if is_function_scheme {
            super::effects::clear_written(&scheme)
        } else {
            scheme
        };
        let ty = match captures {
            Some(captures) => self.inferred_captures(local_scheme, captures, self.span(decl)),
            None => local_scheme,
        };
        let alias = self.let_alias(parts);
        self.push_local(name, at, ty);
        self.proof_alias(at, alias);
        if let Some(local) = self.cx.locals.last_mut()
            && !parts.is_function()
            && !parts.rec
        {
            local.binding = super::mutation::Binding::Let;
        }
    }

    /// Whether a local binding with signature `scheme` gets an inferred outer
    /// capture qualifier (`docs/formal/regions.md#local-signatures-infer-only-the-outer-qualifier`).
    /// An explicit outer qualifier written on a local callable is respected as written.
    fn infers_local_captures(&self, scheme: &Type) -> bool {
        !matches!(self.qualified_head(scheme), Type::Captured { ty, .. } if self.is_callable(&ty))
    }

    /// `let (a, b) = …`: the names are monomorphic.
    /// `rest` holds the statements after a local `let` in its block.
    pub fn pattern_let(&mut self, decl: &Node, parts: &LetParts, rest: Option<&[&Node]>) {
        self.store.level += 1;
        let ty = parts.body.map_or(Type::Error, |b| self.infer(b));
        self.store.level -= 1;
        self.store.lower(&ty);
        if let Some(pattern) = parts.pattern {
            let pat = self.bind_pattern(pattern, &ty);
            if parts.assert {
                self.let_assert((&pat, &ty), pattern, decl, rest);
            } else {
                self.irrefutable(&pat, &ty, pattern, rest.map(|rest| (decl, rest)));
            }
        }
    }

    /// Reports `FLICK-T012` when `let rec` defines a non-function value.
    pub(super) fn recursive_value(&mut self, decl: &Node, parts: &LetParts, name: &str) {
        if parts.rec && !parts.is_function() {
            self.error(
                codes::T012,
                self.span(decl),
                format!(
                    "`{name}` would be evaluated while it is being defined; only functions can be recursive"
                ),
            );
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
        captures: Option<&Type>,
    ) -> bool {
        self.header_types(decl, parts, true);
        if let Some(name) = &parts.name {
            self.recursive_value(decl, parts, name);
        }
        self.store.level += 1;
        let kinds = binders(scheme);
        let (body, skolems, givens) = self.skolemize(scheme);
        // each top-level item is checked from level 0
        if self.store.level == 1 {
            let rigid = skolems.iter().filter_map(|(_, t)| match t {
                Type::Skolem(s) => Some(*s),
                _ => None,
            });
            self.cx.regions.signed.extend(rigid);
        }
        self.record_givens(self.span(decl), &givens);
        let body = match captures {
            Some(c) => Type::Captured {
                ty: Box::new(body),
                captures: Box::new(c.clone()),
                value: false,
                elided: Elided::default(),
            },
            None => body,
        };
        let assumed = self.classes.givens.len();
        self.classes.givens.extend(givens);
        let mark = self.cx.tyvars.len();
        for (name, ty) in skolems.iter().cloned() {
            let kind = kinds
                .iter()
                .find(|b| b.name == name)
                .map_or(super::kinds::Kind::Type, |b| b.kind.clone());
            self.cx.tyvars.push(TyVar { name, ty, kind });
        }
        if let Some(rec) = rec {
            let rec = &mut self.recs[rec];
            rec.scheme = Some(scheme.clone());
            rec.target = Some(body.clone());
            rec.skolems.clone_from(&skolems);
        }
        let saved = mem::take(&mut self.cx.runtime);
        let complete = !scheme.contains(&mut |t| matches!(t, Type::Meta(_)));
        let signed = mem::replace(&mut self.gadt.signed, complete);
        if parts.params.is_empty() {
            if let Some(b) = parts.body {
                self.check(b, &body);
            }
        } else {
            self.function(&parts.params, parts.body, Some(&body), decl);
        }
        self.gadt.signed = signed;
        let runtime = mem::replace(&mut self.cx.runtime, saved);
        self.classes.givens.truncate(assumed);
        self.cx.tyvars.truncate(mark);
        self.store.level -= 1;
        runtime
    }
}

/// The binders of a scheme's outer `forall`.
fn binders(scheme: &Type) -> Vec<super::ty::Binder> {
    match scheme {
        Type::Forall { vars, body, .. } => vars.iter().cloned().chain(binders(body)).collect(),
        _ => Vec::new(),
    }
}

/// The names a top-level pattern binds, a field pun among them.
pub(super) fn pattern_names(pattern: &Node, text: &str, out: &mut Vec<String>) {
    let mut names = Vec::new();
    patterns::binders(pattern, &mut names);
    out.extend(names.iter().map(|name| name.text(text).to_owned()));
}
