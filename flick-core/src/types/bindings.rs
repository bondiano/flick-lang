//! `let` bindings, signatures and blocks: every named function has its
//! full signature right above it; a signature is checked, not trusted, and
//! does not lift the generalization rules.

use std::mem;

use super::convert::{TyVar, is_type};
use super::patterns::is_pattern;
use super::ty::Type;
use super::{Checker, Local};
use crate::diagnostics::{Diagnostic, Edit, codes};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::Lang;

mod header;

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
                            self.local_let(decl, Some(statement), &statements[i + 2..]);
                            i += 1;
                        }
                        None => self.lone_signature(statement, false),
                    }
                }
                K::LetDecl => self.local_let(statement, None, &statements[i + 1..]),
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
        let signature = sig
            .and_then(|s| s.nodes().find(|n| is_type(n.kind)))
            .map(|t| self.signature(t));
        if let Some((scheme, explicit)) = signature {
            let rec = parts.rec.then(|| {
                let rec = (!explicit).then(|| self.new_rec(&name));
                self.locals.push(Local {
                    name: name.clone(),
                    at: Some(at),
                    generated: at >= self.generated,
                    ty: scheme.clone(),
                    rec,
                    unsigned: false,
                });
                rec
            });
            let mark = self.locals.len();
            // what a local holds is inferred: its signature need not name the regions
            let captures = self.fresh();
            let runtime = self.check_signed(decl, &parts, &scheme, rec.flatten(), Some(&captures));
            self.locals.truncate(mark - usize::from(rec.is_some()));
            if let Some(Some(rec)) = rec {
                self.finish_recs(&[rec]);
            }
            self.polymorphic_signature(&name, &scheme, runtime, decl, &parts);
            let ty = self.inferred_captures(scheme, captures, self.span(decl));
            self.push_local(&name, at, ty);
        } else {
            if parts.rec {
                let ty = self.fresh();
                self.push_local(&name, at, ty);
            }
            let ty = self.unsigned(decl, &parts, &name);
            if parts.rec {
                self.locals.pop();
            }
            self.push_local(&name, at, ty);
            if let Some(local) = self.locals.last_mut() {
                local.unsigned = parts.is_function();
            }
        }
        if let (Some(binder), Some(local)) = (binder, self.locals.last()) {
            let ty = local.ty.clone();
            self.note_type(self.span(binder), &ty);
        }
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
                self.let_assert(&pat, pattern, decl, rest);
            } else {
                self.irrefutable(&pat, pattern, rest.map(|rest| (decl, rest)));
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
        captures: Option<&Type>,
    ) -> bool {
        self.header_types(decl, parts, true);
        self.store.level += 1;
        let kinds = binders(scheme);
        let (body, skolems, givens) = self.skolemize(scheme);
        self.record_givens(self.span(decl), &givens);
        let body = match captures {
            Some(c) => Type::Captured {
                ty: Box::new(body),
                captures: Box::new(c.clone()),
                value: false,
            },
            None => body,
        };
        let assumed = self.classes.givens.len();
        self.classes.givens.extend(givens);
        let mark = self.tyvars.len();
        for (name, ty) in skolems.iter().cloned() {
            let kind = kinds
                .iter()
                .find(|b| b.name == name)
                .map_or(super::kinds::Kind::Type, |b| b.kind.clone());
            self.tyvars.push(TyVar { name, ty, kind });
        }
        if let Some(rec) = rec {
            let rec = &mut self.recs[rec];
            rec.scheme = Some(scheme.clone());
            rec.target = Some(body.clone());
            rec.skolems.clone_from(&skolems);
        }
        let saved = mem::take(&mut self.runtime);
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
        let runtime = mem::replace(&mut self.runtime, saved);
        self.classes.givens.truncate(assumed);
        self.tyvars.truncate(mark);
        self.store.level -= 1;
        runtime
    }

    /// A `let` without a signature: a value, whose type is inferred and
    /// generalized when it is built purely. A function here is an error.
    /// `FLICK-T004` for a function without a signature, with the inferred one
    /// as an edit when it is known.
    fn missing_signature(&mut self, name: &str, ty: &Type, span: Span, errors: usize) {
        let incomplete = self.zonk(ty).contains(&mut |t| matches!(t, Type::Error));
        let diagnostic = Diagnostic::error(
            codes::T004,
            format!("function `{name}` needs a signature right above its `let`"),
            span,
        )
        .with_incomplete(incomplete);
        if cfg!(feature = "diagnostic-fixes") {
            let suggestion = self.signature_of(ty);
            let shown = self.show(&suggestion);
            // the edit is offered only for a type inferred without errors
            let line = self.text[..span.start as usize]
                .rfind('\n')
                .map_or(0, |i| i + 1);
            let indent = &self.text[line..span.start as usize];
            let inferred = errors == self.diagnostics.len()
                && self.operators_fit()
                && !suggestion.contains(&mut |t| matches!(t, Type::Error | Type::Meta(_)));
            let at = (inferred && indent.chars().all(|c| c == ' ' || c == '\t')).then(|| {
                (
                    u32::try_from(line).expect("offsets fit in u32"),
                    indent.to_owned(),
                )
            });
            // a number literal's type is known once the declaration settles
            if self.has_open_number(ty) {
                self.signatures.push(PendingSignature {
                    index: self.diagnostics.len(),
                    name: name.to_owned(),
                    ty: ty.clone(),
                    span,
                    at,
                });
                self.diagnostics
                    .push(signature_diagnostic(diagnostic, name, &shown, span, None));
            } else {
                self.diagnostics
                    .push(signature_diagnostic(diagnostic, name, &shown, span, at));
            }
        } else {
            self.diagnostics.push(diagnostic);
        }
    }

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
        let signed = mem::take(&mut self.gadt.signed);
        let errors = self.diagnostics.len();
        let mut redundant = None;
        let ty = match (parts.params.is_empty(), parts.header) {
            (false, _) => self.function(&parts.params, parts.body, None, decl),
            (true, Some(header)) => {
                let ty = self.annotation(header);
                if let Some(b) = parts.body
                    && self.check_exact(b, &ty)
                {
                    redundant = Some(header);
                }
                ty
            }
            (true, None) => parts.body.map_or(Type::Error, |b| self.infer(b)),
        };
        let runtime = mem::replace(&mut self.runtime, saved);
        self.gadt.signed = signed;
        self.store.level -= 1;
        let is_fun = matches!(self.whnf(&ty), Type::Fun { .. });
        // a plugin's code needs no signature: no one reads it. Nor does a local
        // function: nothing outside its body sees it, and it stays of one type
        let local = self.store.level > 0 && !parts.rec;
        let signed = header || local || self.span(decl).start >= self.generated;
        if let (true, Some(written), true) = (is_fun, parts.header, parts.params.is_empty()) {
            let written = self.text_of(written);
            self.error(
                codes::T005,
                self.span(decl),
                format!(
                    "write the type of function `{name}` as a signature above the `let`: `{name} : {written}`"
                ),
            );
        } else if (function || is_fun) && !signed {
            // the suggestion shows the rows the body needs, not their unknowns;
            // only this binding's own: an enclosing row still gathers its calls
            self.close_rows_above(Some(self.store.level));
            self.missing_signature(name, &ty, self.span(decl), errors);
        } else if let Some(header) = redundant {
            self.redundant_annotation(decl, parts, header);
        }
        // a type in a GADT branch may hold by its equalities only
        if runtime || function || !self.store.equalities.is_empty() {
            self.store.lower(&ty);
            ty
        } else {
            // a local atom's uses, all in sight, decide its one type
            if self.store.level > 0 {
                self.lower_atoms(&ty);
            }
            let ty = self.generalize(&ty);
            // an annotation was checked as the value's type already
            if let (None, Some(body)) = (parts.header, parts.body) {
                self.polymorphic_atom(Some(name), &ty, self.span(body));
            }
            ty
        }
    }

    /// Writes the signatures suggested before the number literals in them
    /// were settled, once they are.
    pub fn settle_signatures(&mut self) {
        for pending in mem::take(&mut self.signatures) {
            if self.has_open_number(&pending.ty) {
                self.signatures.push(pending);
                continue;
            }
            let suggestion = self.signature_of(&pending.ty);
            let shown = self.show(&suggestion);
            let incomplete = suggestion.contains(&mut |t| matches!(t, Type::Error));
            let diagnostic = self.diagnostics[pending.index]
                .clone()
                .with_incomplete(incomplete);
            self.diagnostics[pending.index] =
                signature_diagnostic(diagnostic, &pending.name, &shown, pending.span, pending.at);
        }
    }

    /// Whether `ty` holds the type of a number literal not settled yet.
    fn has_open_number(&self, ty: &Type) -> bool {
        self.zonk(ty)
            .contains(&mut |t| matches!(t, Type::Meta(m) if self.store.is_numeric(*m)))
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
    match pattern.kind {
        K::VarPat => out.extend(
            pattern
                .child_tokens()
                .find(|t| t.kind == K::Ident)
                .map(|t| t.text(text).to_owned()),
        ),
        K::RecordPatField if !pattern.has_token(K::Eq) => out.extend(
            pattern
                .child_tokens()
                .find(|t| t.kind == K::Ident)
                .map(|t| t.text(text).to_owned()),
        ),
        _ => {
            for child in pattern.nodes() {
                pattern_names(child, text, out);
            }
        }
    }
}

/// A signature suggested for a function, to write later: the literals of
/// its type are not settled yet.
#[derive(Debug, Clone)]
pub(super) struct PendingSignature {
    index: usize,
    name: String,
    ty: Type,
    span: Span,
    at: Option<(u32, String)>,
}

/// `FLICK-T004` for function `name`, with the edit that inserts `shown` at
/// the start of the line `at` when the type is fully known.
fn signature_diagnostic(
    diagnostic: Diagnostic,
    name: &str,
    shown: &str,
    span: Span,
    at: Option<(u32, String)>,
) -> Diagnostic {
    diagnostic.with_fix(|| {
        let edits = match at {
            Some((at, indent)) => vec![Edit {
                span: Span::new(span.file, at, at),
                replacement: format!("{indent}{name} : {shown}\n"),
            }],
            None => Vec::new(),
        };
        (format!("add `{name} : {shown}`"), edits)
    })
}
