//! The TypeScript projection of Flick types: what generated TypeScript and
//! `.d.ts` files say about the exports of a module. The projection is
//! checked and limited: a type TypeScript cannot state faithfully (a
//! higher-kinded variable, a region, an open record) is refused with the
//! reason, never written as `any`.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, HashMap};
use std::fmt::Write;

use super::names;
use crate::iface::{Interface, Namespace};
use crate::types::lang::{self, Lang};
use crate::types::{Form, Kind, Type, TypeDef, passes_dictionary};

/// The runtime package generated code imports.
const RUNTIME: &str = "@flick-lang/runtime";

/// The runtime module whose `Ui` a Flick `Ui` value is.
const RUNTIME_UI: &str = "@flick-lang/runtime/ui/react";

/// Built-in types TypeScript has natively.
const PRIMITIVES: &[(Lang, &str)] = &[
    (Lang::Int, "number"),
    (Lang::Float, "number"),
    (Lang::String, "string"),
    (Lang::Bool, "boolean"),
    (Lang::BigInt, "bigint"),
    (Lang::Unit, "undefined"),
    (Lang::Never, "never"),
    (Lang::Unknown, "unknown"),
    (Lang::Json, "unknown"),
];

/// Built-in types projected to native Effect or TypeScript types, or not at
/// all; `Stream` is Effect's `Stream`.
const NATIVE: &[Lang] = &[
    Lang::List,
    Lang::Option,
    Lang::Result,
    Lang::Eff,
    Lang::Layer,
    Lang::Errors,
    Lang::ServiceTag,
    Lang::ServiceImpl,
    Lang::Scope,
    Lang::Schema,
    Lang::PureCodec,
    Lang::Codec,
    Lang::Ui,
    Lang::Stream,
];

/// The built-in type `module.name` that is never declared, being native
/// to TypeScript or Effect.
fn native(module: &str, name: &str) -> Option<Lang> {
    (PRIMITIVES.iter().map(|&(item, _)| item))
        .chain(NATIVE.iter().copied())
        .find(|&item| lang::is(item, module, name))
}

pub struct Projection<'a> {
    pub module: &'a str,
    /// The interface of a module, this one included.
    pub lookup: &'a dyn Fn(&str) -> Option<Interface>,
    /// The package of a module, for constructor ids and brands.
    pub package: &'a dyn Fn(&str) -> Option<String>,
    /// Other modules whose types are referenced.
    pub used: RefCell<BTreeSet<String>>,
    /// Whether the runtime `Ui` is referenced.
    ui: Cell<bool>,
}

/// Type parameters in scope: Flick name to TypeScript name; regions have
/// `None`, and capture rows specialize to the empty row (`never`).
type Vars = HashMap<String, Option<String>>;

