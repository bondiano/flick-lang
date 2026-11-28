//! Type classification for direct Go package bindings under
//! `flick.go-ffi@0.1.0`.

use flick_backend_support::Known;
use flick_ir::{EnumLit, IrType, Lang, Rep, TypeRef};

use super::go_type::TypeDecls;

/// Defensive depth cap for unfolding aliases and nested types.
const MAX_DEPTH: usize = 32;

/// Built-in lang items explicitly outside the `flick.go-ffi@0.1.0` table.
const UNSUPPORTED_LANG: &[Lang] = &[
    Lang::Never,
    Lang::BigInt,
    Lang::Int8,
    Lang::Int16,
    Lang::Int64,
    Lang::UInt8,
    Lang::UInt16,
    Lang::UInt32,
    Lang::UInt64,
    Lang::Float32,
    Lang::Unknown,
    Lang::JsValue,
    Lang::Result,
];

/// Go type form resolved from a `@go` attribute on an opaque Flick type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GoOpaqueType {
    pub(super) pkg: Option<String>,
    pub(super) form: String,
    /// Resolved Go type spelling after import registration (e.g. `*strings.Replacer`).
    pub(super) resolved: Option<String>,
}

/// A Flick type classified into its direct Go value-ABI representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum GoAbiType {
    Unit,
    Bool,
    Int,
    Float,
    String,
    Bytes,
    List(Box<GoAbiType>),
    /// Go pointer `*T`: used ONLY where `nil` unambiguously represents `None`.
    OptionPtr(Box<GoAbiType>),
    /// Presence-flag representation (`(T, bool)` at top-level return,
    /// `struct{ Val T; Ok bool }` inside parameters, slices, or nested options).
    OptionFlag(Box<GoAbiType>),
    /// Opaque nominal handle or polymorphic variable (`flickrt.Value`, or a
    /// typed Go type when the declaration carries a `@go` type form).
    Opaque(Option<GoOpaqueType>),
}

impl GoAbiType {
    /// Whether `*T` unambiguously distinguishes `None` (`nil`) from every valid
    /// `Some` payload without colliding with an inner sentinel.
    pub(super) fn ptr_unambiguous(&self) -> bool {
        matches!(
            self,
            Self::Unit
                | Self::Bool
                | Self::Int
                | Self::Float
                | Self::String
                | Self::Bytes
                | Self::List(_)
        )
    }

    /// Registers Go imports referenced by `@go` opaque type forms and fills in
    /// their resolved Go type spellings.
    pub(super) fn resolve_imports<F>(&mut self, register: &mut F)
    where
        F: FnMut(&str) -> String,
    {
        match self {
            Self::Opaque(Some(opaque)) => {
                let resolved = match &opaque.pkg {
                    Some(pkg) => {
                        let alias = register(pkg);
                        let stars = opaque.form.bytes().take_while(|&b| b == b'*').count();
                        let base = &opaque.form[stars..];
                        format!("{}{alias}.{base}", &opaque.form[..stars])
                    }
                    None => opaque.form.clone(),
                };
                opaque.resolved = Some(resolved);
            }
            Self::List(inner) | Self::OptionPtr(inner) | Self::OptionFlag(inner) => {
                inner.resolve_imports(register);
            }
            _ => {}
        }
    }

    /// Static Go type spelling of this ABI value.
    pub(super) fn go_type(&self) -> String {
        match self {
            Self::Unit => "struct{}".to_owned(),
            Self::Bool => "bool".to_owned(),
            Self::Int => "int32".to_owned(),
            Self::Float => "float64".to_owned(),
            Self::String => "string".to_owned(),
            Self::Bytes => "[]byte".to_owned(),
            Self::List(elem) => format!("[]{}", elem.go_type()),
            Self::OptionPtr(inner) => format!("*{}", inner.go_type()),
            Self::OptionFlag(inner) => format!("struct{{ Val {}; Ok bool }}", inner.go_type()),
            Self::Opaque(Some(GoOpaqueType {
                resolved: Some(go_ty),
                ..
            })) => go_ty.clone(),
            Self::Opaque(_) => "flickrt.Value".to_owned(),
        }
    }

