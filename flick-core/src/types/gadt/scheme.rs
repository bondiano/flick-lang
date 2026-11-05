//! The schemes of the constructors of a variant type. An ordinary
//! constructor builds `T ᾱ` over the type's parameters; one written with
//! `: R` builds `R` over variables of its own, and those of its fields that
//! `R` does not mention are existential.

use std::mem;

use crate::diagnostics::codes;
use crate::iface::Namespace;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::Checker;
use crate::types::convert::{Free, is_type};
use crate::types::kinds::Kind;
use crate::types::ty::{Binder, Norm, Type};

/// A constructor of a variant type as declared.
pub struct Variant {
    pub name: String,
    pub payload: Vec<Type>,
    pub indexed: Option<Indexed>,
}

/// What a constructor written with `: R` builds.
pub struct Indexed {
    pub result: Type,
    /// The constructor's own type variables, in the order they are written.
    pub locals: Vec<Binder>,
    pub span: Span,
}

/// Whether `v` is the hidden capture row shared by a constructor's fields
/// and the value it builds.
pub fn is_hidden_captures(v: &Binder) -> bool {
    v.kind == Kind::CaptureRow && v.name.starts_with("captures")
}

/// Whether the constructor with this scheme, of the type `module.name`,
/// builds something other than `T ᾱ` over the type's parameters, hides a
/// type, or holds a dictionary: matching it is a case of a GADT.
pub fn is_indexed(scheme: &Type, module: &str, name: &str) -> bool {
    let (vars, constraints, body) = match scheme {
        Type::Forall {
            vars,
            constraints,
            body,
        } => (&vars[..], &constraints[..], &**body),
        t => (&[][..], &[][..], t),
    };
    let (head, args) = built(body).spine();
    if !matches!(head, Type::Con { module: m, name: n } if m == module && n == name) {
        return false;
    }
    let plain = args.len() <= vars.len()
        && args
            .iter()
            .zip(vars)
            .all(|(a, v)| matches!(a, Type::Var(x) if *x == v.name));
    !(plain && vars[args.len()..].iter().all(is_hidden_captures) && constraints.is_empty())
}

/// What a constructor of type `body`, without its `forall`, builds: the
/// result of its fields without the hidden capture row.
pub fn built(body: &Type) -> &Type {
    let mut result = body;
    while let Type::Fun { result: r, .. } = result {
        result = r;
    }
    match result {
        Type::Captured { ty, .. } => ty,
        t => t,
    }
}

/// `forall params. body`, or `body` when there are no parameters.
pub fn quantify(params: &[Binder], body: Type) -> Type {
    if params.is_empty() {
        return body;
    }
    match body {
        Type::Forall {
            vars,
            constraints,
            body,
        } => Type::Forall {
            vars: params.iter().cloned().chain(vars).collect(),
            constraints,
            body,
        },
        body => Type::Forall {
            vars: params.to_vec(),
            constraints: Vec::new(),
            body: Box::new(body),
        },
    }
}

