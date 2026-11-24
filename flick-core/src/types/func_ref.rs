//! `FuncRef (a -> b)`: a function without captures, which a host without
//! closures takes as a plain function pointer. Where one is expected, the
//! value is checked against `a -> b` and must then be closed: the name of a
//! top-level function that takes no class dictionary at its type, or a
//! lambda that uses no local and no dictionary of the code around it. A
//! dictionary counts as a capture: it is a value the closure would carry.

use super::Checker;
use super::classes::Dict;
use super::ty::Type;
use super::typing::{Range, Ref};
use crate::diagnostics::codes;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::Lang;
use crate::types::typing::passes_dictionary;

/// A value accepted as a `FuncRef` whose dictionaries are known only once
/// the module is solved.
#[derive(Debug, Clone)]
pub(super) enum FuncRefSite {
    /// The name of a top-level function: any dictionary passed at it is
    /// one the reference would carry.
    Name { range: Range, name: String },
    /// A lambda checked where the code around it assumes the dictionaries
    /// `outer`: one of them used inside is a capture.
    Lambda { range: Range, outer: Vec<Dict> },
}

const CLOSED: &str = "a `FuncRef` is a top-level function or a lambda that captures nothing";

impl Checker<'_> {
    /// The function type of a `FuncRef`.
    pub fn func_ref_parts(&self, t: &Type) -> Option<Type> {
        let t = self.whnf_captured(t).0;
        let (head, args) = t.spine();
        match args[..] {
            [f] if head.is_lang(Lang::FuncRef) => Some(f.clone()),
            _ => None,
        }
    }

    /// Whether `node` is checked as a `FuncRef` against `target`: a block,
    /// an `if` and a `match` pass the expected type on to what they give.
    /// This and `check_func_ref` keep their locals out of `check_value`,
    /// whose frame every nested expression adds to the stack.
    #[inline(never)]
    pub(super) fn expects_func_ref(&self, node: &Node, target: &Type) -> bool {
        !matches!(node.kind, K::Block | K::IfExpr | K::MatchExpr)
            && self.func_ref_parts(target).is_some()
    }

    /// `node` where `expected`, whose head normal form `target` is a
    /// `FuncRef`, is expected.
    #[inline(never)]
    pub(super) fn check_func_ref(&mut self, node: &Node, target: &Type, expected: &Type) -> Type {
        let fn_ty = &self
            .func_ref_parts(target)
            .expect("`expects_func_ref` saw a `FuncRef`");
        let mut node = node;
        while node.kind == K::ParenExpr {
            let Some(inner) = node.nodes().next() else {
                return expected.clone();
            };
            node = inner;
        }
        let span = self.span(node);
        if !self.func_ref_type(fn_ty, span) {
            self.check_value(node, fn_ty);
            return expected.clone();
        }
        if node.kind == K::FunExpr {
            let (locals, uses) = (self.cx.locals.len(), self.uses_mark());
            let outer = self.outer_dictionaries();
            self.check_value(node, fn_ty);
            if let Some((name, _)) = self.held_since(uses, locals).into_iter().next() {
                let name = name.unwrap_or_default();
                let at = self.capture_span(node, &name).unwrap_or(span);
                self.error(
                    codes::F015,
                    at,
                    format!(
                        "the lambda captures `{name}`, but {CLOSED}: pass `{name}` as an argument"
                    ),
                );
            }
            let range = node.range();
            self.func_refs.push(FuncRefSite::Lambda { range, outer });
            return expected.clone();
        }
        let actual = self.infer(node);
        if matches!(self.whnf(&actual), Type::Error) {
            return actual;
        }
        // a `FuncRef` already, as a parameter or a top-level value of that type
        if self.func_ref_parts(&actual).is_some() {
            self.expect(&actual, expected, span);
            return actual;
        }
        self.expect(&actual, fn_ty, span);
        match (node.kind, self.typing.names.get(&node.range())) {
            (
                K::NameExpr,
                Some(Ref::Value { .. } | Ref::Constructor(_) | Ref::Operation { .. }),
            ) => {
                let (range, name) = (node.range(), node.text(self.text).trim().to_owned());
                self.func_refs.push(FuncRefSite::Name { range, name });
            }
            (K::NameExpr, Some(Ref::Local)) => {
                let name = node.text(self.text).trim().to_owned();
                self.error(
                    codes::F015,
                    span,
                    format!(
                        "`{name}` is a local, which may capture what is around it, but {CLOSED}"
                    ),
                );
            }
            (K::NameExpr, Some(Ref::Method { name, .. })) => {
                let name = name.clone();
                self.error(
                    codes::F015,
                    span,
                    format!("`{name}` is a class method, which needs its instance's dictionary, but {CLOSED}: call it from a lambda, `fun x -> {name} x`"),
                );
            }
            _ => self.error(
                codes::F015,
                span,
                format!(
                    "this makes a closure at run time, as a partial application does, but {CLOSED}"
                ),
            ),
        }
        expected.clone()
    }

    /// Refuses a `FuncRef` of a type that is no function, or of a function
    /// that takes class dictionaries; `false` after a diagnostic.
    fn func_ref_type(&mut self, fn_ty: &Type, span: Span) -> bool {
        match self.whnf(fn_ty) {
            Type::Forall { constraints, .. } if constraints.iter().any(passes_dictionary) => {
                let shown = self.show(fn_ty);
                self.error(
                    codes::F015,
                    span,
                    format!("`{shown}` takes class dictionaries, which a `FuncRef` cannot carry"),
                );
                false
            }
            ty @ (Type::Con { .. } | Type::App { .. } | Type::Tuple(_) | Type::Record(_)) => {
                let shown = self.show(&ty);
                self.error(
                    codes::F015,
                    span,
                    format!("a `FuncRef` refers to a function, not to `{shown}`"),
                );
                false
            }
            _ => true,
        }
    }

    /// Whether the lambda `node` was checked as a `FuncRef`, where the
    /// function its body calls may not stand for it.
    pub(super) fn is_func_ref_lambda(&self, node: &Node) -> bool {
        let at = node.range();
        (self.func_refs.iter())
            .any(|site| matches!(site, FuncRefSite::Lambda { range, .. } if *range == at))
    }

    /// The dictionaries the code being checked assumes: of its signature,
    /// of the patterns it is in and of the modules it opened.
    fn outer_dictionaries(&self) -> Vec<Dict> {
        if self.classes.givens.is_empty() && self.classes.local.is_empty() {
            return Vec::new();
        }
        self.closure(&self.classes.givens)
            .into_iter()
            .map(|(_, dict)| dict)
            .filter(|dict| matches!(dict, Dict::Given(_) | Dict::Field(_) | Dict::Opened { .. }))
            .collect()
    }

    /// Where the lambda `node` uses the local `name` it captures.
    fn capture_span(&self, node: &Node, name: &str) -> Option<Span> {
        let (start, end) = node.range();
        let outside = |at: &Range| at.0 < start || at.0 >= end;
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            let range = n.range();
            if n.kind == K::NameExpr
                && matches!(self.typing.names.get(&range), Some(Ref::Local))
                && n.text(self.text).trim() == name
                && self.typing.binders.get(&range).is_some_and(outside)
            {
                return Some(Span::new(self.file, range.0, range.1));
            }
            let children: Vec<&Node> = n.nodes().collect();
            stack.extend(children.into_iter().rev());
        }
        None
    }

    /// Refuses the `FuncRef`s of the module that carry a dictionary, now
    /// that every dictionary is known.
    pub(super) fn func_ref_dictionaries(&mut self) {
        // the lambdas stay for the lints, which read them after
        for site in self.func_refs.clone() {
            let (range, outer) = match &site {
                FuncRefSite::Name { range, .. } => (*range, None),
                FuncRefSite::Lambda { outer, .. } if outer.is_empty() => continue,
                FuncRefSite::Lambda { range, outer } => (*range, Some(outer)),
            };
            let found = self
                .typing
                .dictionaries_in(range)
                .find(|(_, dict)| match outer {
                    None => !matches!(dict, Dict::Error),
                    Some(outer) => carries(dict, outer),
                });
            let Some((at, dict)) = found.map(|(at, dict)| (at, dict.clone())) else {
                continue;
            };
            let message = match (&site, &dict) {
                (FuncRefSite::Name { name, .. }, _) => format!(
                    "`{name}` takes class dictionaries at this type, which a `FuncRef` cannot carry: call it from a lambda, `fun x -> {name} x`"
                ),
                (FuncRefSite::Lambda { .. }, Dict::Given(given)) => format!(
                    "the lambda uses the dictionary of `{given}` that the code around it assumes, but {CLOSED}"
                ),
                (FuncRefSite::Lambda { .. }, _) => format!(
                    "the lambda uses a class dictionary of the code around it, but {CLOSED}"
                ),
            };
            self.error(codes::F015, Span::new(self.file, at.0, at.1), message);
        }
    }
}

/// Whether `dict` is built from one of the assumptions `outer`.
fn carries(dict: &Dict, outer: &[Dict]) -> bool {
    if outer.contains(dict) {
        return true;
    }
    match dict {
        Dict::Super { dict, .. } => carries(dict, outer),
        Dict::Instance { args: dicts, .. } | Dict::Structural { fields: dicts, .. } => {
            dicts.iter().any(|d| carries(d, outer))
        }
        _ => false,
    }
}
