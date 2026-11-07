//! Foreign declarations: their metadata, checked against the signature and
//! the mode, becomes the calling convention code generation follows.
//!
//! | Metadata | Meaning |
//! | - | - |
//! | `arity n` | `n` positional JS arguments; `arity 0` takes the `()` of a first `Unit` parameter |
//! | `this` | the first argument is the receiver: `target.call(a0, …)` |
//! | `catch f` | a throw or rejection becomes the typed error `f : JsValue -> E`, `E` in the error row |
//! | `cancel signal` | an async call gets an `AbortSignal` last, aborted on interruption |
//! | `nullable` | the result is `Option a`: `null` and `undefined` are `None` |
//! | `optional i` | argument `i` is `Option a`: `None` is passed as `undefined` |
//! | `variadic` | the last argument is a `List a` spread into the call |
//! | `retention call` / `retention owner` | what a parameter that may hold a scope's capabilities is kept for |

mod retention;
pub use retention::keeps_for_owner;

mod suspends;

use super::Checker;
use super::env::Form;
use super::kinds::Kind;
use super::ty::{Stage, Type};
use super::typing::{ForeignInfo, range};
use crate::diagnostics::codes;
use crate::ir::{Copies, ForeignMode};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::{self, Lang};

/// What a foreign function keeps of its arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retention {
    /// Nothing beyond what its result's type shows.
    Call,
    /// Until the owner of the region it was given closes.
    Owner,
}

