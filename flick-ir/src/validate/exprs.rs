//! Expression, local, lambda, and call consistency checks.

use super::Validator;
use super::agree::matches_scheme;
use crate::{Expr, ExprKind, IrType, Lang, Lit, Num, Param, TypeRef};

impl Validator<'_> {
    pub(super) fn check_local_use(&mut self, name: &str, use_ty: Option<&TypeRef>) {
        let Some(use_ty) = use_ty else { return };
        let Some(binding) = self.local_types.iter().rev().find(|b| b.name == name) else {
            return;
        };
        let Some(bound_ty) = binding.ty.clone() else {
            return;
        };
        let mut type_params = binding.type_params.clone();
        let mut inner: &IrType = &bound_ty;
        while let IrType::Forall { vars, body, .. } = inner {
            type_params.extend(vars.iter().cloned());
            inner = body;
        }
        if type_params.is_empty() {
            self.expect_agree(inner, use_ty, &format!("local `{name}`"));
        } else if !matches_scheme(
            self.types,
            inner,
            &type_params,
            use_ty,
            self.gadt_depth > 0,
            false,
        ) {
            let module = self.module;
            self.problems.push(format!(
                "`{module}` local `{name}` annotated as `{use_ty}`, incompatible with bound scheme `{bound_ty}`"
            ));
        }
    }

    pub(super) fn check_var_use(&mut self, name: &str, use_ty: Option<&TypeRef>) {
        let Some(use_ty) = use_ty else { return };
        let Some((_, Some(var_ty))) = self.var_types.iter().rev().find(|(n, _)| n == name) else {
            return;
        };
        let var_ty = var_ty.clone();
        self.expect_agree(&var_ty, use_ty, &format!("variable `{name}` read"));
    }

    pub(super) fn check_assign(&mut self, name: &str, value: &Expr) {
        let Some((_, Some(var_ty))) = self.var_types.iter().rev().find(|(n, _)| n == name) else {
            return;
        };
        let var_ty = var_ty.clone();
        if let Some(actual) = self.effective_expr_type(value) {
            self.expect_agree(&var_ty, &actual, &format!("assignment to `{name}`"));
        }
    }

    pub(super) fn check_lambda(&mut self, params: &[Param], body: &Expr, lam_ty: Option<&TypeRef>) {
        let Some(lam_ty) = lam_ty else { return };
        let mut cur: &IrType = lam_ty;
        // a lambda the checker accepted as a `FuncRef` may be annotated so
        if let Some([f]) = self.types.is_lang_named(cur, Lang::FuncRef) {
            cur = f;
        }
        for (i, p) in params.iter().enumerate() {
            match cur {
                IrType::Fun {
                    param,
                    stage,
                    result,
                } => {
                    if *stage != p.stage {
                        let module = self.module;
                        self.problems.push(format!(
                            "`{module}` lambda parameter {i} has stage {:?}, type has {:?}",
                            p.stage, stage
                        ));
                    }
                    if let Some(pty) = &p.ty {
                        self.expect_agree(param, pty, &format!("lambda parameter {i}"));
                    }
                    cur = result;
                }
                IrType::Var(_) if self.gadt_depth > 0 => return,
                _ => {
                    let module = self.module;
                    self.problems.push(format!(
                        "`{module}` lambda has {} parameter(s), but annotation `{lam_ty}` is not a function of that arity",
                        params.len()
                    ));
                    return;
                }
            }
        }
        if let Some(bty) = self.effective_expr_type(body) {
            self.expect_agree(cur, &bty, "lambda body");
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn check_expr_consistency(&mut self, e: &Expr) {
        let module = self.module;
        match &e.kind {
            ExprKind::Lit(lit) => {
                if let Some(ty) = &e.ty {
                    self.check_lit_type(lit, ty, "literal");
                }
            }
            ExprKind::Dicts(inner, _) => {
                if let (Some(et), Some(it)) = (&e.ty, self.effective_expr_type(inner)) {
                    self.expect_agree(et, &it, "dictionary application");
                }
            }
            ExprKind::App {
                func, arg, stage, ..
            } => {
                let arg_ty = self.effective_expr_type(arg);
                if let Some(fty) = self.effective_expr_type(func) {
                    match &*fty {
                        IrType::Fun {
                            param,
                            stage: fun_stage,
                            result,
                        } => {
                            if stage != fun_stage {
                                self.problems.push(format!(
                                    "`{module}` call stage {stage:?} disagrees with function type stage {fun_stage:?}"
                                ));
                            }
                            if let Some(at) = &arg_ty {
                                self.expect_agree(param, at, "call argument");
                            }
                            if let Some(rt) = &e.ty {
                                self.expect_agree(result, rt, "call result");
                            }
                        }
                        IrType::Forall { .. } => {}
                        IrType::Var(_) if self.gadt_depth > 0 => {}
                        other => {
                            self.problems
                                .push(format!("`{module}` applies non-function type `{other}`"));
                        }
                    }
                }
                let (head, args) = e.spine();
                let con = match &head.kind {
                    ExprKind::Con(c) => Some(c),
                    ExprKind::Dicts(inner, _) => match &inner.kind {
                        ExprKind::Con(c) => Some(c),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(c) = con
                    && !c.fields.is_empty()
                    && let Some(idx) = args.len().checked_sub(1)
                {
                    if idx >= c.fields.len() {
                        self.problems.push(format!(
                            "`{module}` constructor `{}` applied to too many arguments (arity {})",
                            c.name, c.arity
                        ));
                    } else if let Some(at) = &arg_ty {
                        self.expect_agree(
                            &c.fields[idx],
                            at,
                            &format!("constructor `{}` operand {idx}", c.name),
                        );
                    }
                }
            }
            ExprKind::If(cond, then_e, else_e) => {
                if let Some(ct) = self.effective_expr_type(cond) {
                    self.check_bool(&ct, "if condition");
                }
                if let (Some(et), Some(tt)) = (&e.ty, self.effective_expr_type(then_e)) {
                    self.expect_agree(et, &tt, "if then branch");
                }
                if let (Some(et), Some(ft)) = (&e.ty, self.effective_expr_type(else_e)) {
                    self.expect_agree(et, &ft, "if else branch");
                }
            }
            ExprKind::Tuple(items) => {
                if let Some(ty) = &e.ty {
                    match &**ty {
                        IrType::Tuple(tys) if tys.len() == items.len() => {
                            for (i, (item, expected)) in items.iter().zip(tys).enumerate() {
                                if let Some(actual) = self.effective_expr_type(item) {
                                    self.expect_agree(
                                        expected,
                                        &actual,
                                        &format!("tuple element {i}"),
                                    );
                                }
                            }
                        }
                        IrType::Tuple(tys) => {
                            self.problems.push(format!(
                                "`{module}` tuple has {} elements, annotated with {}",
                                items.len(),
                                tys.len()
                            ));
                        }
                        IrType::Var(_) if self.gadt_depth > 0 => {}
                        other => {
                            self.problems.push(format!(
                                "`{module}` tuple annotated with non-tuple `{other}`"
                            ));
                        }
                    }
                }
            }
            ExprKind::List(items) => {
                if let Some(ty) = &e.ty
                    && let Some(args) = self.types.is_lang_named(ty, Lang::List)
                    && let [elem] = args
                {
                    for (i, item) in items.iter().enumerate() {
                        if let Some(actual) = self.effective_expr_type(item) {
                            self.expect_agree(elem, &actual, &format!("list element {i}"));
                        }
                    }
                }
            }
            ExprKind::Record(fields) => {
                if let Some(ty) = &e.ty
                    && let IrType::Record {
                        fields: ty_fields,
                        tail: None,
                    } = &**ty
                {
                    if fields.len() != ty_fields.len() {
                        self.problems.push(format!(
                            "`{module}` record has {} fields, annotated with {}",
                            fields.len(),
                            ty_fields.len()
                        ));
                    }
                    for (label, expr) in fields {
                        match ty_fields.iter().find(|(l, _)| l == label) {
                            Some((_, expected)) => {
                                if let Some(actual) = self.effective_expr_type(expr) {
                                    self.expect_agree(
                                        expected,
                                        &actual,
                                        &format!("record field `{label}`"),
                                    );
                                }
                            }
                            None => {
                                self.problems.push(format!(
                                    "`{module}` record has field `{label}` absent from annotation `{ty}`"
                                ));
                            }
                        }
                    }
                }
            }
            ExprKind::Not(inner) => {
                if let Some(it) = self.effective_expr_type(inner) {
                    self.check_bool(&it, "not operand");
                }
                if let Some(et) = &e.ty {
                    self.check_bool(et, "not result");
                }
            }
            ExprKind::Neg(num, inner) => {
                let role = num_lang(*num);
                if let Some(it) = self.effective_expr_type(inner)
                    && !self.is_lang_or_gadt_var(&it, role)
                {
                    self.problems.push(format!(
                        "`{module}` negation operand annotated as `{it}`, expected {role:?}"
                    ));
                }
                if let Some(et) = &e.ty
                    && !self.is_lang_or_gadt_var(et, role)
                {
                    self.problems.push(format!(
                        "`{module}` negation result annotated as `{et}`, expected {role:?}"
                    ));
                }
            }
            _ => {}
        }
    }

    pub(super) fn effective_expr_type(&self, e: &Expr) -> Option<TypeRef> {
        if let Some(ty) = &e.ty {
            return Some(ty.clone());
        }
        match &e.kind {
            ExprKind::Dicts(inner, _) => self.effective_expr_type(inner),
            ExprKind::Lit(lit) => {
                let (m, n) = self.types.lang_con(lit_lang(lit));
                (!n.is_empty()).then(|| {
                    TypeRef::new(IrType::Named {
                        module: m.to_owned(),
                        name: n.to_owned(),
                        args: Vec::new(),
                    })
                })
            }
            ExprKind::Local(name) => self
                .local_types
                .iter()
                .rev()
                .find(|b| b.name == *name && b.type_params.is_empty())
                .and_then(|b| b.ty.clone()),
            ExprKind::Var(name) => self
                .var_types
                .iter()
                .rev()
                .find(|(n, _)| n == name)
                .and_then(|(_, t)| t.clone()),
            _ => None,
        }
    }

    pub(super) fn check_lit_type(&mut self, lit: &Lit, ty: &IrType, what: &str) {
        let role = lit_lang(lit);
        if !self.is_lang_or_gadt_var(ty, role) {
            let module = self.module;
            let (m, n) = self.types.lang_con(role);
            self.problems.push(format!(
                "`{module}` {what} annotated as `{ty}`, expected `{m}.{n}`"
            ));
        }
    }

    pub(super) fn check_bool(&mut self, ty: &IrType, what: &str) {
        if !self.is_lang_or_gadt_var(ty, Lang::Bool) {
            let module = self.module;
            self.problems.push(format!(
                "`{module}` {what} annotated as `{ty}`, expected `Bool`"
            ));
        }
    }

    fn is_lang_or_gadt_var(&self, ty: &IrType, role: Lang) -> bool {
        self.types.is_lang_con(ty, role) || (self.gadt_depth > 0 && matches!(ty, IrType::Var(_)))
    }
}

fn lit_lang(lit: &Lit) -> Lang {
    match lit {
        Lit::Int(_) => Lang::Int,
        Lit::Int8(_) => Lang::Int8,
        Lit::Int16(_) => Lang::Int16,
        Lit::Int64(_) => Lang::Int64,
        Lit::UInt8(_) => Lang::UInt8,
        Lit::UInt16(_) => Lang::UInt16,
        Lit::UInt32(_) => Lang::UInt32,
        Lit::UInt64(_) => Lang::UInt64,
        Lit::Float(_) => Lang::Float,
        Lit::Float32(_) => Lang::Float32,
        Lit::BigInt(_) => Lang::BigInt,
        Lit::String(_) => Lang::String,
        Lit::Bool(_) => Lang::Bool,
        Lit::Unit => Lang::Unit,
    }
}

fn num_lang(num: Num) -> Lang {
    num.lang()
}