impl Checker<'_> {
    /// A constructor with its fields converted; the declaration's
    /// parameters are in scope, except for a constructor written with `: R`.
    pub(in crate::types) fn variant(&mut self, variant: &Node) -> Option<Variant> {
        let name = self.con_token(variant)?;
        let Some(result) = variant.nodes().find(|n| n.kind == K::VariantResult) else {
            let payload = self.payload(variant);
            return Some(Variant {
                name,
                payload,
                indexed: None,
            });
        };
        let outer = mem::take(&mut self.tyvars);
        let payload = self.fields(variant, Free::Quantify);
        let ty = match result.nodes().find(|n| is_type(n.kind)) {
            Some(node) => self.ty(node, &Kind::Type, Free::Quantify),
            None => Type::Error,
        };
        let locals = mem::replace(&mut self.tyvars, outer)
            .into_iter()
            .map(|v| Binder {
                name: v.name,
                kind: v.kind,
            })
            .collect();
        Some(Variant {
            name,
            payload,
            indexed: Some(Indexed {
                result: ty,
                locals,
                span: self.span(result),
            }),
        })
    }

    pub(in crate::types) fn payload(&mut self, variant: &Node) -> Vec<Type> {
        self.fields(variant, Free::Reject)
    }

    fn fields(&mut self, variant: &Node, free: Free) -> Vec<Type> {
        let mut out = Vec::new();
        for field in variant.nodes().filter(|n| is_type(n.kind)) {
            if !self.literal_field(variant, field) {
                out.push(self.ty(field, &Kind::Type, free));
            }
        }
        out
    }

    /// Stores the scheme of each constructor of the type `name` with these
    /// (kind-defaulted) parameters.
    pub(in crate::types) fn constructor_schemes(
        &mut self,
        name: &str,
        error: bool,
        params: &[Binder],
        variants: Vec<Variant>,
    ) {
        let module = self.env.module.clone();
        let own = Type::apps(
            Type::con(&module, name),
            params.iter().map(|b| Type::Var(b.name.clone())),
        );
        for variant in variants {
            let scheme = if error {
                let ty = variant
                    .payload
                    .into_iter()
                    .rev()
                    .fold(own.clone(), |acc, t| Type::pure_fun(t, acc));
                quantify(params, ty)
            } else if let Some(indexed) = variant.indexed {
                let locals: Vec<Binder> = indexed
                    .locals
                    .iter()
                    .map(|b| Binder {
                        name: b.name.clone(),
                        kind: self.kinds.zonk(&b.kind, true),
                    })
                    .collect();
                let result = self.indexed_result(name, params, &locals, &indexed);
                quantify(&locals, self.held_scheme(&locals, variant.payload, result))
            } else {
                quantify(params, self.held_scheme(params, variant.payload, own.clone()))
            };
            let entity = self.env.entity(Namespace::Constructor, &variant.name);
            let scheme = self.default_kinds(&scheme);
            self.env.set_value(entity.clone(), scheme);
            self.env.set_parent(entity, name);
            self.env.declare(Namespace::Constructor, &variant.name);
        }
    }

    /// `R` of `| K … : R` once it is a full application of the type `name`
    /// itself whose region, row and capture arguments are variables of the
    /// constructor; `Error` after reporting otherwise.
    fn indexed_result(
        &mut self,
        name: &str,
        params: &[Binder],
        locals: &[Binder],
        indexed: &Indexed,
    ) -> Type {
        if indexed.result == Type::Error {
            return Type::Error;
        }
        let (head, args) = indexed.result.spine();
        let own = matches!(head, Type::Con { module, name: n }
            if *module == self.env.module && n == name);
        if !own || args.len() != params.len() {
            let shown = self.show(&indexed.result);
            self.error(
                codes::T056,
                indexed.span,
                format!("a constructor of `{name}` builds `{name}` applied to all its parameters, not `{shown}`"),
            );
            return Type::Error;
        }
        for (param, arg) in params.iter().zip(&args) {
            let passed = matches!(arg, Type::Var(v) if locals.iter().any(|l| l.name == *v));
            if (param.kind.is_row() || param.kind == Kind::Region) && !passed {
                let shown = self.show(arg);
                self.error(
                    codes::T056,
                    indexed.span,
                    format!(
                        "the parameter `{}` of `{name}` is of kind {}: a constructor passes it on as a variable, not `{shown}`",
                        param.name, param.kind
                    ),
                );
                return Type::Error;
            }
        }
        indexed.result.clone()
    }

    /// `fields -> result` whose fields and value share a hidden capture row,
    /// bound by its own `forall` when anything holds one.
    fn held_scheme(&mut self, vars: &[Binder], payload: Vec<Type>, result: Type) -> Type {
        let mut capture_name = "captures".to_owned();
        while vars.iter().any(|p| p.name == capture_name)
            || payload.iter().any(|t| {
                t.contains(&mut |t| match t {
                    Type::Var(name) => *name == capture_name,
                    Type::Forall { vars, .. } => vars.iter().any(|v| v.name == capture_name),
                    _ => false,
                })
            })
        {
            capture_name.push('_');
        }
        let capture = Binder {
            name: capture_name,
            kind: Kind::CaptureRow,
        };
        let row = Type::Var(capture.name.clone());
        let payload: Vec<Type> = payload
            .iter()
            .map(|t| self.constructor_field(t, &row))
            .collect();
        let mut held = Norm::default();
        for field in &payload {
            let captures = self.captures_of(field);
            held.entries.extend(captures.entries);
            held.tails.extend(captures.tails);
        }
        let captured = Type::Captured {
            ty: Box::new(result),
            captures: Box::new(held.into_type()),
            value: false,
        };
        let ty = payload
            .into_iter()
            .rev()
            .fold(captured, |acc, t| Type::pure_fun(t, acc));
        if ty.contains(&mut |t| matches!(t, Type::Var(name) if *name == capture.name)) {
            Type::Forall {
                vars: vec![capture],
                constraints: Vec::new(),
                body: Box::new(ty),
            }
        } else {
            ty
        }
    }
}
