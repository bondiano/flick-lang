//! Helpers for attaching interned [`flick_ir::TypeRef`]s, type arguments,
//! and pattern binder types during IR lowering.

#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;

#[cfg(not(target_arch = "wasm32"))]
use flick_ir::{IrType, TypeInterner};
use flick_ir::{Lift, TypeRef};

use super::Lower;
use crate::effects::key;
use crate::ir::{Binding, Expr, Foreign, Param, Pat};
use crate::syntax::cst::{Node, foreign_words};
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::literal;
use crate::types::Range;
#[cfg(not(target_arch = "wasm32"))]
use crate::types::lang;
use crate::types::lang::Lang;

impl Lower<'_> {
    pub(super) fn foreign(&mut self, decl: &Node) -> Option<Foreign> {
        let name = foreign_words(decl).1?.text(self.text).to_owned();
        let source = decl.nodes().find(|n| n.kind == K::ForeignSource)?;
        let strings: Vec<String> = source
            .child_tokens()
            .filter(|t| t.kind == K::String)
            .map(|t| literal::string_value(t.text(self.text)))
            .collect();
        let (module, export) = match &strings[..] {
            [export] => (None, export.clone()),
            [module, export] => (Some(module.clone()), export.clone()),
            _ => return None,
        };
        let info = self
            .typing
            .foreigns
            .get(&decl.range())
            .cloned()
            .unwrap_or_default();
        let catch = info.catch.and_then(|at| {
            let node = decl
                .nodes()
                .filter(|n| n.kind == K::ForeignMeta)
                .flat_map(Node::nodes)
                .flat_map(Node::nodes)
                .find(|n| n.range() == at)?;
            Some(self.expr(node))
        });
        let (type_params, _, ty) = self.ir_top_scheme(&name);
        Some(Foreign {
            source_name: None,
            name,
            mode: info.mode,
            binding: Binding {
                from: module.map(|spec| key(&spec).into_owned()),
                name: export,
            },
            stages: info.stages.clone(),
            dicts: info.dicts,
            arity: info.js_arity(),
            adapter: info.adapter,
            catch,
            type_params,
            ty,
        })
    }
    /// Interned IR type recorded by the checker at `key`, if available.
    #[cfg_attr(target_arch = "wasm32", allow(clippy::unused_self))]
    pub(super) fn ir_type(&self, key: Range) -> Option<TypeRef> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.typing.ir_values.get(&key).cloned()
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = key;
            None
        }
    }

    #[cfg_attr(target_arch = "wasm32", allow(clippy::unused_self))]
    pub(super) fn ir_coercion_target(&self, key: Range) -> Option<TypeRef> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.typing.ir_coercion_targets.get(&key).cloned()
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = key;
            None
        }
    }

    #[cfg_attr(target_arch = "wasm32", allow(clippy::unused_self))]
    pub(super) fn ir_type_args(&self, key: Range) -> Vec<TypeRef> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.typing
                .ir_type_args
                .get(&key)
                .cloned()
                .unwrap_or_default()
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = key;
            Vec::new()
        }
    }

    #[cfg_attr(target_arch = "wasm32", allow(clippy::unused_self))]
    pub(super) fn ir_con_fields(&self, key: Range) -> Option<Vec<TypeRef>> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.typing.ir_con_fields.get(&key).cloned()
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = key;
            None
        }
    }

    #[cfg_attr(target_arch = "wasm32", allow(clippy::unused_self))]
    pub(super) fn ir_top_scheme(
        &self,
        name: &str,
    ) -> (Vec<String>, Vec<flick_ir::Constraint>, Option<TypeRef>) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            match self.typing.ir_top_schemes.get(name) {
                Some((tps, cs, ty)) => (tps.clone(), cs.clone(), Some(ty.clone())),
                None => (Vec::new(), Vec::new(), None),
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = name;
            (Vec::new(), Vec::new(), None)
        }
    }

    #[cfg_attr(target_arch = "wasm32", allow(clippy::unused_self))]
    pub(super) fn ir_local_scheme(
        &self,
        key: Range,
    ) -> (Vec<String>, Vec<flick_ir::Constraint>, Option<TypeRef>) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            match self.typing.ir_local_schemes.get(&key) {
                Some((tps, cs, ty)) => (tps.clone(), cs.clone(), Some(ty.clone())),
                None => (Vec::new(), Vec::new(), None),
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = key;
            (Vec::new(), Vec::new(), None)
        }
    }

    #[cfg_attr(target_arch = "wasm32", allow(clippy::unused_self))]
    pub(super) fn ir_ctor_fields(&self, name: &str) -> Vec<TypeRef> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.typing
                .ir_ctor_fields
                .get(name)
                .cloned()
                .unwrap_or_default()
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = name;
            Vec::new()
        }
    }

    #[cfg_attr(target_arch = "wasm32", allow(clippy::unused_self))]
    pub(super) fn ir_type_decls(&self) -> Vec<flick_ir::TypeDecl> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.typing.ir_type_decls.clone()
        }
        #[cfg(target_arch = "wasm32")]
        {
            Vec::new()
        }
    }

    /// Lowers a pattern and returns `(pat, binder_types)` with the unique
    /// lowered name and interned type of every variable bound by the pattern.
    pub(super) fn pattern_typed(&mut self, node: &Node) -> (Pat, Vec<(String, TypeRef)>) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let prev = self.collecting_binders.replace(Vec::new());
            let pat = self.pattern(node);
            let binders = self.collecting_binders.take().unwrap_or_default();
            self.collecting_binders = prev;
            (pat, binders)
        }
        #[cfg(target_arch = "wasm32")]
        {
            (self.pattern(node), Vec::new())
        }
    }

    /// Records the type of a freshly bound pattern variable if `pattern_typed`
    /// is active.
    #[cfg_attr(target_arch = "wasm32", allow(clippy::unused_self))]
    pub(super) fn record_pat_binder_type(&mut self, unique: &str, key: Range) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(binders) = &mut self.collecting_binders
            && let Some(ty) = self.typing.ir_values.get(&key)
        {
            binders.push((unique.to_owned(), ty.clone()));
        }
        #[cfg(target_arch = "wasm32")]
        let _ = (unique, key);
    }

    /// Type arguments for a `Global` reference at `key`.
    ///
    /// Uses the checker's recorded instantiation arguments when present; falls
    /// back to matching the global's lowered scheme against `expr_ty` for
    /// monomorphic self-recursion inside an unsigned `let rec` before
    /// generalization.
    #[cfg_attr(target_arch = "wasm32", allow(clippy::unused_self))]
    pub(super) fn global_type_args(
        &self,
        key: Range,
        module: &str,
        name: &str,
        expr_ty: Option<&TypeRef>,
    ) -> Vec<TypeRef> {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (key, module, name, expr_ty);
            Vec::new()
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(args) = self.typing.ir_type_args.get(&key) {
                return args.clone();
            }
            if module != self.module {
                return Vec::new();
            }
            let Some((type_params, _, decl_ty)) = self.typing.ir_top_schemes.get(name) else {
                return Vec::new();
            };
            if type_params.is_empty() {
                return Vec::new();
            }
            let mut matched: HashMap<&str, TypeRef> = HashMap::new();
            if let Some(actual) = expr_ty {
                match_type_args(decl_ty, type_params, actual, &mut matched);
            }
            type_params
                .iter()
                .map(|p| {
                    matched
                        .get(p.as_str())
                        .cloned()
                        .unwrap_or_else(|| TypeRef::new(IrType::Var(p.clone())))
                })
                .collect()
        }
    }
}

