//! Foreign declarations: core contract metadata (`arity`, `catch`,
//! `retention`, `nullable`, `optional`, `reviewed`, `owned`) is validated
//! here, while backend-declared adapter words (such as JS `this`,
//! `cancel signal`, `variadic`, Go equivalents, or third-party words) are
//! parsed against the active backend's descriptor and lowered into
//! `Adapter::words` for backend validation.

mod bridge_shape;
pub(super) mod imply;
mod native;
mod retention;
pub(super) mod spec;
mod value_abi;
#[cfg(not(target_arch = "wasm32"))]
pub(super) mod value_abi_doc;
pub(super) mod value_abi_spec;
pub use imply::{
    AxisTagModel, BackendTagModel, HostTagModel, NativeOverrideEnv, OverrideSelection,
};
pub use native::NativeOverrideInfo;
pub use retention::keeps_for_owner;
pub use spec::{
    AdapterArgKind, AdapterArgSpec, AdapterWordSpec, CORE_FOREIGN_META, CoreForeignMeta,
    ForeignBackendSpec, TypeFormAttrSpec, core_foreign_meta,
};
#[cfg(not(target_arch = "wasm32"))]
pub use value_abi_doc::{DocLocale, render_value_abi_markdown, replace_value_abi_block};
pub use value_abi_spec::{
    AbiConversion, AbiDirection, ValueAbiEntrySpec, ValueAbiId, ValueAbiRefusalSpec, ValueAbiSpec,
};

mod pending;
mod suspends;
pub(super) use pending::PendingForeign;

use std::collections::BTreeSet;
use std::slice;

use super::Checker;
use super::env::Form;
use super::kinds::Kind;
use super::regions::is_elided_row;
use super::ty::{Stage, Type};
use super::typing::{ForeignInfo, ForeignOwnerPolicy, range};
use crate::diagnostics::codes;
use crate::source::Span;
use crate::syntax::cst::{Node, Token, foreign_words};
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::literal;
use crate::types::lang::{self, Lang};
use flick_ir::{AdapterArg, BackendAdapter, Copies, ForeignMode};

/// Pure foreign functions over a capability whose answer does not depend on
/// whether its region is still open (G4 in `docs/formal/regions.md`): each
/// returns a value the capability fixed when it was acquired.
pub const PURE_OVER_CAPABILITY: &[(&str, &str)] = &[
    // the id the system gave the process when it started
    ("ChildProcess", "pid"),
    // the port fixed when the server started listening
    ("HttpApi", "port"),
    ("Rpc", "port"),
    // the address fixed when the server started listening
    ("Http.Server", "address"),
];

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
        let pending = self.read_meta(decl, scheme);
        self.typing.pending_foreigns.push(pending);
    }

    fn finalized_foreign_info(&mut self, mut pending: PendingForeign, scheme: &Type) {
        let decl = pending.decl.clone();
        let decl = &decl;
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
        if stages.contains(&Stage::Sync) && !self.options.stages_by_class {
            self.bad_meta(
                decl,
                "a foreign function returns an Effect, so its arrows cannot be Sync (`->{Atom.Read}`): define the Sync function in Flick around a foreign",
            );
        }
        let mode = if pending.native_override.is_some() {
            suspends::native_override_mode(&body)
        } else {
            self.foreign_mode(decl, &body)
        };
        let stages: Vec<bool> = stages.iter().map(|&s| s != Stage::Pure).collect();
        let dicts = match scheme {
            Type::Forall { constraints, .. } => constraints
                .iter()
                .filter(|c| super::typing::passes_dictionary(c))
                .count(),
            _ => constraints,
        };
        self.validate_prepared_meta(&mut pending, mode, &body, stages.len());
        let mut info = pending.info;
        let retention = pending.retention;
        info.stages.clone_from(&stages);
        info.dicts = dicts;
        info.scoped_stages = self.foreign_scoped_stages(&body);
        info.mode = mode;
        let arity = info.js_arity();
        if let Ok(result) = self.foreign_owner_result(&body, arity, params.is_empty()) {
            info.owner_result = result;
        } else {
            pending.info = info;
            self.defer_foreign(pending, scheme);
            return;
        }
        let span = pending.span;
        self.check_meta(&info, &params, &body, span);
        info.list_params = params
            .iter()
            .take(arity)
            .map(|p| self.whnf(p).spine().0.is_lang(Lang::List))
            .collect();
        info.adapter.result.unit = self
            .call_result(&body, arity)
            .is_some_and(|r| r.is_lang(Lang::Unit));
        info.adapter.callbacks = self.foreign_callbacks(&params, arity);
        info.adapter.copies = self.copies(&params, &body, arity, &info.adapter.callbacks);
        info.adapter.shapes = self.param_shapes(&params, arity);
        info.adapter.result_shape = self
            .call_result(&body, arity)
            .or_else(|| {
                (arity == 0 && mode == ForeignMode::Pure).then(|| self.whnf_captured(&body).0)
            })
            .map_or(flick_ir::Shape::Data, |r| self.shape(&r, 0));
        self.check_foreign_regions(
            decl, scheme, &body, &binders, &params, &stages, retention, span,
        );
        let value_abi_spec = if pending.native_override.is_some() {
            self.options
                .native_env()
                .active_backend_spec(self.options.foreign_backend)
                .clone()
        } else {
            self.options.foreign_backend.clone()
        };
        self.check_foreign_value_abi(
            pending.native_override.is_none().then_some(decl),
            &info,
            &params,
            &body,
            &value_abi_spec,
            span,
        );
        if let Some((override_range, backend_id, enabled)) = pending.native_override {
            self.typing.native_overrides.push((
                decl.range(),
                NativeOverrideInfo {
                    override_range,
                    info,
                    backend_id,
                    enabled,
                },
            ));
        } else {
            self.typing.foreigns.insert(range(span), info);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn check_foreign_regions(
        &mut self,
        decl: &Node,
        scheme: &Type,
        body: &Type,
        binders: &[super::ty::Binder],
        params: &[Type],
        stages: &[bool],
        retention: Option<Retention>,
        span: Span,
    ) {
        let scoped: Vec<String> = binders
            .iter()
            // a hidden row read into the signature is no capability it took
            .filter(|b| matches!(b.kind, Kind::Region | Kind::CaptureRow) && !is_elided_row(b))
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
            self.owner_retention(scheme, body, span);
        }
        // G4: a parameter that reaches a region its result does not carry is
        // used by the call, so the call needs a stage that keeps it live; a
        // capture row is no use of its own: a pure callback uses nothing (T4′)
        let result = self.result_of(body);
        let used: Vec<String> = binders
            .iter()
            .filter(|b| b.kind == Kind::Region && !mentions(&result, slice::from_ref(&b.name)))
            .map(|b| b.name.clone())
            .collect();
        if let Some(i) = params.iter().position(|p| mentions(p, &used))
            && !stages.iter().skip(i).any(|runs| *runs)
            && !PURE_OVER_CAPABILITY.contains(&(
                self.env.module.as_str(),
                super::toplevel::foreign_name(decl, self.text).unwrap_or_default(),
            ))
        {
            let shown = self.show(&params[i]);
            self.error(
                codes::F009,
                span,
                format!("`{shown}` reaches the region of a scope, but the foreign function runs no stage, so a call after the scope closed would read what it released: give it a stage, `->{{}}` and `foreign effect`"),
            );
        }
    }

    fn foreign_callbacks(&self, params: &[Type], arity: usize) -> Vec<Option<usize>> {
        params
            .iter()
            .take(arity)
            .map(|param| {
                // A stage that runs gives its value, not more arguments to take.
                let stages = self.arrow_stages(param);
                let n = stages
                    .iter()
                    .position(|stage| *stage != Stage::Pure)
                    .map_or(stages.len(), |index| index + 1);
                (n > 0).then_some(n)
            })
            .collect()
    }
}