impl Projection<'_> {
    pub fn new<'a>(
        module: &'a str,
        lookup: &'a dyn Fn(&str) -> Option<Interface>,
        package: &'a dyn Fn(&str) -> Option<String>,
    ) -> Projection<'a> {
        Projection {
            module,
            lookup,
            package,
            used: RefCell::new(BTreeSet::new()),
            ui: Cell::new(false),
        }
    }

    /// The namespace alias a module's types are imported under.
    pub fn alias(module: &str) -> String {
        names::module_alias(module, 't')
    }

    /// The type of a value of type `scheme`.
    pub fn value(&self, scheme: &Type) -> Result<String, String> {
        self.value_in(&stages::exported(scheme), &Vars::new())
    }

    fn value_in(&self, scheme: &Type, outer: &Vars) -> Result<String, String> {
        let (vars, constraints, body) = match scheme {
            Type::Forall {
                vars,
                constraints,
                body,
            } => (vars.as_slice(), constraints.as_slice(), &**body),
            other => (&[][..], &[][..], other),
        };
        let mut scope = outer.clone();
        let mut generics: Vec<(String, String)> = Vec::new();
        for binder in vars {
            let ts = match binder.kind {
                Kind::Type | Kind::ErrorRow | Kind::ServiceRow => Some(generic(&binder.name)),
                Kind::Region => None,
                Kind::CaptureRow => Some("never".to_owned()),
                _ => {
                    return Err(format!(
                        "type variable `{}` of kind `{}` has no TypeScript form; export a concrete façade",
                        binder.name, binder.kind
                    ));
                }
            };
            if let Some(ts) = &ts
                && binder.kind != Kind::CaptureRow
            {
                generics.push((binder.name.clone(), ts.clone()));
            }
            scope.insert(binder.name.clone(), ts);
        }
        let body = uncaptured(body, &scope)?;
        let function = matches!(body, Type::Fun { .. }) || !constraints.is_empty();
        if !function {
            // a polymorphic constant: `forall a. List a` is `ReadonlyArray<never>`
            for binder in vars {
                if let Some(Some(ts)) = scope.get_mut(&binder.name) {
                    "never".clone_into(ts);
                }
            }
            generics.clear();
        }
        if constraints.iter().all(|c| !passes_dictionary(c)) && !generics.is_empty() {
            return self.staged(body, &scope, &generics);
        }
        let mut ty = self.ty(body, &scope)?;
        // all dictionaries come at once, before the first parameter, with every type parameter
        let mut dicts = Vec::new();
        for constraint in constraints {
            if passes_dictionary(constraint) {
                dicts.push(self.ty(constraint, &scope)?);
            }
        }
        if !dicts.is_empty() {
            let params: Vec<String> = dicts
                .iter()
                .enumerate()
                .map(|(i, d)| format!("dictionary{i}: {d}"))
                .collect();
            ty = format!("({}) => {ty}", params.join(", "));
        }
        if generics.is_empty() {
            Ok(ty)
        } else {
            let names: Vec<&str> = generics.iter().map(|(_, ts)| ts.as_str()).collect();
            Ok(format!("<{}>{ty}", names.join(", ")))
        }
    }

    pub fn ty(&self, t: &Type, vars: &Vars) -> Result<String, String> {
        match t {
            Type::Var(name) => match vars.get(name) {
                Some(Some(ts)) => Ok(ts.clone()),
                Some(None) => Err(format!(
                    "`{name}` is a region: scoped values have no bare TypeScript export"
                )),
                None => Ok(generic(name)),
            },
            Type::Con { .. } | Type::App { .. } => self.applied(t, vars),
            Type::Fun {
                param,
                effects,
                result,
            } => {
                let param = self.ty(param, vars)?;
                let result = match effects {
                    // a Sync function returns its value, read in the context the runtime set
                    None => self.ty(result, vars)?,
                    Some(e) if e.sync => self.ty(result, vars)?,
                    Some(e) => {
                        let a = self.ty(result, vars)?;
                        let e_row = self.row(&e.errors, vars)?;
                        let r_row = self.row(&e.services, vars)?;
                        effect(&a, &e_row, &r_row)
                    }
                };
                Ok(format!("(arg: {param}) => {result}"))
            }
            Type::Tuple(items) => {
                let items: Result<Vec<String>, String> =
                    items.iter().map(|i| self.ty(i, vars)).collect();
                Ok(format!("readonly [{}]", items?.join(", ")))
            }
            Type::Record(row) => {
                if row.tail.is_some() {
                    return Err("an open record has no TypeScript form".to_owned());
                }
                let fields: Result<Vec<String>, String> = row
                    .entries
                    .iter()
                    .map(|(label, ty)| {
                        Ok(format!("readonly {}: {}", key(label), self.ty(ty, vars)?))
                    })
                    .collect();
                let fields = fields?;
                Ok(if fields.is_empty() {
                    "{}".to_owned()
                } else {
                    format!("{{ {} }}", fields.join("; "))
                })
            }
            Type::Captured { .. } | Type::Bounded { .. } => self.ty(uncaptured(t, vars)?, vars),
            Type::Forall { .. } => {
                let inner = self.value_in(t, vars)?;
                Ok(format!("({inner})"))
            }
            Type::Row(_) | Type::Union(..) => self.row(t, vars),
            Type::Symbol(s) => Ok(names::string(s)),
            Type::Meta(_) | Type::Skolem(_) | Type::Error => {
                Err("the type is not fully known".to_owned())
            }
        }
    }

    /// A service or error row as a union; the empty row is `never`.
    fn row(&self, t: &Type, vars: &Vars) -> Result<String, String> {
        let mut parts = Vec::new();
        self.row_parts(t, vars, &mut parts)?;
        Ok(if parts.is_empty() {
            "never".to_owned()
        } else {
            parts.join(" | ")
        })
    }

    fn row_parts(&self, t: &Type, vars: &Vars, out: &mut Vec<String>) -> Result<(), String> {
        match t {
            Type::Row(row) => {
                for (_, entry) in &row.entries {
                    out.push(self.ty(entry, vars)?);
                }
                if let Some(tail) = &row.tail {
                    self.row_parts(tail, vars, out)?;
                }
                Ok(())
            }
            Type::Union(a, b) => {
                self.row_parts(a, vars, out)?;
                self.row_parts(b, vars, out)
            }
            other => {
                out.push(self.ty(other, vars)?);
                Ok(())
            }
        }
    }

    /// A named type applied to its arguments.
    fn applied(&self, t: &Type, vars: &Vars) -> Result<String, String> {
        let (head, args) = t.spine();
        let Type::Con { module, name } = head else {
            let shown = match head {
                Type::Var(v) => v.as_str(),
                _ => "a type",
            };
            return Err(format!(
                "`{shown}` is applied to a type: a higher-kinded type has no TypeScript form; export a concrete façade"
            ));
        };
        let arg = |i: usize| self.ty(args[i], vars);
        let seq = lang::is(Lang::Seq, module, name).then_some(Lang::Seq);
        if let Some(item) = native(module, name).or(seq) {
            if let Some((_, ts)) = PRIMITIVES.iter().find(|&&(p, _)| p == item) {
                return Ok((*ts).to_owned());
            }
            match (item, args.len()) {
                (Lang::Stream, 3) => {
                    let r = self.row(args[0], vars)?;
                    let e = self.row(args[1], vars)?;
                    return Ok(format!("$Stream.Stream<{}, {e}, {r}>", arg(2)?));
                }
                (Lang::List, 1) => return Ok(format!("ReadonlyArray<{}>", arg(0)?)),
                (Lang::Ui, 0) => {
                    self.ui.set(true);
                    return Ok("$Ui".to_owned());
                }
                (Lang::Seq, 1) => return Ok(format!("Iterable<{}>", arg(0)?)),
                (Lang::Option, 1) => return Ok(format!("$Option.Option<{}>", arg(0)?)),
                (Lang::Result, 2) => {
                    return Ok(format!("$Result.Result<{}, {}>", arg(1)?, arg(0)?));
                }
                (Lang::Eff, 3) => {
                    let r = self.row(args[0], vars)?;
                    let e = self.row(args[1], vars)?;
                    return Ok(effect(&arg(2)?, &e, &r));
                }
                (Lang::Layer, 3) => {
                    let ri = self.row(args[0], vars)?;
                    let e = self.row(args[1], vars)?;
                    let ro = self.row(args[2], vars)?;
                    return Ok(format!("$Layer.Layer<{ro}, {e}, {ri}>"));
                }
                (Lang::Errors, 1) => return self.row(args[0], vars),
                (Lang::Schema, 1) => return Ok(format!("$Schema.Codec<{0}, {0}>", arg(0)?)),
                (Lang::PureCodec, 2) => {
                    return Ok(format!("$Schema.Codec<{}, {}>", arg(1)?, arg(0)?));
                }
                (Lang::Codec, 4) => {
                    let rd = self.row(args[0], vars)?;
                    let re = self.row(args[1], vars)?;
                    return Ok(format!(
                        "$Schema.Codec<{}, {}, {rd}, {re}>",
                        arg(3)?,
                        arg(2)?
                    ));
                }
                (Lang::ServiceTag | Lang::ServiceImpl | Lang::Scope, _) => {
                    return Err(format!("`{name}` has no TypeScript form"));
                }
                _ => {}
            }
        }
        let def = self.definition(module, name)?;
        let params = params(&def.kind);
        let mut shown = Vec::new();
        for (kind, a) in params.iter().zip(&args) {
            match kind {
                Kind::Region | Kind::CaptureRow => {
                    return Err(format!(
                        "`{name}` holds a region: scoped values have no bare TypeScript export"
                    ));
                }
                Kind::ErrorRow | Kind::ServiceRow => shown.push(self.row(a, vars)?),
                _ => shown.push(self.ty(a, vars)?),
            }
        }
        if args.len() != params.len() {
            return Err(format!("`{name}` is not fully applied"));
        }
        if let Form::Alias {
            params: names,
            body,
            opaque: false,
        } = &def.form
        {
            // declared as `export type`, once its body has a form
            let mut inner = vars.clone();
            for (n, a) in names.iter().zip(&shown) {
                inner.insert(n.clone(), Some(a.clone()));
            }
            self.ty(body, &inner)?;
        }
        Ok(format!(
            "{}{}",
            self.qualified(module, name),
            type_args(&shown)
        ))
    }

    /// `T` of this module, `$t_A$B.T` of another.
    fn qualified(&self, module: &str, name: &str) -> String {
        if module == self.module {
            name.to_owned()
        } else {
            self.used.borrow_mut().insert(module.to_owned());
            format!("{}.{name}", Self::alias(module))
        }
    }

    fn definition(&self, module: &str, name: &str) -> Result<TypeDef, String> {
        let interface =
            (self.lookup)(module).ok_or_else(|| format!("module `{module}` is not known"))?;
        interface
            .find(Namespace::Type, name)
            .and_then(|e| e.definition.clone())
            .ok_or_else(|| format!("type `{name}` of `{module}` is not exported"))
    }

    /// `import type` lines for what the projection referenced, with
    /// `path(module)` the import specifier of a module.
    pub fn imports(&self, path: &dyn Fn(&str) -> String) -> String {
        let mut out = format!(
            "import type {{ Effect as $Effect, Option as $Option, Result as $Result, Layer as $Layer, Schema as $Schema, Stream as $Stream, Context as $Context }} from \"{RUNTIME}\";\n"
        );
        if self.ui.get() {
            let _ = writeln!(out, "import type {{ Ui as $Ui }} from \"{RUNTIME_UI}\";");
        }
        for module in self.used.borrow().iter() {
            let _ = writeln!(
                out,
                "import type * as {} from {};",
                Self::alias(module),
                names::string(&path(module))
            );
        }
        out
    }
}

