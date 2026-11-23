//! Type declaration index and well-formedness / declaration checks.

use std::collections::{BTreeMap, BTreeSet};

use super::Validator;
use crate::{
    Constraint, Constructor, ConstructorDecl, Expr, IrType, ItemKind, Lang, Module, Program, Stage,
    TypeDecl, TypeInterner, TypeRef, cmp_utf16,
};

/// Declared type parameters and body type of a top-level value or foreign.
type GlobalScheme<'p> = (&'p [String], Option<&'p TypeRef>);

/// Declaration index across all modules of a `Program`.
pub(super) struct TypeIndex<'p> {
    pub type_decls: BTreeMap<(&'p str, &'p str), &'p TypeDecl>,
    pub ctor_decls: BTreeMap<(&'p str, &'p str, &'p str), (&'p TypeDecl, &'p ConstructorDecl)>,
    pub globals: BTreeMap<(&'p str, &'p str), GlobalScheme<'p>>,
    pub lang: BTreeMap<Lang, (&'p str, &'p str)>,
}

impl<'p> TypeIndex<'p> {
    pub(super) fn build(program: &'p Program) -> TypeIndex<'p> {
        let mut type_decls = BTreeMap::new();
        let mut ctor_decls = BTreeMap::new();
        let mut globals = BTreeMap::new();
        for module in &program.modules {
            for decl in &module.types {
                type_decls.insert((module.name.as_str(), decl.name.as_str()), decl);
                for ctor in &decl.constructors {
                    ctor_decls.insert(
                        (module.name.as_str(), decl.name.as_str(), ctor.name.as_str()),
                        (decl, ctor),
                    );
                }
            }
            for item in &module.items {
                match &item.kind {
                    ItemKind::Value {
                        name,
                        type_params,
                        ty,
                        ..
                    } => {
                        globals.insert(
                            (module.name.as_str(), name.as_str()),
                            (type_params.as_slice(), ty.as_ref()),
                        );
                    }
                    ItemKind::Foreign(f) => {
                        globals.insert(
                            (module.name.as_str(), f.name.as_str()),
                            (f.type_params.as_slice(), f.ty.as_ref()),
                        );
                    }
                    _ => {}
                }
            }
        }
        let mut lang = BTreeMap::new();
        for item in &program.lang {
            if let Some(role) = Lang::from_name(&item.lang) {
                lang.insert(role, (item.module.as_str(), item.name.as_str()));
            }
        }
        TypeIndex {
            type_decls,
            ctor_decls,
            globals,
            lang,
        }
    }

    pub(super) fn lang_con(&self, role: Lang) -> (&str, &str) {
        if let Some(&pair) = self.lang.get(&role) {
            return pair;
        }
        match role {
            Lang::Int => ("Prelude", "Int"),
            Lang::Float => ("Prelude", "Float"),
            Lang::BigInt => ("Prelude", "BigInt"),
            Lang::String => ("Prelude", "String"),
            Lang::Bool => ("Prelude", "Bool"),
            Lang::Unit => ("Prelude", "Unit"),
            Lang::Never => ("Prelude", "Never"),
            Lang::Errors => ("Prelude", "Errors"),
            Lang::ServiceImpl => ("Prelude", "ServiceImpl"),
            Lang::List => ("Prelude", "List"),
            _ => ("Prelude", ""),
        }
    }

    pub(super) fn is_lang_con(&self, ty: &IrType, role: Lang) -> bool {
        let (module, name) = self.lang_con(role);
        !name.is_empty() && ty.as_bare_con() == Some((module, name))
    }

    pub(super) fn is_lang_named<'a>(&self, ty: &'a IrType, role: Lang) -> Option<&'a [TypeRef]> {
        let (module, name) = self.lang_con(role);
        match ty {
            IrType::Named {
                module: m,
                name: n,
                args,
            } if !name.is_empty() && m == module && n == name => Some(args),
            _ => None,
        }
    }
}

pub(super) fn has_dups(names: &[String]) -> bool {
    let mut seen = BTreeSet::new();
    names.iter().any(|n| !seen.insert(n.as_str()))
}

