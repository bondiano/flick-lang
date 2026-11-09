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
use crate::types::{Form, Kind, ModuleRequirement, Type, TypeDef, passes_dictionary};

/// The runtime package generated code imports.
const EFFECT_RUNTIME: &str = "@flick-lang/effect-ts";
const FLICK_RUNTIME: &str = "@flick-lang/runtime/rt/types";

/// The runtime module whose `Ui` a Flick `Ui` value is.
const EFFECT_RUNTIME_UI: &str = "@flick-lang/effect-ts/ui/react";
const FLICK_RUNTIME_UI: &str = "@flick-lang/runtime/rt/types";

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
    /// The projection of values compiled for Flick's own runtime.
    rt: bool,
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
        Self::for_runtime(module, lookup, package, false)
    }

    pub fn for_runtime<'a>(
        module: &'a str,
        lookup: &'a dyn Fn(&str) -> Option<Interface>,
        package: &'a dyn Fn(&str) -> Option<String>,
        rt: bool,
    ) -> Projection<'a> {
        Projection {
            module,
            lookup,
            package,
            used: RefCell::new(BTreeSet::new()),
            ui: Cell::new(false),
            rt,
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
                        effect(&a, &e_row, &r_row, self.rt)
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
            // TypeScript has no existential opener that preserves a fresh
            // abstract module identity. `unknown` keeps the exported value
            // present without claiming that its witness may be inspected.
            Type::Exists { .. } => Ok("unknown".to_owned()),
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
                // TypeScript has no `Async`: a function that may suspend is async anyway
                let not_async =
                    |(l, _): &&(String, Type)| lang::label(Lang::Async) != Some(l.as_str());
                for (_, entry) in row.entries.iter().filter(not_async) {
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
        if let Some(shown) = self.module_or_dictionary(module, name, &args, vars)? {
            return Ok(shown);
        }
        let seq = lang::is(Lang::Seq, module, name).then_some(Lang::Seq);
        if let Some(item) = native(module, name).or(seq)
            && let Some(projected) = self.native_applied(item, &args, vars, name)?
        {
            return Ok(projected);
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

    /// A built-in type with a TypeScript form on the selected runtime.
    fn native_applied(
        &self,
        item: Lang,
        args: &[&Type],
        vars: &Vars,
        name: &str,
    ) -> Result<Option<String>, String> {
        if let Some((_, ts)) = PRIMITIVES.iter().find(|&&(p, _)| p == item) {
            return Ok(Some((*ts).to_owned()));
        }
        let arg = |i: usize| self.ty(args[i], vars);
        let projected = match (item, args.len()) {
            (Lang::Stream, 3) => {
                let r = self.row(args[0], vars)?;
                let e = self.row(args[1], vars)?;
                let stream = if self.rt { "$Stream" } else { "$Stream.Stream" };
                format!("{stream}<{}, {e}, {r}>", arg(2)?)
            }
            (Lang::List, 1) => format!("ReadonlyArray<{}>", arg(0)?),
            (Lang::Ui, 0) => {
                self.ui.set(true);
                "$Ui".to_owned()
            }
            (Lang::Seq, 1) => format!("Iterable<{}>", arg(0)?),
            (Lang::Option, 1) => {
                let option = if self.rt { "$Option" } else { "$Option.Option" };
                format!("{option}<{}>", arg(0)?)
            }
            (Lang::Result, 2) => {
                let result = if self.rt { "$Result" } else { "$Result.Result" };
                format!("{result}<{}, {}>", arg(1)?, arg(0)?)
            }
            (Lang::Eff, 3) => {
                let r = self.row(args[0], vars)?;
                let e = self.row(args[1], vars)?;
                effect(&arg(2)?, &e, &r, self.rt)
            }
            (Lang::Layer, 3) => {
                let ri = self.row(args[0], vars)?;
                let e = self.row(args[1], vars)?;
                let ro = self.row(args[2], vars)?;
                let layer = if self.rt { "$Layer" } else { "$Layer.Layer" };
                format!("{layer}<{ro}, {e}, {ri}>")
            }
            (Lang::Errors, 1) => self.row(args[0], vars)?,
            (Lang::Schema, 1) => {
                let schema = if self.rt { "$Schema" } else { "$Schema.Codec" };
                format!("{schema}<{0}, {0}>", arg(0)?)
            }
            (Lang::PureCodec, 2) => {
                let schema = if self.rt { "$Schema" } else { "$Schema.Codec" };
                format!("{schema}<{}, {}>", arg(1)?, arg(0)?)
            }
            (Lang::Codec, 4) => {
                let rd = self.row(args[0], vars)?;
                let re = self.row(args[1], vars)?;
                let schema = if self.rt { "$Schema" } else { "$Schema.Codec" };
                format!("{schema}<{}, {}, {rd}, {re}>", arg(3)?, arg(2)?)
            }
            (Lang::ServiceTag | Lang::ServiceImpl | Lang::Scope, _) => {
                return Err(format!("`{name}` has no TypeScript form"));
            }
            _ => return Ok(None),
        };
        Ok(Some(projected))
    }

    fn module_or_dictionary(
        &self,
        module: &str,
        name: &str,
        args: &[&Type],
        vars: &Vars,
    ) -> Result<Option<String>, String> {
        if module == "Prelude" && name == "Module" {
            return match args {
                [signature] => self.module_value(signature).map(Some),
                _ => Err("`Module` needs one module contract".to_owned()),
            };
        }
        if let Some(class) = name.strip_suffix("$Dict") {
            let def = self.definition(module, class)?;
            if matches!(def.form, Form::Class { .. }) {
                let shown: Result<Vec<_>, _> = args.iter().map(|arg| self.ty(arg, vars)).collect();
                return Ok(Some(format!(
                    "{}{}",
                    self.qualified(module, class),
                    type_args(&shown?)
                )));
            }
        }
        Ok(None)
    }

    fn module_value(&self, signature: &Type) -> Result<String, String> {
        let Type::Con { module, name } = signature else {
            return Err("a module contract alias has no TypeScript projection".to_owned());
        };
        let def = self.definition(module, name)?;
        let Form::ModuleContract { requirements } = def.form else {
            return Err("`Module` argument is not a module contract".to_owned());
        };
        let mut subst = Vec::new();
        for requirement in &requirements {
            match requirement {
                ModuleRequirement::Type {
                    name,
                    manifest: Some(ty),
                    ..
                } => {
                    subst.push((name.clone(), ty.substitute(&subst)));
                }
                ModuleRequirement::Type {
                    name,
                    manifest: None,
                    ..
                } => {
                    return Err(format!(
                        "abstract type `{name}` in a module value has no faithful TypeScript projection"
                    ));
                }
                ModuleRequirement::Nominal {
                    name,
                    identity: Some(identity),
                    ..
                } => {
                    subst.push((name.clone(), Type::con(&identity.module, &identity.name)));
                }
                ModuleRequirement::Nominal {
                    name,
                    identity: None,
                    ..
                } => {
                    return Err(format!(
                        "abstract class or service `{name}` has no TypeScript projection"
                    ));
                }
                _ => {}
            }
        }
        let mut fields = Vec::new();
        for requirement in &requirements {
            if let ModuleRequirement::Value { name, ty } = requirement {
                fields.push(format!(
                    "readonly {}: {}",
                    key(name),
                    self.value(&ty.substitute(&subst))?
                ));
            }
        }
        Ok(if fields.is_empty() {
            "{}".to_owned()
        } else {
            format!("{{ {} }}", fields.join("; "))
        })
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
        let runtime = if self.rt {
            FLICK_RUNTIME
        } else {
            EFFECT_RUNTIME
        };
        let runtime_ui = if self.rt {
            FLICK_RUNTIME_UI
        } else {
            EFFECT_RUNTIME_UI
        };
        let mut out = if self.rt {
            format!(
                "import type {{ EffectValue as $Effect, OptionValue as $Option, ResultValue as $Result, LayerValue as $Layer, SchemaValue as $Schema, StreamValue as $Stream }} from \"{runtime}\";\n"
            )
        } else {
            format!(
                "import type {{ Effect as $Effect, Option as $Option, Result as $Result, Layer as $Layer, Schema as $Schema, Stream as $Stream, Context as $Context }} from \"{runtime}\";\n"
            )
        };
        if self.ui.get() {
            let _ = writeln!(out, "import type {{ Ui as $Ui }} from \"{runtime_ui}\";");
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

include!("ts/helpers.rs");
#[cfg(test)]
mod tests;
