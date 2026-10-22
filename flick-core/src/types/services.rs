//! Services and errors.
//! A `service` lists operations with full signatures; `S.op` needs `S` at
//! every runtime arrow of the operation, and `S.Impl` is the record of the
//! operations as declared, which `Layer.succeed S impl` turns into a layer.
//! Operations are not class methods: no instance is looked up for them.
//! Services and errors are identified by module and name.

use super::Checker;
use super::env::{Form, TypeDef};
use super::kinds::Kind;
use super::ty::{Binder, Row, Type};
use crate::diagnostics::codes;
use crate::iface::{Entity, Namespace};
use crate::source::Span;

/// The definition of `S.Impl` for the service definition `def`.
pub fn impl_def(def: &TypeDef) -> Option<TypeDef> {
    let Form::Service { params, operations } = &def.form else {
        return None;
    };
    let fields = operations
        .iter()
        .map(|op| (op.name.clone(), own_type(&op.ty, params.len())))
        .collect();
    Some(TypeDef {
        kind: with_result(&def.kind, Kind::Type),
        form: Form::Alias {
            params: params.clone(),
            body: Type::Record(Row::closed(fields)),
            opaque: false,
        },
    })
}

/// An operation's scheme without the service's own parameters, which come
/// first among its variables.
fn own_type(scheme: &Type, params: usize) -> Type {
    match scheme {
        Type::Forall {
            vars,
            constraints,
            body,
        } if params > 0 => {
            if vars.len() <= params && constraints.is_empty() {
                return (**body).clone();
            }
            Type::Forall {
                vars: vars.iter().skip(params).cloned().collect(),
                constraints: constraints.clone(),
                body: body.clone(),
            }
        }
        other => other.clone(),
    }
}

fn with_result(kind: &Kind, result: Kind) -> Kind {
    match kind {
        Kind::Arrow(param, rest) => Kind::arrow((**param).clone(), with_result(rest, result)),
        _ => result,
    }
}

/// Whether the arrows of `ty`, stage by stage, include a runtime one.
pub fn has_runtime_arrow(ty: &Type) -> bool {
    match ty {
        Type::Forall { body, .. } => has_runtime_arrow(body),
        Type::Fun {
            effects, result, ..
        } => effects.is_some() || has_runtime_arrow(result),
        _ => false,
    }
}

/// `ty` with the service `label` needed at every runtime arrow of its stages.
fn require(ty: Type, label: &str, service: &Type) -> Type {
    match ty {
        Type::Fun {
            param,
            effects,
            result,
        } => Type::Fun {
            param,
            effects: effects.map(|mut e| {
                e.services = Type::Row(Row {
                    entries: vec![(label.to_owned(), service.clone())],
                    tail: Some(Box::new(e.services)),
                });
                e
            }),
            result: Box::new(require(*result, label, service)),
        },
        other => other,
    }
}