    /// Classifies `ty` into its `flick.go-ffi@0.1.0` representation, or `None`
    /// if `ty` lies outside the Go ABI table.
    pub(super) fn classify(
        lang: &Known,
        decls: &TypeDecls,
        ty: &IrType,
        depth: usize,
    ) -> Option<Self> {
        if depth >= MAX_DEPTH {
            return None;
        }
        match ty {
            IrType::Forall { body, .. } => Self::classify(lang, decls, body, depth + 1),
            IrType::Var(_) => Some(Self::Opaque(None)),
            IrType::Named { module, name, args } => {
                Self::classify_named(lang, decls, module, name, args, depth)
            }
            IrType::Tuple(_)
            | IrType::Record { .. }
            | IrType::Fun { .. }
            | IrType::App { .. }
            | IrType::Exists { .. } => None,
        }
    }

    fn classify_named(
        lang: &Known,
        decls: &TypeDecls,
        module: &str,
        name: &str,
        args: &[TypeRef],
        depth: usize,
    ) -> Option<Self> {
        if lang.is(Lang::Unit, module, name) {
            return Some(Self::Unit);
        }
        if lang.is(Lang::Bool, module, name) {
            return Some(Self::Bool);
        }
        if lang.is(Lang::Int, module, name) {
            return Some(Self::Int);
        }
        if lang.is(Lang::Float, module, name) {
            return Some(Self::Float);
        }
        if lang.is(Lang::String, module, name) {
            return Some(Self::String);
        }
        if lang.is(Lang::Bytes, module, name) {
            return Some(Self::Bytes);
        }
        if lang.is(Lang::List, module, name) {
            let [elem] = args else { return None };
            return Some(Self::List(Box::new(Self::classify(
                lang,
                decls,
                elem,
                depth + 1,
            )?)));
        }
        if lang.is(Lang::Option, module, name) {
            let [inner] = args else { return None };
            let inner_abi = Self::classify(lang, decls, inner, depth + 1)?;
            return Some(if inner_abi.ptr_unambiguous() {
                Self::OptionPtr(Box::new(inner_abi))
            } else {
                Self::OptionFlag(Box::new(inner_abi))
            });
        }
        if UNSUPPORTED_LANG
            .iter()
            .any(|&item| lang.is(item, module, name))
        {
            return None;
        }
        let Some(info) = decls.get(&(module.to_owned(), name.to_owned())) else {
            return Some(Self::Opaque(None));
        };
        let decl = info.decl;
        if let Some(alias) = &decl.alias {
            let instantiated = subst_params(alias, &decl.params, args);
            return Self::classify(lang, decls, &instantiated, depth + 1);
        }
        if decl.error {
            return None;
        }
        if decl.constructors.is_empty() {
            let form = decl
                .foreign_forms
                .iter()
                .find(|f| f.attr == "go")
                .map(|f| GoOpaqueType {
                    pkg: f.from.clone(),
                    form: f.form.clone(),
                    resolved: None,
                });
            return Some(Self::Opaque(form));
        }
        if let [ctor] = decl.constructors.as_slice()
            && ctor.rep == Rep::Unboxed
            && let [field] = ctor.fields.as_slice()
        {
            let instantiated = subst_params(field, &ctor.type_params, args);
            return Self::classify(lang, decls, &instantiated, depth + 1);
        }
        if decl
            .constructors
            .iter()
            .all(|c| matches!(c.rep, Rep::Literal(EnumLit::String(_))))
        {
            return Some(Self::String);
        }
        if decl
            .constructors
            .iter()
            .all(|c| matches!(c.rep, Rep::Literal(EnumLit::Int(_))))
        {
            return Some(Self::Int);
        }
        None
    }
}

fn subst_params(ty: &IrType, params: &[String], args: &[TypeRef]) -> IrType {
    if params.is_empty() || args.is_empty() {
        return ty.clone();
    }
    match ty {
        IrType::Var(v) => params
            .iter()
            .zip(args.iter())
            .find(|(p, _)| *p == v)
            .map_or_else(|| ty.clone(), |(_, arg)| (**arg).clone()),
        IrType::Named {
            module,
            name,
            args: inner,
        } => IrType::Named {
            module: module.clone(),
            name: name.clone(),
            args: inner
                .iter()
                .map(|a| TypeRef::new(subst_params(a, params, args)))
                .collect(),
        },
        IrType::Forall {
            vars,
            constraints,
            body,
        } => {
            let filtered: Vec<(String, TypeRef)> = params
                .iter()
                .zip(args.iter())
                .filter(|(p, _)| !vars.contains(p))
                .map(|(p, a)| (p.clone(), a.clone()))
                .collect();
            let (fp, fa): (Vec<_>, Vec<_>) = filtered.into_iter().unzip();
            IrType::Forall {
                vars: vars.clone(),
                constraints: constraints.clone(),
                body: TypeRef::new(subst_params(body, &fp, &fa)),
            }
        }
        _ => ty.clone(),
    }
}
