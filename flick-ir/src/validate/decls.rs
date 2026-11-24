//! Type declaration index and well-formedness / declaration checks.

use std::collections::{BTreeMap, BTreeSet};

use super::Validator;
use crate::{
    ConstructorDecl, IrType, ItemKind, Lang, Module, Program, TypeDecl, TypeRef, cmp_utf16,
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
            Lang::Int8 => ("Prelude", "Int8"),
            Lang::Int16 => ("Prelude", "Int16"),
            Lang::Int64 => ("Prelude", "Int64"),
            Lang::UInt8 => ("Prelude", "UInt8"),
            Lang::UInt16 => ("Prelude", "UInt16"),
            Lang::UInt32 => ("Prelude", "UInt32"),
            Lang::UInt64 => ("Prelude", "UInt64"),
            Lang::Float => ("Prelude", "Float"),
            Lang::Float32 => ("Prelude", "Float32"),
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
        for preamble in &module.preambles {
            self.span(preamble.span);
        }
        for export in &module.exports.host {
            self.span(export.span);
            let exists = module.items.iter().any(|item| match &item.kind {
                ItemKind::Value { name, .. } => name == &export.name,
                ItemKind::Foreign(f) => f.name == export.name,
                _ => false,
            });
            if !exists {
                self.problems.push(format!(
                    "`{}` host export `{}` names missing value `{}`",
                    module.name, export.export_name, export.name
                ));
            }
        }
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
}