include!("ffi/metadata.rs");

impl Checker<'_> {
    /// What a curried function type returns after its last arrow.
    fn result_of(&self, ty: &Type) -> Type {
        let mut ty = ty.clone();
        while let Type::Fun { result, .. } = self.whnf_captured(&ty).0 {
            ty = *result;
        }
        ty
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

    fn foreign_owner_result(
        &self,
        body: &Type,
        arity: usize,
        constant: bool,
    ) -> Result<Option<Box<ForeignOwnerPolicy>>, ()> {
        if constant {
            self.foreign_owner_policy(body)
        } else {
            self.call_result(body, arity.max(1))
                .map_or(Ok(None), |result| self.foreign_owner_policy(&result))
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

    /// `catch f`: `f : HostErr -> E` where `HostErr` comes from the active
    /// backend's `value_abi.catch_lang_item` (default `JsValue`), `E` is an
    /// error of the row the call fails with, and no capability of a scope is in it.
    fn infer_catch_mapping(&mut self, mapping: &Node, spec: &ForeignBackendSpec) -> Option<Type> {
        let span = self.span(mapping);
        let error = self.fresh();
        let catch_lang = spec
            .value_abi
            .as_ref()
            .and_then(|abi| abi.catch_lang_item.as_deref())
            .and_then(Lang::from_name)
            .unwrap_or(Lang::JsValue);
        let expected = Type::pure_fun(Type::lang(catch_lang), error.clone());
        let actual = self.infer(mapping);
        if let Err(m) = self.unify(&actual, &expected) {
            self.mismatch(&m, &actual, &expected, span);
            return None;
        }
        Some(error)
    }

    fn check_catch_mapping(&mut self, mapping: &Node, body: &Type, error: &Type) {
        let span = self.span(mapping);
        let row = self.error_row(body);
        let error = self.zonk(error);
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
        Type::Without { row, .. } => mentions(row, names),
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

/// Returns the raw token slice when `node` is a single plain string literal
/// (`K::Literal` with one non-trivia `K::String` token and no child nodes),
/// accepting plain, raw (`r"..."`, `r#"..."#`), and multiline (`"""..."""`)
/// string literals while rejecting postfix expressions (`"a".b`, `"a"()`) and
/// string interpolation.
fn plain_string_literal<'a>(text: &'a str, node: &Node) -> Option<&'a str> {
    if node.kind != K::Literal || node.nodes().next().is_some() {
        return None;
    }
    let mut tokens = node.child_tokens();
    let token = tokens.next()?;
    if token.kind != K::String || tokens.next().is_some() {
        return None;
    }
    Some(token.text(text))
}

#[cfg(test)]
#[path = "ffi/metadata_tests.rs"]
mod metadata_tests;
#[cfg(test)]
#[path = "ffi/pending_tests.rs"]
mod pending_tests;
#[cfg(test)]
#[path = "ffi/value_abi_tests.rs"]
mod value_abi_tests;