impl Checker<'_> {
    /// Checks the metadata of the foreign declaration `decl` of type
    /// `scheme` and records its calling convention.
    pub fn foreign_info(&mut self, decl: &Node, scheme: &Type) {
        let (binders, constraints, body) = match scheme {
            Type::Forall {
                vars,
                constraints,
                body,
            } => (vars.clone(), constraints.len(), (**body).clone()),
            other => (Vec::new(), 0, other.clone()),
        };
        let params = self.params_of(&body);
        let stages = self.arrow_stages(scheme);
        if stages.contains(&Stage::Sync) && !super::ty::stages_by_class() {
            self.bad_meta(
                decl,
                "a foreign function returns an Effect, so its arrows cannot be Sync (`->{Atom.Read;}`): define the Sync function in Flick around a foreign",
            );
        }
        let mode = self.foreign_mode(decl, &body);
        let stages: Vec<bool> = stages.iter().map(|&s| s != Stage::Pure).collect();
        let dicts = match scheme {
            Type::Forall { constraints, .. } => constraints
                .iter()
                .filter(|c| super::typing::passes_dictionary(c))
                .count(),
            _ => constraints,
        };
        let mut info = ForeignInfo {
            stages: stages.clone(),
            dicts,
            ..ForeignInfo::default()
        };
        info.mode = mode;
        let retention = self.read_meta(decl, mode, &body, &mut info);
        let arity = info.js_arity();
        let span = self.span(decl);
        self.check_meta(&info, &params, &body, span);
        info.adapter.result.unit = self
            .call_result(&body, arity)
            .is_some_and(|r| r.is_lang(Lang::Unit));
        info.adapter.callbacks = params
            .iter()
            .take(arity)
            .map(|p| {
                // a stage that runs gives its value, not more arguments to take
                let stages = self.arrow_stages(p);
                let n =
                    (stages.iter().position(|s| *s != Stage::Pure)).map_or(stages.len(), |i| i + 1);
                (n > 0).then_some(n)
            })
            .collect();
        info.adapter.copies = self.copies(&params, &body, arity, &info.adapter.callbacks);
        let scoped: Vec<String> = binders
            .iter()
            .filter(|b| matches!(b.kind, Kind::Region | Kind::CaptureRow))
            .map(|b| b.name.clone())
            .collect();
        if retention.is_none()
            && let Some(param) = params.iter().find(|p| mentions(p, &scoped))
        {
            let shown = self.show(param);
            self.error(
                codes::F002,
                span,
                format!("`{shown}` may hold the capabilities of a scope, but the foreign function has no `retention`: write `retention call` if it keeps nothing past the call"),
            );
        }
        if retention == Some(Retention::Owner) {
            self.owner_retention(scheme, &body, span);
        }
        self.typing.foreigns.insert(range(span), info);
    }

    /// Reads the metadata lines of `decl` into `info`; the retention, if
    /// written.
    fn read_meta(
        &mut self,
        decl: &Node,
        mode: ForeignMode,
        body: &Type,
        info: &mut ForeignInfo,
    ) -> Option<Retention> {
        let stages = info.stages.clone();
        let mut retention = None;
        let meta: Vec<&Node> = decl
            .nodes()
            .filter(|n| n.kind == K::ForeignMeta)
            .flat_map(Node::nodes)
            .collect();
        for item in &meta {
            let Some(name) = item.child_tokens().find(|t| t.kind == K::Ident) else {
                continue;
            };
            let args: Vec<&Node> = item.nodes().collect();
            let word = |i: usize| args.get(i).map(|n| n.text(self.text).trim().to_owned());
            let number = |i: usize| word(i).and_then(|w| w.parse::<usize>().ok());
            match name.text(self.text) {
                "arity" => match number(0) {
                    Some(n) if n <= stages.len() => info.arity = Some(n),
                    _ => self.bad_meta(item, &format!(
                        "`arity` takes the number of JS arguments, at most the {} parameters of the signature",
                        stages.len()
                    )),
                },
                "this" => info.adapter.receiver = true,
                "reviewed" if !word(0).is_some_and(|w| w.starts_with('"')) => self.bad_meta(
                    item,
                    "`reviewed` takes the package and fingerprint the binding was checked against, as a string",
                ),
                "nullable" => info.adapter.result.nullable = true,
                "variadic" => info.adapter.variadic = true,
                "optional" => match number(0) {
                    Some(i) => info.adapter.optional.push(i),
                    None => self.bad_meta(item, "`optional` takes the position of an `Option` argument, from 0"),
                },
                "cancel" => match word(0).as_deref() {
                    Some("signal") if mode == ForeignMode::Async => info.adapter.cancel = true,
                    Some("signal") => self.bad_meta(item, "`cancel signal` is for a call whose row has `Async`: only a promise can be aborted"),
                    _ => self.bad_meta(item, "write `cancel signal`"),
                },
                "retention" => match word(0).as_deref() {
                    Some("call") => retention = Some(Retention::Call),
                    Some("owner") => retention = Some(Retention::Owner),
                    _ => self.bad_meta(item, "write `retention call` or `retention owner`"),
                },
                "catch" => match args.first() {
                    Some(mapping) if mode != ForeignMode::Pure => {
                        self.catch_mapping(mapping, body);
                        info.catch = Some(mapping.range());
                    }
                    Some(_) => self.bad_meta(item, "a `foreign pure` call cannot throw: `catch` needs another mode"),
                    None => self.bad_meta(item, "`catch` takes the function that maps a JS exception to an error"),
                },
                _ => {}
            }
        }
        retention
    }

    /// Checks the metadata in `info` against the parameters of the signature.
    fn check_meta(&mut self, info: &ForeignInfo, params: &[Type], body: &Type, span: Span) {
        let arity = info.js_arity();
        let stages = &info.stages;
        if arity == 0
            && !stages.is_empty()
            && !params.first().is_some_and(|p| p.is_lang(Lang::Unit))
        {
            self.error(
                codes::F001,
                span,
                "`arity 0` drops the first argument, which must be `()`".to_owned(),
            );
        }
        if info.adapter.receiver && arity == 0 {
            self.error(
                codes::F001,
                span,
                "`this` needs an argument to be the receiver".to_owned(),
            );
        }
        for &i in &info.adapter.optional {
            if !params
                .get(i)
                .is_some_and(|p| self.whnf(p).spine().0.is_lang(Lang::Option))
                || i >= arity
            {
                self.error(
                    codes::F001,
                    span,
                    format!("`optional {i}` names an argument of type `Option a`"),
                );
            }
        }
        if info.adapter.variadic
            && (arity == 0 || !self.whnf(&params[arity - 1]).spine().0.is_lang(Lang::List))
        {
            self.error(
                codes::F001,
                span,
                "`variadic` spreads a last argument of type `List a`".to_owned(),
            );
        }
        if info.adapter.result.nullable
            && !self
                .call_result(body, arity)
                .is_some_and(|r| self.whnf(&r).spine().0.is_lang(Lang::Option))
        {
            self.error(
                codes::F001,
                span,
                "`nullable` makes the result an `Option a`".to_owned(),
            );
        }
    }

    /// The parameters of a curried function type, one per arrow.
    fn params_of(&self, ty: &Type) -> Vec<Type> {
        let mut out = Vec::new();
        let mut ty = ty.clone();
        while let Type::Fun { param, result, .. } = self.whnf_captured(&ty).0 {
            out.push(*param);
            ty = *result;
        }
        out
    }

    /// Where the call passes Flick data: its first `arity` parameters, the
    /// result and, for a callback, what JS gives it and what it returns. The
    /// result of a callback's runtime stage is an `Effect`, not data.
    fn copies(
        &self,
        params: &[Type],
        body: &Type,
        arity: usize,
        callbacks: &[Option<usize>],
    ) -> Copies {
        let callback = |p: &Type, n: usize| {
            let args = self
                .params_of(p)
                .iter()
                .take(n)
                .map(|a| self.holds_data(a))
                .collect();
            let pure = self.arrow_stages(p).iter().all(|s| *s == Stage::Pure);
            let result = pure && self.call_result(p, n).is_some_and(|r| self.holds_data(&r));
            (args, result)
        };
        Copies {
            args: params
                .iter()
                .take(arity)
                .map(|p| self.holds_data(p))
                .collect(),
            result: self
                .call_result(body, arity)
                .is_some_and(|r| self.holds_data(&r)),
            callbacks: params
                .iter()
                .zip(callbacks)
                .map(|(p, n)| n.map(|n| callback(p, n)))
                .collect(),
        }
    }

    /// Whether a value of `ty` may be Flick data JS could change in place:
    /// a list, tuple, record or constructed value, or a type variable that
    /// may stand for one. Functions, `JsValue` and opaque types are not.
    fn holds_data(&self, ty: &Type) -> bool {
        match self.whnf_captured(ty).0 {
            Type::Tuple(_) | Type::Record(_) | Type::Var(_) | Type::Skolem(_) => true,
            Type::App { head, .. } => self.holds_data(&head),
            Type::Forall { body, .. } => self.holds_data(&body),
            Type::Con { module, name } => {
                lang::is(Lang::List, &module, &name)
                    || matches!(
                        self.env.type_def(&module, &name).map(|d| &d.form),
                        Some(Form::Adt { .. } | Form::Error | Form::Alias { .. })
                    )
            }
            _ => false,
        }
    }

    /// The result of a call with `arity` arguments.
    fn call_result(&self, ty: &Type, arity: usize) -> Option<Type> {
        let mut ty = ty.clone();
        for _ in 0..arity.max(1) {
            match self.whnf_captured(&ty).0 {
                Type::Fun { result, .. } => ty = *result,
                _ => return None,
            }
        }
        Some(self.whnf_captured(&ty).0)
    }

    /// `catch f`: `f : JsValue -> E` with `E` an error of the row the call
    /// fails with, and no capability of a scope in it.
    fn catch_mapping(&mut self, mapping: &Node, body: &Type) {
        let span = self.span(mapping);
        let error = self.fresh();
        let expected = Type::pure_fun(Type::lang(Lang::JsValue), error.clone());
        let actual = self.infer(mapping);
        if let Err(m) = self.unify(&actual, &expected) {
            self.mismatch(&m, &actual, &expected, span);
            return;
        }
        let row = self.error_row(body);
        let error = self.zonk(&error);
        let allowed = row.is_some_and(|row| {
            let norm = self.store.normalize(&row);
            norm.entries.iter().any(|(_, t)| self.zonk(t) == error)
        });
        if !allowed {
            let shown = self.show(&error);
            self.error(
                codes::F003,
                span,
                format!(
                    "`catch` makes `{shown}`, which the error row of the signature does not list"
                ),
            );
        } else if !self.captures_of(&error).entries.is_empty() {
            self.error(
                codes::F003,
                span,
                "the error `catch` makes holds a capability of a scope, which must not leave it"
                    .to_owned(),
            );
        }
    }

    /// The error row of the first runtime arrow of `ty`.
    fn error_row(&self, ty: &Type) -> Option<Type> {
        let mut ty = ty.clone();
        loop {
            match self.whnf_captured(&ty).0 {
                Type::Fun {
                    effects: Some(e), ..
                } => return Some(e.errors),
                Type::Fun { result, .. } => ty = *result,
                _ => return None,
            }
        }
    }

    fn bad_meta(&mut self, item: &Node, message: &str) {
        self.error(codes::F001, self.span(item), message.to_owned());
    }
}

