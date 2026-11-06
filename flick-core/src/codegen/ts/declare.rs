//! TypeScript declarations of the types a module exports.

use super::{Projection, Vars, generic, jsdoc, key, native, params, type_args, uncaptured};
use crate::codegen::names;
use crate::iface::{Decl, Export, Interface, Namespace};
use crate::types::{Abi, Binder, Constructor, EnumLit, Form, Kind, Rep, Type, built, is_indexed};

impl Projection<'_> {
    /// Declarations of the types `interface` exports; a type without a
    /// faithful form is skipped with the reason.
    pub fn declarations(&self, interface: &Interface) -> (Vec<String>, Vec<(String, String)>) {
        let mut out = Vec::new();
        let mut refused = Vec::new();
        for export in &interface.exports {
            if export.entity.namespace != Namespace::Type || export.entity.module != self.module {
                continue;
            }
            let name = &export.entity.name;
            if self.native(name) {
                continue;
            }
            match self.declaration(interface, export) {
                Ok(Some(d)) => out.push(jsdoc(interface, name) + &d),
                Ok(None) => {}
                Err(reason) => refused.push((name.clone(), reason)),
            }
        }
        (out, refused)
    }

    /// Whether this module's declaration file has the type `export`, so a
    /// module re-exporting it can `export type` it from here.
    pub(super) fn declares(&self, interface: &Interface, export: &Export) -> bool {
        !self.native(&export.entity.name)
            && matches!(self.declaration(interface, export), Ok(Some(_)))
    }

    /// A Prelude type TypeScript or Effect has, never declared; Effect's
    /// `Stream` too.
    fn native(&self, name: &str) -> bool {
        native(self.module, name).is_some()
    }

    fn declaration(
        &self,
        interface: &Interface,
        export: &Export,
    ) -> Result<Option<String>, String> {
        let name = &export.entity.name;
        let Some(def) = &export.definition else {
            return Ok(None);
        };
        let kinds = params(&def.kind);
        let generics: Vec<String> = (0..kinds.len()).map(|i| format!("P{i}")).collect();
        let brand = |generics: &[String]| {
            let tag = names::string(&self.brand(name));
            let phantom = if generics.is_empty() {
                String::new()
            } else {
                format!("; readonly \"~flick/params\"?: [{}]", generics.join(", "))
            };
            format!(
                "export interface {name}{} {{ readonly \"~flick/brand\": {tag}{phantom} }}",
                type_args(generics)
            )
        };
        if kinds
            .iter()
            .any(|k| !matches!(k, Kind::Type | Kind::ErrorRow | Kind::ServiceRow))
        {
            // regions and higher kinds stay opaque, and so unusable from TypeScript
            return Ok(Some(brand(&[])));
        }
        match (&def.form, export.decl) {
            (
                Form::Alias {
                    params,
                    body,
                    opaque: false,
                },
                _,
            ) => {
                let vars: Vars = params
                    .iter()
                    .map(|p| (p.clone(), Some(generic(p))))
                    .collect();
                let shown: Vec<String> = params.iter().map(|p| generic(p)).collect();
                let object = matches!(body, Type::Record(_));
                let body = self.ty(body, &vars)?;
                // a record is an interface, which TypeScript names on hover; unlike
                // the alias it has no implicit index signature, so it is no
                // `Record<string, unknown>`, which no generated or runtime type asks
                // of it (exported values keep their inline object types)
                Ok(Some(if object {
                    format!("export interface {name}{} {body}", type_args(&shown))
                } else {
                    format!("export type {name}{} = {body};", type_args(&shown))
                }))
            }
            (Form::Adt { constructors, abi }, _) => {
                let exported: Vec<&Export> = constructors
                    .iter()
                    .filter_map(|c| interface.find(Namespace::Constructor, c))
                    .collect();
                if let Abi::Literal(literals) = abi {
                    let all = exported.len() == constructors.len();
                    return Ok(Some(self.enumeration(name, literals, all)));
                }
                if exported.len() < constructors.len() {
                    return Ok(Some(brand(&generics)));
                }
                self.variants(name, &exported, abi.is_tagged()).map(Some)
            }
            (Form::Nominal { literals }, _) if !literals.is_empty() => {
                Ok(Some(self.enumeration(name, literals, false)))
            }
            (Form::Error, _) => {
                let constructors: Vec<&Export> = interface
                    .find(Namespace::Constructor, name)
                    .into_iter()
                    .collect();
                if constructors.is_empty() {
                    return Ok(Some(brand(&generics)));
                }
                self.variants(name, &constructors, true).map(Some)
            }
            (Form::Class { .. }, _) | (_, Decl::Class) => {
                self.class(interface, name, &kinds).map(Some)
            }
            _ => Ok(Some(brand(&generics))),
        }
    }

    /// `export type T<A> = { readonly _tag: "…"; readonly _0: A } | …`; an
    /// `unboxed` or `untagged` type is the union of its fields. A constructor
    /// of a GADT is a case of every instance: TypeScript has no refinement.
    fn variants(
        &self,
        name: &str,
        constructors: &[&Export],
        tagged: bool,
    ) -> Result<String, String> {
        let schemes = constructors
            .iter()
            .map(|c| c.scheme.as_ref().ok_or("a constructor has no type"))
            .collect::<Result<Vec<_>, _>>()?;
        let split = |scheme: &'_ Type| match scheme {
            Type::Forall { vars, body, .. } => (vars.clone(), (**body).clone()),
            other => (Vec::new(), other.clone()),
        };
        let indexed = |scheme: &Type| is_indexed(scheme, self.module, name);
        // the parameters as an ordinary constructor names them
        let params: Vec<String> = if let Some(scheme) = schemes.iter().find(|s| !indexed(s)) {
            let (vars, body) = split(scheme);
            let scope = scope_of(&vars, None);
            built(&body)
                .spine()
                .1
                .iter()
                .map(|a| self.ty(a, &scope))
                .collect::<Result<_, _>>()?
        } else {
            let arity = schemes
                .first()
                .map_or(0, |s| built(&split(s).1).spine().1.len());
            (0..arity).map(|i| format!("T{i}")).collect()
        };
        let mut cases = Vec::new();
        for (c, scheme) in constructors.iter().zip(&schemes) {
            let (vars, ty) = split(scheme);
            let args: Vec<Type> = built(&ty).spine().1.into_iter().cloned().collect();
            let scope = scope_of(&vars, indexed(scheme).then_some((&args[..], &params[..])));
            let mut ty = uncaptured(&ty, &scope)?;
            let mut fields = Vec::new();
            while let Type::Fun { param, result, .. } = ty {
                fields.push(self.ty(param, &scope)?);
                ty = uncaptured(result, &scope)?;
            }
            if !tagged {
                cases.extend(fields);
                continue;
            }
            let constructor = Constructor {
                module: self.module.to_owned(),
                ty: name.to_owned(),
                name: c.entity.name.clone(),
                arity: fields.len(),
                rep: Rep::Tagged,
                dicts: 0,
            };
            let tag = names::string(&names::tag(
                &constructor,
                (self.package)(self.module).as_deref(),
            ));
            let mut parts = vec![format!("readonly _tag: {tag}")];
            parts.extend(
                fields
                    .iter()
                    .enumerate()
                    .map(|(i, f)| format!("readonly _{i}: {f}")),
            );
            cases.push(format!("{{ {} }}", parts.join("; ")));
        }
        Ok(format!(
            "export type {name}{} = {};",
            type_args(&params),
            cases.join(" | ")
        ))
    }

    /// A class as the record of its dictionary: methods and superclass getters.
    fn class(&self, interface: &Interface, name: &str, kinds: &[Kind]) -> Result<String, String> {
        if kinds.iter().any(|k| *k != Kind::Type) {
            return Err(format!(
                "class `{name}` is over a higher kind: its dictionary has no TypeScript form"
            ));
        }
        let def = interface
            .find(Namespace::Type, name)
            .and_then(|e| e.definition.clone())
            .ok_or("the class has no definition")?;
        let Form::Class {
            params,
            supers,
            methods,
            ..
        } = &def.form
        else {
            return Err("not a class".to_owned());
        };
        let vars: Vars = params
            .iter()
            .map(|p| (p.clone(), Some(generic(p))))
            .collect();
        let mut members = Vec::new();
        for s in supers {
            let (head, _) = s.spine();
            if let Type::Con { name: class, .. } = head {
                members.push(format!("readonly ${class}: {}", self.ty(s, &vars)?));
            }
        }
        for method in methods {
            let scheme = interface
                .find(Namespace::Value, method)
                .and_then(|e| e.scheme.clone())
                .ok_or_else(|| format!("method `{method}` is not exported"))?;
            let body = match scheme {
                Type::Forall { body, .. } => *body,
                other => other,
            };
            members.push(format!(
                "readonly {}: {}",
                key(method),
                self.ty(&body, &vars)?
            ));
        }
        let shown: Vec<String> = params.iter().map(|p| generic(p)).collect();
        Ok(format!(
            "export interface {name}{} {{ {} }}",
            type_args(&shown),
            members.join("; ")
        ))
    }

    /// An enum: the union of its literals, or, its constructors hidden, a
    /// brand over the type of its literals.
    fn enumeration(&self, name: &str, literals: &[EnumLit], open: bool) -> String {
        let literal = |l: &EnumLit| match l {
            EnumLit::String(s) => names::string(s),
            EnumLit::Int(n) => n.to_string(),
        };
        if open {
            let cases: Vec<String> = literals.iter().map(literal).collect();
            return format!("export type {name} = {};", cases.join(" | "));
        }
        let base = match literals.first() {
            Some(EnumLit::Int(_)) => "number",
            _ => "string",
        };
        let tag = names::string(&self.brand(name));
        format!("export type {name} = {base} & {{ readonly \"~flick/brand\": {tag} }};")
    }

    /// The brand of a nominal type: its qualified identity.
    fn brand(&self, name: &str) -> String {
        match (self.package)(self.module) {
            Some(p) => format!("{p}/{}.{name}", self.module),
            None => format!("{}.{name}", self.module),
        }
    }
}

/// What a constructor's type variables are in TypeScript. Those of a GADT
/// constructor, with the arguments of what it builds and the type's
/// parameters, are the parameter they stand for, or `unknown`.
fn scope_of(vars: &[Binder], indexed: Option<(&[Type], &[String])>) -> Vars {
    vars.iter()
        .map(|b| {
            let ts = match b.kind {
                Kind::Region => None,
                Kind::CaptureRow => Some("never".to_owned()),
                _ => Some(match indexed {
                    None => generic(&b.name),
                    Some((args, params)) => args
                        .iter()
                        .position(|a| matches!(a, Type::Var(v) if *v == b.name))
                        .and_then(|i| params.get(i).cloned())
                        .unwrap_or_else(|| "unknown".to_owned()),
                }),
            };
            (b.name.clone(), ts)
        })
        .collect()
}
