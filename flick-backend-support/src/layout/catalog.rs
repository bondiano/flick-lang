//! Declaration index, primitive recognition, and GADT constructor instantiation.

use std::collections::{BTreeMap, BTreeSet};

use flick_ir::{
    ConstructorDecl, IrType, Lang, LangItem, Module, Rep, TypeDecl, TypeInterner, TypeRef,
};

use super::{LayoutError, Primitive};

/// Canonical key identifying a type declaration across modules.
pub(crate) type DeclKey = (String, String);

/// Resolved classification of an [`IrType::Named`] reference.
#[derive(Debug, Clone, Copy)]
pub(crate) enum NamedKind<'p> {
    /// Built-in primitive, fixed-width number, or runtime handle.
    Primitive(Primitive),
    /// Declared data type, error type, opaque alias, or nominal type.
    Decl(&'p TypeDecl),
}

/// Instantiated fields and metadata of one active constructor for a concrete
/// instantiation `T args`.
#[derive(Debug, Clone)]
pub(crate) struct InstantiatedCtor<'p> {
    pub decl_index: usize,
    pub ctor: &'p ConstructorDecl,
    pub fields: Vec<TypeRef>,
    /// Free existential variables remaining in `fields` (erased to pointers).
    pub existentials: BTreeSet<String>,
}

/// Indexed view of all type declarations and lang items of a program.
pub(crate) struct TypeCatalog<'p> {
    pub decls: BTreeMap<DeclKey, &'p TypeDecl>,
    lang_by_type: BTreeMap<DeclKey, Primitive>,
    declared_roles: BTreeSet<String>,
}