/// Whether `t` mentions one of the variables `names`.
fn mentions(t: &Type, names: &[String]) -> bool {
    match t {
        Type::Var(v) => names.contains(v),
        Type::App { head, arg } => mentions(head, names) || mentions(arg, names),
        Type::Fun {
            param,
            effects,
            result,
        } => {
            mentions(param, names)
                || mentions(result, names)
                || effects
                    .as_ref()
                    .is_some_and(|e| mentions(&e.services, names) || mentions(&e.errors, names))
        }
        Type::Tuple(items) => items.iter().any(|i| mentions(i, names)),
        Type::Record(row) | Type::Row(row) => {
            row.entries.iter().any(|(_, t)| mentions(t, names))
                || row.tail.as_ref().is_some_and(|t| mentions(t, names))
        }
        Type::Union(a, b) => mentions(a, names) || mentions(b, names),
        Type::Captured { ty, captures, .. } | Type::Bounded { ty, captures } => {
            mentions(ty, names) || mentions(captures, names)
        }
        Type::Forall { body, .. } => mentions(body, names),
        _ => false,
    }
}

impl ForeignInfo {
    /// The JS arguments: `arity n`, or the stages up to the first runtime one.
    pub fn js_arity(&self) -> usize {
        self.arity.unwrap_or_else(|| {
            self.stages
                .iter()
                .position(|&runtime| runtime)
                .map_or(self.stages.len(), |i| i + 1)
        })
    }
}