/// Applies a stage `Lift` to `ty`, producing the lifted function or `Eff` type.
pub(super) fn apply_lift_type(ty: &TypeRef, lift: &Lift) -> TypeRef {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = lift;
        ty.clone()
    }
    #[cfg(not(target_arch = "wasm32"))]
    match (&**ty, lift) {
        (
            IrType::Fun { param, result, .. },
            Lift::Stage {
                to, result: next, ..
            },
        ) => {
            let result = match next {
                Some(inner) => apply_lift_type(result, inner),
                None => result.clone(),
            };
            TypeRef::new(IrType::Fun {
                param: param.clone(),
                stage: *to,
                result,
            })
        }
        (IrType::Named { module, name, args }, Lift::Eff(inner)) if args.len() == 1 => {
            TypeRef::new(IrType::Named {
                module: module.clone(),
                name: name.clone(),
                args: vec![apply_lift_type(&args[0], inner)],
            })
        }
        _ => ty.clone(),
    }
}

/// Result type of applying `func` to one argument, when `func.ty` is a `Fun`.
pub(super) fn app_result_type(func: &Expr) -> Option<TypeRef> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = func;
        None
    }
    #[cfg(not(target_arch = "wasm32"))]
    match func.ty.as_deref()? {
        IrType::Fun { result, .. } => Some(result.clone()),
        _ => None,
    }
}