impl Validator<'_> {
    pub(super) fn check_module_decls(&mut self, module: &Module) {
        let mut seen = BTreeSet::new();
        for decl in &module.types {
            if !seen.insert(decl.name.as_str()) {
                self.problems.push(format!(
                    "`{}` declares type `{}` twice",
                    module.name, decl.name
                ));
            }
            if has_dups(&decl.params) {
                self.problems.push(format!(
                    "`{}` type `{}` has duplicate type parameters",
                    module.name, decl.name
                ));
            }
            if let Some(alias) = &decl.alias {
                let bound: BTreeSet<&str> = decl.params.iter().map(String::as_str).collect();
                self.check_type_wf(alias, Some(&bound));
            }
            for ctor in &decl.constructors {
                self.check_ctor_decl(decl, ctor);
            }
        }
    }

    fn check_ctor_decl(&mut self, decl: &TypeDecl, ctor: &ConstructorDecl) {
        let module = self.module;
        if has_dups(&ctor.type_params) {
            self.problems.push(format!(
                "`{module}` constructor `{}` has duplicate type parameters",
                ctor.name
            ));
        }
        let bound: BTreeSet<&str> = ctor.type_params.iter().map(String::as_str).collect();
        for ex in &ctor.existentials {
            if !bound.contains(ex.as_str()) {
                self.problems.push(format!(
                    "`{module}` constructor `{}` lists existential `{ex}` not in `type_params`",
                    ctor.name
                ));
            }
        }
        for c in &ctor.constraints {
            for a in &c.args {
                self.check_type_wf_inner(a, Some(&bound), true);
            }
        }
        for f in &ctor.fields {
            self.check_type_wf(f, Some(&bound));
        }
        if let Some(res) = &ctor.result_args {
            if res.len() != decl.params.len() {
                self.problems.push(format!(
                    "`{module}` constructor `{}` has {} result type arguments, expected {} for `{}`",
                    ctor.name,
                    res.len(),
                    decl.params.len(),
                    decl.name
                ));
            }
            for a in res {
                self.check_type_wf_inner(a, Some(&bound), true);
            }
        }
        if let Some(labels) = &ctor.record {
            let valid = ctor.fields.len() == 1
                && matches!(
                    &*ctor.fields[0],
                    IrType::Record { fields, tail: None }
                        if fields.len() == labels.len()
                            && fields.iter().zip(labels).all(|((f, _), l)| f == l)
                );
            if !valid {
                self.problems.push(format!(
                    "`{module}` constructor `{}` record labels do not match its single closed-record field type",
                    ctor.name
                ));
            }
        }
    }

    pub(super) fn check_type_wf(&mut self, ty: &IrType, bound: Option<&BTreeSet<&str>>) {
        self.check_type_wf_inner(ty, bound, false);
    }

    pub(super) fn check_type_wf_arg(&mut self, ty: &IrType, bound: Option<&BTreeSet<&str>>) {
        self.check_type_wf_inner(ty, bound, true);
    }

    #[allow(clippy::too_many_lines)]
    fn check_type_wf_inner(
        &mut self,
        ty: &IrType,
        bound: Option<&BTreeSet<&str>>,
        allow_partial: bool,
    ) {
        let module = self.module;
        match ty {
            IrType::Named {
                module: m,
                name,
                args,
            } => {
                if let Some(decl) = self.types.type_decls.get(&(m.as_str(), name.as_str())) {
                    let bad = if allow_partial {
                        args.len() > decl.params.len()
                    } else {
                        args.len() != decl.params.len()
                    };
                    if bad {
                        self.problems.push(format!(
                            "`{module}` applies type `{m}.{name}` to {} type argument(s), expected {}",
                            args.len(),
                            decl.params.len()
                        ));
                    }
                }
                for a in args {
                    self.check_type_wf_inner(a, bound, true);
                }
            }
            IrType::Var(v) => {
                if v.is_empty() {
                    self.problems
                        .push(format!("`{module}` has an empty type variable name"));
                } else if let Some(bound) = bound
                    && !bound.contains(v.as_str())
                {
                    self.problems.push(format!(
                        "`{module}` type declaration uses unbound type variable `{v}`"
                    ));
                }
            }
            IrType::App { head, args } => {
                if args.is_empty() {
                    self.problems.push(format!(
                        "`{module}` has a type application with no arguments"
                    ));
                }
                self.check_type_wf_inner(head, bound, true);
                for a in args {
                    self.check_type_wf_inner(a, bound, true);
                }
            }
            IrType::Fun { param, result, .. } => {
                self.check_type_wf_inner(param, bound, false);
                self.check_type_wf_inner(result, bound, false);
            }
            IrType::Tuple(items) => {
                for a in items {
                    self.check_type_wf_inner(a, bound, false);
                }
            }
            IrType::Record { fields, tail } => {
                for pair in fields.windows(2) {
                    let ord = cmp_utf16(&pair[0].0, &pair[1].0);
                    if ord.is_eq() {
                        self.problems.push(format!(
                            "`{module}` record type repeats field `{}`",
                            pair[0].0
                        ));
                    } else if ord.is_gt() {
                        self.problems.push(format!(
                            "`{module}` record type fields are not in canonical UTF-16 order (`{}` before `{}`)",
                            pair[0].0, pair[1].0
                        ));
                    }
                }
                for (_, t) in fields {
                    self.check_type_wf_inner(t, bound, false);
                }
                if let Some(t) = tail
                    && let Some(bound) = bound
                    && !bound.contains(t.as_str())
                {
                    self.problems.push(format!(
                        "`{module}` record type uses unbound tail variable `{t}`"
                    ));
                }
            }
            IrType::Forall {
                vars,
                constraints,
                body,
            } => {
                if has_dups(vars) {
                    self.problems
                        .push(format!("`{module}` forall repeats a type variable"));
                }
                let extended = bound.map(|b| {
                    let mut next = b.clone();
                    next.extend(vars.iter().map(String::as_str));
                    next
                });
                let inner = extended.as_ref();
                for c in constraints {
                    for a in &c.args {
                        self.check_type_wf_inner(a, inner, true);
                    }
                }
                self.check_type_wf_inner(body, inner, false);
            }
            IrType::Exists {
                binder,
                contract,
                body,
            } => {
                if binder.is_empty() {
                    self.problems
                        .push(format!("`{module}` existential type has empty binder"));
                }
                self.check_type_wf_inner(contract, bound, true);
                self.check_type_wf_inner(body, bound, false);
            }
        }
    }

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