fn uncaptured<'a>(mut t: &'a Type, vars: &Vars) -> Result<&'a Type, String> {
    while let Type::Captured { ty, captures, .. } | Type::Bounded { ty, captures } = t {
        if !empty_captures(captures, vars) {
            return Err(
                "a value holding a scope's capabilities has no bare TypeScript export".to_owned(),
            );
        }
        t = ty;
    }
    Ok(t)
}

fn empty_captures(t: &Type, vars: &Vars) -> bool {
    match t {
        Type::Row(row) => {
            row.entries.iter().all(|(_, t)| empty_captures(t, vars))
                && row.tail.as_deref().is_none_or(|t| empty_captures(t, vars))
        }
        Type::Union(a, b) => empty_captures(a, vars) && empty_captures(b, vars),
        // TypeScript type parameters contain only scope-free data.
        Type::Var(name) => matches!(vars.get(name), Some(Some(_))),
        _ => false,
    }
}

/// The kinds of the parameters of a type constructor of kind `k`.
fn params(k: &Kind) -> Vec<Kind> {
    let mut out = Vec::new();
    let mut k = k;
    while let Kind::Arrow(p, r) = k {
        out.push((**p).clone());
        k = r;
    }
    out
}

/// The TypeScript name of a type variable: `a` is `A`, `elem` is `Elem`.
fn generic(name: &str) -> String {
    let mut chars = name.chars();
    let first = chars.next().map_or('T', |c| c.to_ascii_uppercase());
    let rest: String = chars
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    format!("{first}{rest}")
}

fn type_args(args: &[String]) -> String {
    if args.is_empty() {
        String::new()
    } else {
        format!("<{}>", args.join(", "))
    }
}

/// `Effect<A, E, R>` with `never` for an empty row.
fn effect(a: &str, e: &str, r: &str) -> String {
    format!("$Effect.Effect<{a}, {e}, {r}>")
}

fn key(label: &str) -> String {
    if names::is_property(label) {
        label.to_owned()
    } else {
        names::string(label)
    }
}

mod declare;
mod dts;
pub mod stages;

pub use dts::{Facade, declaration_file, facade_files};
#[cfg(test)]
mod tests;