/// Constructs the `Fun` type of a lambda from its parameters and body type.
pub(super) fn lambda_type(params: &[Param], body: &Expr) -> Option<TypeRef> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (params, body);
        None
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut ty = body.ty.clone()?;
        for p in params.iter().rev() {
            let param = p.ty.clone()?;
            ty = TypeRef::new(IrType::Fun {
                param,
                stage: p.stage,
                result: ty,
            });
        }
        Some(ty)
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn match_type_args<'a>(
    scheme: &'a IrType,
    params: &[String],
    actual: &TypeRef,
    out: &mut HashMap<&'a str, TypeRef>,
) {
    if let IrType::Var(v) = scheme
        && params.iter().any(|p| p == v)
    {
        out.entry(v.as_str()).or_insert_with(|| actual.clone());
        return;
    }
    match (scheme, &**actual) {
        (
            IrType::Named {
                module: m1,
                name: n1,
                args: a1,
            },
            IrType::Named {
                module: m2,
                name: n2,
                args: a2,
            },
        ) if m1 == m2 && n1 == n2 && a1.len() == a2.len() => {
            for (x, y) in a1.iter().zip(a2) {
                match_type_args(x, params, y, out);
            }
        }
        (
            IrType::Fun {
                param: p1,
                result: r1,
                ..
            },
            IrType::Fun {
                param: p2,
                result: r2,
                ..
            },
        ) => {
            match_type_args(p1, params, p2, out);
            match_type_args(r1, params, r2, out);
        }
        (IrType::Tuple(t1), IrType::Tuple(t2)) if t1.len() == t2.len() => {
            for (x, y) in t1.iter().zip(t2) {
                match_type_args(x, params, y, out);
            }
        }
        (IrType::Record { fields: f1, .. }, IrType::Record { fields: f2, .. }) => {
            for (l1, ty1) in f1 {
                if let Some((_, ty2)) = f2.iter().find(|(l2, _)| l2 == l1) {
                    match_type_args(ty1, params, ty2, out);
                }
            }
        }
        (IrType::Forall { body: b1, .. }, IrType::Forall { body: b2, .. }) => {
            match_type_args(b1, params, b2, out);
        }
        (
            IrType::Exists {
                contract: c1,
                body: b1,
                ..
            },
            IrType::Exists {
                contract: c2,
                body: b2,
                ..
            },
        ) => {
            match_type_args(c1, params, c2, out);
            match_type_args(b1, params, b2, out);
        }
        _ => {}
    }
}

/// Builds a `TypeRef` for `Prelude.Unit` (used for synthesized unit literals),
/// or `None` on `wasm32` where full IR types are erased.
#[allow(
    clippy::unnecessary_wraps,
    reason = "returns None on wasm32 where full IR types are erased"
)]
pub(super) fn lang_bare(lang: Lang) -> Option<TypeRef> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::sync::OnceLock;
        if lang == Lang::Unit {
            static UNIT: OnceLock<TypeRef> = OnceLock::new();
            return Some(
                UNIT.get_or_init(|| {
                    let item = lang::item(Lang::Unit);
                    TypeInterner::new().named(&item.module, &item.name, Vec::new())
                })
                .clone(),
            );
        }
        let item = lang::item(lang);
        Some(TypeInterner::new().named(&item.module, &item.name, Vec::new()))
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = lang;
        None
    }
}
