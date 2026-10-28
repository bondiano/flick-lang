//! TypeScript declarations of the types a module exports.

use super::{
    NATIVE, PRELUDE, PRIMITIVES, Projection, STREAM, Vars, generic, key, params, type_args,
    uncaptured,
};
use crate::codegen::names;
use crate::iface::{Decl, Export, Interface, Namespace};
use crate::types::{Constructor, Form, Kind, Rep, Type};

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
                Ok(Some(d)) => out.push(d),
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
        (self.module == PRELUDE
            && (PRIMITIVES.iter().any(|(n, _)| *n == name) || NATIVE.contains(&name)))
            || (self.module == STREAM && name == "Stream")
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
                let body = self.ty(body, &vars)?;
                Ok(Some(format!(
                    "export type {name}{} = {body};",
                    type_args(&shown)
                )))
            }
            (Form::Adt { constructors, abi }, _) => {
                let exported: Vec<&Export> = constructors
                    .iter()
                    .filter_map(|c| interface.find(Namespace::Constructor, c))
                    .collect();
                if exported.len() < constructors.len() {
                    return Ok(Some(brand(&generics)));
                }
                self.variants(name, &exported, abi.is_tagged()).map(Some)
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
    /// `unboxed` or `untagged` type is the union of its fields.
    fn variants(
        &self,
        name: &str,
        constructors: &[&Export],
        tagged: bool,
    ) -> Result<String, String> {
        let mut params: Vec<String> = Vec::new();
        let mut cases = Vec::new();
        for c in constructors {
            let scheme = c.scheme.as_ref().ok_or("a constructor has no type")?;
            let (vars, ty) = match scheme {
                Type::Forall { vars, body, .. } => (vars.as_slice(), &**body),
                other => (&[][..], other),
            };
            let scope: Vars = vars
                .iter()
                .map(|b| {
                    let ts = match b.kind {
                        Kind::Region => None,
                        Kind::CaptureRow => Some("never".to_owned()),
                        _ => Some(generic(&b.name)),
                    };
                    (b.name.clone(), ts)
                })
                .collect();
            let mut ty = uncaptured(ty, &scope)?;
            let mut fields = Vec::new();
            while let Type::Fun { param, result, .. } = ty {
                fields.push(self.ty(param, &scope)?);
                ty = uncaptured(result, &scope)?;
            }
            if params.is_empty() {
                params = ty
                    .spine()
                    .1
                    .iter()
                    .map(|a| self.ty(a, &scope))
                    .collect::<Result<_, _>>()?;
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

    /// The brand of a nominal type: its qualified identity.
    fn brand(&self, name: &str) -> String {
        match (self.package)(self.module) {
            Some(p) => format!("{p}/{}.{name}", self.module),
            None => format!("{}.{name}", self.module),
        }
    }
}
