//! Services and errors.
//! A `service` lists operations with full signatures; `S.op` needs `S` at
//! every runtime arrow of the operation, and `S.Impl` is the record of the
//! operations as declared, which `Layer.succeed S impl` turns into a layer.
//! Operations are not class methods: no instance is looked up for them.
//! Services and errors are identified by module and name.

use crate::types::Elem;
use serde::{Deserialize, Serialize};

use super::Checker;
use super::env::{Form, TypeDef};
use super::kinds::Kind;
use super::regions::{Unfold, leave_alias};
use super::ty::{Binder, Row, Type};
use crate::diagnostics::codes;
use crate::iface::{Entity, Namespace};
use crate::source::Span;
use crate::types::lang::Lang;
pub use flick_ir::OpClass;

use crate::{diagnostics, source};

/// An operation of a service or an effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Operation {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: Type,
    #[serde(default, skip_serializing_if = "OpClass::is_tail")]
    pub class: OpClass,
}

/// The definition of `S.Impl` for the service definition `def`.
pub fn impl_def(def: &TypeDef) -> Option<TypeDef> {
    let Form::Service {
        params, operations, ..
    } = &def.form
    else {
        return None;
    };
    let fields = operations
        .iter()
        .map(|op| (Elem::named(&op.name), own_type(&op.ty, params.len())))
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
        Type::Forall { body, .. }
        | Type::Captured { ty: body, .. }
        | Type::Bounded { ty: body, .. } => has_runtime_arrow(body),
        Type::Fun {
            effects, result, ..
        } => effects.is_some() || has_runtime_arrow(result),
        _ => false,
    }
}

/// Whether the result of the scheme `ty`, after all its arrows, is a
/// variable it quantifies that no parameter and no constraint names: the
/// type of an operation that never returns.
pub fn returns_any(ty: &Type) -> bool {
    let Type::Forall {
        vars,
        constraints,
        body,
    } = ty
    else {
        return false;
    };
    let mut params = Vec::new();
    let mut result: &Type = body;
    while let Type::Fun {
        param, result: r, ..
    } = result
    {
        params.push(&**param);
        result = r;
    }
    let Type::Var(b) = result else {
        return false;
    };
    let names = |t: &Type| t.contains(&mut |t| matches!(t, Type::Var(v) if v == b));
    vars.iter().any(|v| v.name == *b)
        && !params.into_iter().any(names)
        && !constraints.iter().any(names)
}

