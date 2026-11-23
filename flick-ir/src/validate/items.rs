//! Top-level item, constructor-use, and global-use validation.

use super::Validator;
use super::decls::has_dups;
use crate::{Binding, Constraint, Constructor, Expr, Foreign, Stage, TypeInterner, TypeRef};

impl Validator<'_> {
    pub(super) fn check_value_item(
        &mut self,
        name: &str,
        type_params: &[String],
        dicts: &[String],
        dict_constraints: &[Constraint],
        ty: Option<&TypeRef>,
        body: &Expr,
    ) {
        let module = self.module;
        if has_dups(type_params) {
            self.problems
                .push(format!("`{module}.{name}` has duplicate type parameters"));
        }
        if !dict_constraints.is_empty() && dict_constraints.len() != dicts.len() {
            self.problems.push(format!(
                "`{module}.{name}` has {} dictionary constraints for {} dictionaries",
                dict_constraints.len(),
                dicts.len()
            ));
        }
        for c in dict_constraints {
            for a in &c.args {
                self.check_type_wf_arg(a, None);
            }
        }
        if let Some(ty) = ty {
            self.check_type_wf(ty, None);
            if let Some(bty) = self.effective_expr_type(body) {
                self.expect_agree(ty, &bty, &format!("definition `{module}.{name}` body"));
            }
        }
    }

    pub(super) fn check_foreign_item(&mut self, foreign: &Foreign) {
        if let Binding::Inline(inline) = &foreign.binding {
            self.span(inline.span);
        }
        for word in &foreign.adapter.words {
            self.span(word.span);
        }
        if let Some(ty) = &foreign.ty {
            self.check_type_wf(ty, None);
        }
        if let Some(catch) = &foreign.catch {
            self.expr(catch);
        }
    }

    pub(super) fn check_constructor_item(&mut self, c: &Constructor) {
        let module = self.module;
        if !c.fields.is_empty() && c.fields.len() != c.arity {
            self.problems.push(format!(
                "`{module}` constructor `{}` has {} field types for arity {}",
                c.name,
                c.fields.len(),
                c.arity
            ));
        }
        for f in &c.fields {
            self.check_type_wf(f, None);
        }
        if let Some((_, decl)) =
            self.types
                .ctor_decls
                .get(&(c.module.as_str(), c.ty.as_str(), c.name.as_str()))
        {
            if c.arity != decl.fields.len() {
                self.problems.push(format!(
                    "`{module}` constructor `{}` arity {} disagrees with declaration ({})",
                    c.name,
                    c.arity,
                    decl.fields.len()
                ));
            }
            if c.dicts != decl.constraints.len() {
                self.problems.push(format!(
                    "`{module}` constructor `{}` dictionary count {} disagrees with declaration ({})",
                    c.name,
                    c.dicts,
                    decl.constraints.len()
                ));
            }
            if !c.fields.is_empty() && c.fields != decl.fields {
                self.problems.push(format!(
                    "`{module}` constructor `{}` field types disagree with declaration",
                    c.name
                ));
            }
        }
    }

    pub(super) fn check_con_use(&mut self, c: &Constructor, expr_ty: Option<&TypeRef>) {
        let module = self.module;
        for f in &c.fields {
            self.check_type_wf(f, None);
        }
        for a in &c.type_args {
            self.check_type_wf_arg(a, None);
        }
        if !c.fields.is_empty() && c.fields.len() != c.arity {
            self.problems.push(format!(
                "`{module}` constructor `{}` has {} field types for arity {}",
                c.name,
                c.fields.len(),
                c.arity
            ));
        }
        let Some((tdecl, cdecl)) =
            self.types
                .ctor_decls
                .get(&(c.module.as_str(), c.ty.as_str(), c.name.as_str()))
        else {
            return;
        };
        let typed = !c.type_args.is_empty() || !c.fields.is_empty() || expr_ty.is_some();
        if typed
            && (!c.type_args.is_empty() || !cdecl.type_params.is_empty())
            && c.type_args.len() != cdecl.type_params.len()
        {
            self.problems.push(format!(
                "`{module}` constructor `{}.{}` given {} type argument(s), expected {}",
                c.module,
                c.name,
                c.type_args.len(),
                cdecl.type_params.len()
            ));
            return;
        }
        if c.type_args.len() == cdecl.type_params.len() && typed {
            let mut interner = TypeInterner::new();
            let subst: Vec<(&str, &TypeRef)> = cdecl
                .type_params
                .iter()
                .map(String::as_str)
                .zip(c.type_args.iter())
                .collect();
            let expected_fields: Vec<TypeRef> = cdecl
                .fields
                .iter()
                .map(|f| f.substitute(&subst, &mut interner))
                .collect();
            if !c.fields.is_empty() {
                for (i, (actual, expected)) in c.fields.iter().zip(&expected_fields).enumerate() {
                    self.expect_agree(
                        expected,
                        actual,
                        &format!("constructor `{}.{}` field {i}", c.module, c.name),
                    );
                }
            }
            if let Some(expr_ty) = expr_ty {
                let res_args: Vec<TypeRef> = match &cdecl.result_args {
                    Some(args) => args
                        .iter()
                        .map(|a| a.substitute(&subst, &mut interner))
                        .collect(),
                    None => tdecl
                        .params
                        .iter()
                        .map(|p| {
                            subst
                                .iter()
                                .find(|&&(k, _)| k == p)
                                .map_or_else(|| interner.var(p), |&(_, v)| v.clone())
                        })
                        .collect(),
                };
                let mut expected_ty = interner.named(&c.module, &c.ty, res_args);
                for f in expected_fields.iter().rev() {
                    expected_ty = interner.fun(f.clone(), Stage::Pure, expected_ty);
                }
                self.expect_agree(
                    &expected_ty,
                    expr_ty,
                    &format!("constructor `{}.{}` type", c.module, c.name),
                );
            }
        }
    }

    pub(super) fn check_global_use(
        &mut self,
        module: &str,
        name: &str,
        type_args: &[TypeRef],
        expr_ty: Option<&TypeRef>,
    ) {
        for a in type_args {
            self.check_type_wf_arg(a, None);
        }
        let Some(&(type_params, decl_ty)) = self.types.globals.get(&(module, name)) else {
            return;
        };
        let typed = !type_args.is_empty() || expr_ty.is_some();
        if typed
            && (!type_args.is_empty() || !type_params.is_empty())
            && type_args.len() != type_params.len()
        {
            let here = self.module;
            self.problems.push(format!(
                "`{here}` global `{module}.{name}` given {} type argument(s), expected {}",
                type_args.len(),
                type_params.len()
            ));
            return;
        }
        if let (Some(decl_ty), Some(expr_ty)) = (decl_ty, expr_ty)
            && type_args.len() == type_params.len()
        {
            let mut interner = TypeInterner::new();
            let subst: Vec<(&str, &TypeRef)> = type_params
                .iter()
                .map(String::as_str)
                .zip(type_args.iter())
                .collect();
            let expected = decl_ty.substitute(&subst, &mut interner);
            self.expect_agree(
                &expected,
                expr_ty,
                &format!("global `{module}.{name}` instantiation"),
            );
        }
    }
}