impl Checker<'_> {
    /// The type of `S.op` at a use.
    pub fn operation_type(
        &mut self,
        module: &str,
        name: &str,
        params: usize,
        scheme: &Type,
    ) -> Type {
        let (vars, body) = match scheme {
            Type::Forall { vars, body, .. } => (vars.as_slice(), (**body).clone()),
            other => (&[][..], other.clone()),
        };
        let subst: Vec<(String, Type)> = vars
            .iter()
            .map(|v: &Binder| (v.name.clone(), self.fresh()))
            .collect();
        let service = Type::apps(
            Type::con(module, name),
            subst.iter().take(params).map(|(_, t)| t.clone()),
        );
        let ty = self.instantiate(&body.substitute(&subst));
        require(ty, &format!("{module}.{name}"), &service)
    }

    /// A service named as a value, as `Layer.succeed` takes it.
    pub fn service_tag(&mut self, name: &str) -> Option<Type> {
        let super::env::Found::Entity(entity) = self.env.find(Namespace::Type, name) else {
            return None;
        };
        let Some(Form::Service { params, .. }) = self
            .env
            .type_def(&entity.module, name)
            .map(|d| d.form.clone())
        else {
            return None;
        };
        let args: Vec<Type> = params.iter().map(|_| self.fresh()).collect();
        let service = Type::apps(Type::con(&entity.module, name), args);
        let row = Type::Row(Row::closed(vec![(
            format!("{}.{name}", entity.module),
            service,
        )]));
        Some(Type::app(Type::prim("ServiceTag"), row))
    }

    /// Two layers merged into one must not build the same service: which
    /// implementation wins is said explicitly with `Layer.override`.
    pub fn merge_conflicts(&mut self, first: &Type, second: &Type, span: Span) {
        let outputs = |this: &Self, layer: &Type| {
            let layer = this.whnf(layer);
            let (head, args) = layer.spine();
            match args[..] {
                [_, _, outputs] if head.is_prim("Layer") => this.store.normalize(outputs).entries,
                _ => Vec::new(),
            }
        };
        let theirs = outputs(self, second);
        let both: Vec<String> = outputs(self, first)
            .iter()
            .filter(|(label, _)| theirs.iter().any(|(l, _)| l == label))
            .map(|(_, ty)| format!("`{}`", self.show(ty)))
            .collect();
        if !both.is_empty() {
            self.diagnostics.push(
                crate::diagnostics::Diagnostic::error(
                    codes::E004,
                    format!(
                        "both layers build {}: a merged environment has one implementation of each service",
                        both.join(", ")
                    ),
                    span,
                )
                .with_fix(
                    "use `Layer.override` to let the second layer replace the first",
                    Vec::new(),
                ),
            );
        }
    }

    /// The fields of an error carry data, not live values: no cell,
    /// computation, layer, runtime function or `Unknown`.
    pub fn check_error_payload(&mut self, name: &str, span: Span) {
        let entity = self.env.entity(Namespace::Constructor, name);
        let Some(mut ty) = self.env.value(&entity).cloned() else {
            return;
        };
        if let Type::Forall { body, .. } = ty {
            ty = *body;
        }
        let mut seen = Vec::new();
        while let Type::Fun { param, result, .. } = ty {
            if let Some(live) = self.live_part(&param, &mut seen) {
                let shown = self.show(&live);
                self.error(
                    codes::E005,
                    span,
                    format!(
                        "the payload of error `{name}` holds `{shown}`, a live value; an error carries data such as a path, an id or a copy"
                    ),
                );
                return;
            }
            ty = *result;
        }
    }

    fn live_part(&self, ty: &Type, seen: &mut Vec<(String, String)>) -> Option<Type> {
        if let Type::Fun {
            effects: Some(_), ..
        } = ty
        {
            return Some(ty.clone());
        }
        let (head, args) = ty.spine();
        if let Type::Con { module, name } = head {
            if ["Ref", "Eff", "Layer", "Unknown"]
                .iter()
                .any(|n| head.is_prim(n))
            {
                return Some(ty.clone());
            }
            let key = (module.clone(), name.clone());
            if !seen.contains(&key) {
                seen.push(key);
                let inner = match self.env.type_def(module, name).map(|d| &d.form) {
                    Some(Form::Alias { body, .. }) => vec![body.clone()],
                    Some(Form::Adt { constructors }) => constructors
                        .iter()
                        .filter_map(|c| {
                            self.env.value(&Entity {
                                module: module.clone(),
                                namespace: Namespace::Constructor,
                                name: c.clone(),
                            })
                        })
                        .cloned()
                        .collect(),
                    _ => Vec::new(),
                };
                for t in inner {
                    if let Some(live) = self.live_part(&t, seen) {
                        return Some(live);
                    }
                }
            }
            return args.iter().find_map(|a| self.live_part(a, seen));
        }
        let mut found = None;
        ty.for_each_child(&mut |child| {
            if found.is_none() {
                found = self.live_part(child, seen);
            }
        });
        found
    }
}