/// `ty` with the service `label` needed at every runtime arrow of its stages;
/// an arrow stays direct if `never_suspends` holds of the service.
fn require(ty: Type, label: &str, service: &Type, never_suspends: bool) -> Type {
    match ty {
        Type::Captured {
            ty,
            captures,
            value,
        } => Type::Captured {
            ty: Box::new(require(*ty, label, service, never_suspends)),
            captures,
            value,
        },
        Type::Bounded { ty, captures } => Type::Bounded {
            ty: Box::new(require(*ty, label, service, never_suspends)),
            captures,
        },
        Type::Forall {
            vars,
            constraints,
            body,
        } => Type::Forall {
            vars,
            constraints,
            body: Box::new(require(*body, label, service, never_suspends)),
        },
        Type::Fun {
            param,
            effects,
            result,
        } => Type::Fun {
            param,
            effects: effects.map(|mut e| {
                e.services = Type::Row(Row {
                    entries: vec![(Elem::named(label), service.clone())],
                    tail: Some(Box::new(e.services)),
                    ..Row::default()
                });
                e.sync = super::ty::sync_rows(&e.services, &e.errors);
                e.direct = e.direct && e.sync && never_suspends;
                e
            }),
            result: Box::new(require(*result, label, service, never_suspends)),
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
        span: source::Span,
    ) -> Type {
        let (vars, constraints, body) = match scheme {
            Type::Forall {
                vars,
                constraints,
                body,
            } => (vars.as_slice(), constraints.as_slice(), (**body).clone()),
            other => (&[][..], &[][..], other.clone()),
        };
        let subst: Vec<(String, Type)> = vars
            .iter()
            .map(|v: &Binder| (v.name.clone(), self.store.fresh_of(&v.kind)))
            .collect();
        // the implementation takes their dictionaries, as a constrained function does
        for c in constraints {
            self.want(c.substitute(&subst), span);
        }
        let service = Type::apps(
            Type::con(module, name),
            subst.iter().take(params).map(|(_, t)| t.clone()),
        );
        let ty = self.instantiate(&body.substitute(&subst), span);
        let never_suspends = self.never_suspends(module, name);
        require(ty, &format!("{module}.{name}"), &service, never_suspends)
    }

    /// A service named as a value, `S` or `M.S`, as `Layer.succeed` takes it.
    pub fn service_tag(&mut self, path: &str) -> Option<Type> {
        let super::env::Found::Entity(entity) = self.env.find_type(path) else {
            return None;
        };
        let name = &entity.name;
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
            Elem::Named(format!("{}.{name}", entity.module)),
            service,
        )]));
        Some(Type::app(Type::lang(Lang::ServiceTag), row))
    }

    /// Two layers merged into one must not build the same service: which
    /// implementation wins is said explicitly with `Layer.override`.
    pub fn merge_conflicts(&mut self, first: &Type, second: &Type, span: Span) {
        let outputs = |this: &Self, layer: &Type| {
            let layer = this.whnf(layer);
            let (head, args) = layer.spine();
            match args[..] {
                [_, _, outputs] if head.is_lang(Lang::Layer) => {
                    this.store.normalize(outputs).entries
                }
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
                diagnostics::Diagnostic::error(
                    codes::E004,
                    format!(
                        "both layers build {}: a merged environment has one implementation of each service",
                        both.join(", ")
                    ),
                    span,
                ).with_fix(|| ("use `Layer.override` to let the second layer replace the first", Vec::new())),
            );
        }
    }

    /// The fields of an error carry data, not live values: no cell,
    /// computation, layer, runtime function, `Unknown`, nor anything that
    /// holds a region.
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

    /// `seen` holds the types it has looked into and the aliases it is
    /// unfolding: one met again inside its own unfolding holds what its
    /// arguments hold.
    fn live_part(&self, ty: &Type, seen: &mut Vec<(String, String)>) -> Option<Type> {
        let unfold = self.enter_alias(ty, seen);
        if matches!(unfold, Unfold::Again) {
            let (_, args) = self.store.head(ty).spine();
            return args.iter().find_map(|a| self.live_part(a, seen));
        }
        let live = self.live_unfolded(ty, seen);
        leave_alias(seen, unfold);
        live
    }

    fn live_unfolded(&self, ty: &Type, seen: &mut Vec<(String, String)>) -> Option<Type> {
        let normalized = self.bound_head(ty);
        let ty = &normalized;
        match ty {
            Type::Bounded { ty, .. } => return self.live_part(ty, seen),
            Type::Captured {
                ty: inner,
                captures,
                ..
            } => {
                let mut held = super::ty::Norm::default();
                self.capture_row(captures, &mut held);
                return if held.entries.is_empty() && held.tails.is_empty() {
                    self.live_part(inner, seen)
                } else {
                    Some(ty.clone())
                };
            }
            Type::Fun {
                effects: Some(_), ..
            } => return Some(ty.clone()),
            _ => {}
        }
        let (head, args) = ty.spine();
        if let Type::Con { module, name } = head {
            if [Lang::Ref, Lang::Eff, Lang::Layer, Lang::Unknown]
                .into_iter()
                .any(|n| head.is_lang(n))
            {
                return Some(ty.clone());
            }
            let mut kind = self.env.type_def(module, name).map(|d| &d.kind);
            for _ in &args {
                let Some(Kind::Arrow(param, rest)) = kind else {
                    break;
                };
                if matches!(**param, Kind::Region | Kind::CaptureRow) {
                    return Some(ty.clone());
                }
                kind = Some(rest);
            }
            let key = (module.clone(), name.clone());
            if !seen.contains(&key) {
                seen.push(key);
                let inner = match self.env.type_def(module, name).map(|d| &d.form) {
                    Some(Form::Alias { params, body, .. }) => vec![
                        body.substitute(
                            &params
                                .iter()
                                .cloned()
                                .zip(args.iter().map(|a| (*a).clone()))
                                .collect::<Vec<_>>(),
                        ),
                    ],
                    Some(Form::Adt { constructors, .. }) => constructors
                        .iter()
                        .filter_map(|c| {
                            self.env.value(&Entity {
                                module: module.clone(),
                                namespace: Namespace::Constructor,
                                name: c.clone(),
                            })
                        })
                        .flat_map(|scheme| self.constructor_payloads(scheme, &args, ty))
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

    /// A constructor's result and quantified capture row are metadata, not
    /// stored payloads; specialize its actual fields before checking them.
    fn constructor_payloads(&self, scheme: &Type, args: &[&Type], data: &Type) -> Vec<Type> {
        let mut current = scheme.clone();
        if let Type::Forall { vars, body, .. } = current {
            let mut subst: Vec<(String, Type)> = vars
                .iter()
                .zip(args)
                .map(|(v, a)| (v.name.clone(), (*a).clone()))
                .collect();
            if vars.len() > args.len()
                && let Some(capture) = vars.last().filter(|v| v.kind == Kind::CaptureRow)
            {
                subst.push((capture.name.clone(), self.captures_of(data).into_type()));
            }
            current = body.substitute(&subst);
        }
        let mut payloads = Vec::new();
        while let Type::Fun { param, result, .. } = self.whnf(&current) {
            payloads.push(*param);
            current = *result;
        }
        payloads
    }
}