impl<'p> TypeCatalog<'p> {
    pub(crate) fn new(modules: &'p [Module], lang: &'p [LangItem]) -> TypeCatalog<'p> {
        let mut decls = BTreeMap::new();
        for module in modules {
            for decl in &module.types {
                decls.insert((module.name.clone(), decl.name.clone()), decl);
            }
        }
        let mut lang_by_type = BTreeMap::new();
        let mut declared_roles = BTreeSet::new();
        for item in lang {
            declared_roles.insert(item.lang.clone());
            if let Some(prim) = lang_role_primitive(&item.lang) {
                lang_by_type.insert((item.module.clone(), item.name.clone()), prim);
            }
        }
        TypeCatalog {
            decls,
            lang_by_type,
            declared_roles,
        }
    }

    /// Classifies `Named { module, name, args }`, validating type argument arity
    /// when a declaration or built-in arity is known.
    pub(crate) fn classify(
        &self,
        module: &str,
        name: &str,
        args_len: usize,
    ) -> Result<NamedKind<'p>, LayoutError> {
        let key = (module.to_owned(), name.to_owned());
        if let Some(&prim) = self.lang_by_type.get(&key) {
            if let Some(decl) = self.decls.get(&key)
                && args_len != decl.params.len()
            {
                return Err(LayoutError::ArityMismatch {
                    module: module.to_owned(),
                    name: name.to_owned(),
                    expected: decl.params.len(),
                    found: args_len,
                });
            }
            return Ok(NamedKind::Primitive(prim));
        }
        if let Some(&decl) = self.decls.get(&key) {
            if args_len != decl.params.len() {
                return Err(LayoutError::ArityMismatch {
                    module: module.to_owned(),
                    name: name.to_owned(),
                    expected: decl.params.len(),
                    found: args_len,
                });
            }
            if decl.alias.is_none() && decl.constructors.is_empty() {
                if let Some(prim) = self.fallback_primitive(module, name) {
                    return Ok(NamedKind::Primitive(prim));
                }
                // Any nominal/opaque type without constructors or alias is an
                // opaque runtime handle represented as a non-null pointer.
                return Ok(NamedKind::Primitive(Primitive::Pointer));
            }
            return Ok(NamedKind::Decl(decl));
        }
        if let Some((prim, expected)) = self.fallback_primitive_with_arity(module, name) {
            if let Some(expected) = expected
                && args_len != expected
            {
                return Err(LayoutError::ArityMismatch {
                    module: module.to_owned(),
                    name: name.to_owned(),
                    expected,
                    found: args_len,
                });
            }
            return Ok(NamedKind::Primitive(prim));
        }
        Err(LayoutError::UnknownType {
            module: module.to_owned(),
            name: name.to_owned(),
        })
    }

    /// Looks up a declaration by `(module, name)` without checking arity (used
    /// for static declaration-level analysis, including partial applications).
    pub(crate) fn decl_of(&self, module: &str, name: &str) -> Option<&'p TypeDecl> {
        let key = (module.to_owned(), name.to_owned());
        if self.lang_by_type.contains_key(&key) {
            return None;
        }
        let decl = *self.decls.get(&key)?;
        if decl.alias.is_none() && decl.constructors.is_empty() {
            return None;
        }
        Some(decl)
    }

    fn fallback_primitive(&self, module: &str, name: &str) -> Option<Primitive> {
        self.fallback_primitive_with_arity(module, name)
            .map(|(p, _)| p)
    }

    fn fallback_primitive_with_arity(
        &self,
        module: &str,
        name: &str,
    ) -> Option<(Primitive, Option<usize>)> {
        if module == "Bytes" && name == "Bytes" && !self.declared_roles.contains("bytes") {
            return Some((Primitive::Bytes, Some(0)));
        }
        if module != "Prelude" {
            return None;
        }
        let (role, prim, arity) = match name {
            "Unit" => ("unit", Primitive::Unit, Some(0)),
            "Never" => ("never", Primitive::Never, Some(0)),
            "Bool" => ("bool", Primitive::Bool, Some(0)),
            "Int" => ("int", Primitive::Int, Some(0)),
            "Float" => ("float", Primitive::Float, Some(0)),
            "BigInt" => ("bigint", Primitive::BigInt, Some(0)),
            "String" => ("string", Primitive::String, Some(0)),
            "Int8" => ("int8", Primitive::Int8, Some(0)),
            "Int16" => ("int16", Primitive::Int16, Some(0)),
            "Int64" => ("int64", Primitive::Int64, Some(0)),
            "UInt8" => ("uint8", Primitive::UInt8, Some(0)),
            "UInt16" => ("uint16", Primitive::UInt16, Some(0)),
            "UInt32" => ("uint32", Primitive::UInt32, Some(0)),
            "UInt64" => ("uint64", Primitive::UInt64, Some(0)),
            "Float32" => ("float32", Primitive::Float32, Some(0)),
            "List" => ("list", Primitive::Pointer, Some(1)),
            "Seq" | "Ref" | "Redacted" | "Schema" | "Url" | "Form" | "Eff" | "Lazy" | "Exit" => {
                ("", Primitive::Pointer, Some(1))
            }
            "PureCodec" | "Codec" => ("", Primitive::Pointer, Some(2)),
            "Unknown" | "JsValue" | "Layer" | "Errors" | "Ui" | "ServiceTag" | "ServiceImpl"
            | "Failure" => ("", Primitive::Pointer, Some(0)),
            _ => return None,
        };
        if !role.is_empty() && self.declared_roles.contains(role) {
            return None;
        }
        Some((prim, arity))
    }
}

/// Maps a `@lang` role name to its [`Primitive`], when the role denotes a
/// primitive scalar or runtime handle rather than an ADT.
fn lang_role_primitive(role: &str) -> Option<Primitive> {
    if let Some(lang) = Lang::from_name(role) {
        return match lang {
            Lang::Unit => Some(Primitive::Unit),
            Lang::Never => Some(Primitive::Never),
            Lang::Bool => Some(Primitive::Bool),
            Lang::Int => Some(Primitive::Int),
            Lang::Float => Some(Primitive::Float),
            Lang::BigInt => Some(Primitive::BigInt),
            Lang::String => Some(Primitive::String),
            Lang::List
            | Lang::Seq
            | Lang::Ref
            | Lang::Redacted
            | Lang::Unknown
            | Lang::JsValue
            | Lang::Schema
            | Lang::Url
            | Lang::Form
            | Lang::PureCodec
            | Lang::Eff
            | Lang::Lazy
            | Lang::Layer
            | Lang::Codec
            | Lang::Errors
            | Lang::Ui
            | Lang::ServiceTag
            | Lang::ServiceImpl
            | Lang::Exit
            | Lang::Failure => Some(Primitive::Pointer),
            _ => None,
        };
    }
    // Forward-compatible roles for fixed-width numeric types (#678) and Bytes.
    match role {
        "bytes" => Some(Primitive::Bytes),
        "int8" => Some(Primitive::Int8),
        "int16" => Some(Primitive::Int16),
        "int64" => Some(Primitive::Int64),
        "uint8" => Some(Primitive::UInt8),
        "uint16" => Some(Primitive::UInt16),
        "uint32" => Some(Primitive::UInt32),
        "uint64" => Some(Primitive::UInt64),
        "float32" => Some(Primitive::Float32),
        _ => None,
    }
}

/// Whether `decl` is structurally an `unboxed` single-field wrapper (before
/// checking for transparent-only self-cycles).
pub(crate) fn is_unboxed_decl(decl: &TypeDecl) -> bool {
    decl.alias.is_none()
        && decl.constructors.len() == 1
        && decl.constructors[0].rep == Rep::Unboxed
        && decl.constructors[0].fields.len() == 1
        && decl.constructors[0].constraints.is_empty()
        && decl.constructors[0].existentials.is_empty()
        && decl.constructors[0].result_args.is_none()
}

/// Instantiates all active constructors of `decl` for concrete `args`,
/// filtering out GADT constructors whose `result_args` cannot unify with `args`.
pub(crate) fn instantiate_constructors<'p>(
    decl: &'p TypeDecl,
    args: &[TypeRef],
    interner: &mut TypeInterner,
) -> Vec<InstantiatedCtor<'p>> {
    let mut out = Vec::with_capacity(decl.constructors.len());
    for (decl_index, ctor) in decl.constructors.iter().enumerate() {
        let Some(subst_map) = match_constructor(decl, ctor, args) else {
            continue;
        };
        let subst: Vec<(&str, &TypeRef)> = subst_map.iter().map(|(k, v)| (k.as_str(), v)).collect();
        let fields = ctor
            .fields
            .iter()
            .map(|f| f.substitute(&subst, interner))
            .collect();
        let existentials: BTreeSet<String> = ctor
            .existentials
            .iter()
            .filter(|v| !subst_map.contains_key(v.as_str()))
            .cloned()
            .collect();
        out.push(InstantiatedCtor {
            decl_index,
            ctor,
            fields,
            existentials,
        });
    }
    out
}

fn match_constructor(
    decl: &TypeDecl,
    ctor: &ConstructorDecl,
    args: &[TypeRef],
) -> Option<BTreeMap<String, TypeRef>> {
    let mut subst = BTreeMap::new();
    match &ctor.result_args {
        None => {
            if decl.params.len() != args.len() {
                return None;
            }
            for (param, arg) in decl.params.iter().zip(args) {
                subst.insert(param.clone(), arg.clone());
            }
            Some(subst)
        }
        Some(res_args) => {
            if res_args.len() != args.len() {
                return None;
            }
            let existential_set: BTreeSet<&str> =
                ctor.existentials.iter().map(String::as_str).collect();
            let bound_vars: BTreeSet<&str> = ctor
                .type_params
                .iter()
                .map(String::as_str)
                .filter(|v| !existential_set.contains(v))
                .collect();
            for (pat, concrete) in res_args.iter().zip(args) {
                if !unify_pattern(pat, concrete, &bound_vars, &mut subst) {
                    return None;
                }
            }
            Some(subst)
        }
    }
}

fn unify_pattern(
    pat: &TypeRef,
    concrete: &TypeRef,
    bound: &BTreeSet<&str>,
    subst: &mut BTreeMap<String, TypeRef>,
) -> bool {
    match (&**pat, &**concrete) {
        (IrType::Var(v), _) if bound.contains(v.as_str()) => {
            if let Some(prev) = subst.get(v) {
                prev == concrete
            } else {
                subst.insert(v.clone(), concrete.clone());
                true
            }
        }
        (IrType::Var(v1), IrType::Var(v2)) => v1 == v2,
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
        ) => {
            m1 == m2
                && n1 == n2
                && a1.len() == a2.len()
                && a1
                    .iter()
                    .zip(a2)
                    .all(|(x, y)| unify_pattern(x, y, bound, subst))
        }
        (IrType::Tuple(a1), IrType::Tuple(a2)) => {
            a1.len() == a2.len()
                && a1
                    .iter()
                    .zip(a2)
                    .all(|(x, y)| unify_pattern(x, y, bound, subst))
        }
        (
            IrType::Record {
                fields: f1,
                tail: t1,
            },
            IrType::Record {
                fields: f2,
                tail: t2,
            },
        ) => {
            t1 == t2
                && f1.len() == f2.len()
                && f1
                    .iter()
                    .zip(f2)
                    .all(|((l1, x), (l2, y))| l1 == l2 && unify_pattern(x, y, bound, subst))
        }
        (
            IrType::Fun {
                param: p1,
                stage: s1,
                result: r1,
            },
            IrType::Fun {
                param: p2,
                stage: s2,
                result: r2,
            },
        ) => s1 == s2 && unify_pattern(p1, p2, bound, subst) && unify_pattern(r1, r2, bound, subst),
        (IrType::App { head: h1, args: a1 }, IrType::App { head: h2, args: a2 }) => {
            a1.len() == a2.len()
                && unify_pattern(h1, h2, bound, subst)
                && a1
                    .iter()
                    .zip(a2)
                    .all(|(x, y)| unify_pattern(x, y, bound, subst))
        }
        _ => pat == concrete,
    }
}

/// Whether `ty` contains any free type variable (respecting `Forall` binders).
pub(crate) fn has_free_var(ty: &IrType) -> bool {
    has_free_var_excluding(ty, &BTreeSet::new())
}

/// Whether `ty` mentions any variable from `wanted` free (outside `Forall`).
pub(crate) fn mentions_any_var(ty: &IrType, wanted: &BTreeSet<String>) -> bool {
    if wanted.is_empty() {
        return false;
    }
    mentions_var_inner(ty, wanted, &BTreeSet::new())
}

fn has_free_var_excluding(ty: &IrType, bound: &BTreeSet<&str>) -> bool {
    match ty {
        IrType::Var(v) => !bound.contains(v.as_str()),
        IrType::Record { fields, tail } => {
            if let Some(t) = tail
                && !bound.contains(t.as_str())
            {
                return true;
            }
            fields.iter().any(|(_, f)| has_free_var_excluding(f, bound))
        }
        IrType::Forall {
            vars,
            constraints,
            body,
        } => {
            let mut inner = bound.clone();
            inner.extend(vars.iter().map(String::as_str));
            constraints
                .iter()
                .any(|c| c.args.iter().any(|a| has_free_var_excluding(a, &inner)))
                || has_free_var_excluding(body, &inner)
        }
        _ => {
            let mut found = false;
            ty.each_child(&mut |c| {
                if !found && has_free_var_excluding(c, bound) {
                    found = true;
                }
            });
            found
        }
    }
}

fn mentions_var_inner(ty: &IrType, wanted: &BTreeSet<String>, bound: &BTreeSet<&str>) -> bool {
    match ty {
        IrType::Var(v) => !bound.contains(v.as_str()) && wanted.contains(v),
        IrType::Record { fields, tail } => {
            if let Some(t) = tail
                && !bound.contains(t.as_str())
                && wanted.contains(t)
            {
                return true;
            }
            fields
                .iter()
                .any(|(_, f)| mentions_var_inner(f, wanted, bound))
        }
        IrType::Forall {
            vars,
            constraints,
            body,
        } => {
            let mut inner = bound.clone();
            inner.extend(vars.iter().map(String::as_str));
            constraints
                .iter()
                .any(|c| c.args.iter().any(|a| mentions_var_inner(a, wanted, &inner)))
                || mentions_var_inner(body, wanted, &inner)
        }
        _ => {
            let mut found = false;
            ty.each_child(&mut |c| {
                if !found && mentions_var_inner(c, wanted, bound) {
                    found = true;
                }
            });
            found
        }
    }
}
