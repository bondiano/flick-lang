//! Schemas from declarations. `deriving Schema` defines the witness `T$schema`
//! of a type: `Schema T`, or for a generic type a function of the witnesses
//! of its parameters, `Schema a -> Schema (Tree a)`, and for a parameter
//! over type constructors a polymorphic witness,
//! `(forall x. Schema x -> Schema (f x)) -> Schema (PairOf f)`. A refined
//! type also has `T$parse` and `T$value`, and with a schema `T$json`. These
//! companions are values of the type's module, named as `T.parse`,
//! `schema T` names the witness.
//!
//! The witness is built by a plan over the type's definition: each field
//! gets the witness of its type, a reference to a type of the same module
//! is lazy (recursion and mutual recursion), and a field of a type without
//! a witness is reported with its path: `Profile.session`.

use std::ptr;

use super::Checker;
use super::kinds::Kind;
use super::ty::{Binder, Type};
use super::typing::{SchemaItem, SchemaPlan, range};

mod form;
mod plan;
mod url;

pub use form::FormField;
pub use url::{UrlRoute, UrlSegment};

use crate::diagnostics::codes;
use crate::iface::{Entity, Namespace};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use plan::PlanContext;

/// A witness parameter: the type variable it is for, and whether it is over
/// type constructors.
#[derive(Debug, Clone)]
pub(super) struct Witness {
    pub(super) var: String,
    pub(super) higher: bool,
}

impl Checker<'_> {
    /// Defines the companions of the module's refined types and the witness
    /// of every type that derives `Schema`.
    pub fn declare_schemas(&mut self, decls: &[&Node], refined: &[(&Node, Type)]) {
        let module = self.env.module.clone();
        let mut deriving: Vec<(&Node, String)> = Vec::new();
        for decl in decls
            .iter()
            .filter(|d| matches!(d.kind, K::TypeDecl | K::ErrorDecl))
        {
            let Some(name) = self.con_token(decl) else {
                continue;
            };
            let binders = self.decl_binders(decl, &name);
            let this = Type::apps(
                Type::con(&module, &name),
                binders.iter().map(|b| Type::Var(b.name.clone())),
            );
            if decl.has_token(K::RefinedKw)
                && let Some((_, base)) = refined
                    .iter()
                    .find(|(p, _)| decl.nodes().any(|n| ptr::eq(n, *p)))
            {
                let result = Type::apps(
                    Type::prim("Result"),
                    [Type::prim("SchemaError"), this.clone()],
                );
                self.companion(
                    &name,
                    "parse",
                    &binders,
                    Type::pure_fun(base.clone(), result),
                );
                self.companion(
                    &name,
                    "value",
                    &binders,
                    Type::pure_fun(this.clone(), base.clone()),
                );
            }
            if derives(decl, self.text, "Schema") {
                deriving.push((decl, name));
            } else {
                for (witness, code) in [("Url", codes::D009), ("Form", codes::D010)] {
                    if derives(decl, self.text, witness) {
                        self.error(
                            code,
                            self.span(decl),
                            format!(
                                "`{name}`: a type that derives `{witness}` derives `Schema` too"
                            ),
                        );
                    }
                }
            }
        }
        // witnesses first, so that plans may refer to any type of the group
        let mut witnesses = Vec::new();
        for (decl, name) in &deriving {
            let params = self.witnesses(decl, name);
            let binders = self.decl_binders(decl, name);
            let scheme = witness_scheme(&module, name, &binders, &params);
            self.companion(name, "schema", &binders, scheme);
            witnesses.push(params);
        }
        let group: Vec<String> = deriving.iter().map(|(_, n)| n.clone()).collect();
        for ((decl, name), params) in deriving.iter().zip(witnesses) {
            let span = decl
                .nodes()
                .find(|n| n.kind == K::DerivingClause)
                .map_or_else(|| self.span(decl), |c| self.span(c));
            let plan = self.definition_plan(decl, name, &params, &group, refined);
            match plan {
                Ok(plan) => {
                    if decl.has_token(K::RefinedKw) {
                        let codec = Type::apps(
                            Type::prim("PureCodec"),
                            [Type::prim("Json"), Type::con(&module, name)],
                        );
                        self.companion(name, "json", &[], codec);
                    }
                    let url = derives(decl, self.text, "Url")
                        .then(|| self.declare_url(name, &plan, params.len(), span))
                        .flatten();
                    let form = derives(decl, self.text, "Form")
                        .then(|| self.declare_form(name, &plan, span))
                        .flatten();
                    self.typing.schemas.push(SchemaItem {
                        ty: name.clone(),
                        decl: range(self.span(decl)),
                        witnesses: params.len(),
                        plan,
                        url,
                        form,
                    });
                }
                Err((path, message)) => {
                    self.error(codes::D001, span, format!("`{path}`: {message}"));
                }
            }
        }
    }

    fn companion(&mut self, ty: &str, member: &str, binders: &[Binder], body: Type) {
        let scheme = if binders.is_empty() {
            body
        } else {
            Type::Forall {
                vars: binders.to_vec(),
                constraints: Vec::new(),
                body: Box::new(body),
            }
        };
        let entity = self
            .env
            .entity(Namespace::Value, &companion_name(ty, member));
        self.env.set_value(entity, scheme);
    }

    /// The parameters of a type that its witness takes: those its
    /// representation mentions, of kind `Type` or `Type -> Type`.
    fn witnesses(&self, decl: &Node, name: &str) -> Vec<Witness> {
        let Some(def) = self.env.type_def(&self.env.module, name) else {
            return Vec::new();
        };
        let mut kinds = Vec::new();
        let mut kind = def.kind.clone();
        while let Kind::Arrow(param, result) = kind {
            kinds.push(*param);
            kind = *result;
        }
        let fields = self.representation(decl, name);
        decl.nodes()
            .filter(|n| n.kind == K::TypeParam)
            .filter_map(|p| p.child_tokens().find(|t| t.kind == K::Ident))
            .zip(kinds)
            .filter_map(|(t, kind)| {
                let var = t.text(self.text).to_owned();
                let used = fields.iter().any(|f| mentions(f, &var));
                let higher =
                    matches!(&kind, Kind::Arrow(p, r) if **p == Kind::Type && **r == Kind::Type);
                (used && (kind == Kind::Type || higher)).then_some(Witness { var, higher })
            })
            .collect()
    }

    /// The parameters of a declaration with their kinds.
    fn decl_binders(&self, decl: &Node, name: &str) -> Vec<Binder> {
        let Some(def) = self.env.type_def(&self.env.module, name) else {
            return Vec::new();
        };
        let mut kinds = Vec::new();
        let mut kind = def.kind.clone();
        while let Kind::Arrow(param, result) = kind {
            kinds.push(*param);
            kind = *result;
        }
        decl.nodes()
            .filter(|n| n.kind == K::TypeParam)
            .filter_map(|p| p.child_tokens().find(|t| t.kind == K::Ident))
            .zip(kinds)
            .map(|(t, kind)| Binder {
                name: t.text(self.text).to_owned(),
                kind,
            })
            .collect()
    }

    /// The field types of a declaration, over its parameters.
    fn representation(&self, decl: &Node, name: &str) -> Vec<Type> {
        let Some(def) = self.env.type_def(&self.env.module, name) else {
            return Vec::new();
        };
        match &def.form {
            super::env::Form::Adt { constructors, .. } => constructors
                .iter()
                .flat_map(|c| self.constructor_fields(c))
                .collect(),
            super::env::Form::Error => self.constructor_fields(name),
            super::env::Form::Alias { body, .. } => vec![body.clone()],
            _ if decl.has_token(K::RefinedKw) => Vec::new(),
            _ => Vec::new(),
        }
    }

    /// The plan of a type's witness, or the path and reason of a field
    /// without one.
    fn definition_plan(
        &mut self,
        decl: &Node,
        name: &str,
        params: &[Witness],
        group: &[String],
        refined: &[(&Node, Type)],
    ) -> Result<SchemaPlan, (String, String)> {
        let module = self.env.module.clone();
        let ctx = PlanContext {
            params,
            group,
            module: &module,
        };
        let def = self
            .env
            .type_def(&module, name)
            .cloned()
            .ok_or_else(|| (name.to_owned(), "no definition".to_owned()))?;
        match &def.form {
            // the value is its field, and so is its wire form
            super::env::Form::Adt {
                constructors,
                abi: super::env::Abi::Unboxed,
            } => {
                let field = constructors
                    .first()
                    .and_then(|c| self.constructor_fields(c).into_iter().next())
                    .ok_or_else(|| (name.to_owned(), "no field".to_owned()))?;
                self.plan(&field, &format!("{name}[0]"), &ctx)
            }
            super::env::Form::Adt {
                abi: super::env::Abi::Untagged(_),
                ..
            } => Err((
                name.to_owned(),
                "an `untagged type` has no derived witness: write it with `Schema` combinators"
                    .to_owned(),
            )),
            super::env::Form::Adt { constructors, .. } => {
                let mut variants = Vec::new();
                for c in constructors {
                    let mut fields = Vec::new();
                    for (i, f) in self.constructor_fields(c).iter().enumerate() {
                        fields.push(self.plan(f, &format!("{name}.{c}[{i}]"), &ctx)?);
                    }
                    variants.push((c.clone(), fields));
                }
                Ok(SchemaPlan::Adt {
                    name: name.to_owned(),
                    variants,
                })
            }
            super::env::Form::Error => {
                let mut fields = Vec::new();
                for (i, f) in self.constructor_fields(name).iter().enumerate() {
                    fields.push(self.plan(f, &format!("{name}[{i}]"), &ctx)?);
                }
                Ok(SchemaPlan::Adt {
                    name: name.to_owned(),
                    variants: vec![(name.to_owned(), fields)],
                })
            }
            super::env::Form::Alias { body, opaque, .. } => {
                if *opaque {
                    return Err((
                        name.to_owned(),
                        "an opaque type's witness is its owner's to give: write it with `Schema` combinators".to_owned(),
                    ));
                }
                self.plan(body, name, &ctx)
            }
            _ if decl.has_token(K::RefinedKw) => {
                let base = refined
                    .iter()
                    .find(|(p, _)| decl.nodes().any(|n| ptr::eq(n, *p)))
                    .map(|(p, b)| ((*p).clone(), b.clone()));
                let Some((predicate, base)) = base else {
                    return Err((name.to_owned(), "a refined type needs its base".to_owned()));
                };
                let base = self.plan(&base, name, &ctx)?;
                Ok(SchemaPlan::Refined {
                    name: name.to_owned(),
                    base: Box::new(base),
                    known: known_check(&predicate, self.text),
                })
            }
            _ => Err((
                name.to_owned(),
                "an abstract type has no definition to derive a schema from".to_owned(),
            )),
        }
    }
}

/// `W1 -> … -> Schema (T params)`: a witness per parameter that needs one.
fn witness_scheme(module: &str, name: &str, binders: &[Binder], params: &[Witness]) -> Type {
    let all = binders.iter().map(|b| Type::Var(b.name.clone()));
    let result = Type::app(
        Type::prim("Schema"),
        Type::apps(Type::con(module, name), all),
    );
    params.iter().rev().fold(result, |acc, w| {
        let param = if w.higher {
            let x = Type::Var("x$".to_owned());
            Type::Forall {
                vars: vec![Binder {
                    name: "x$".to_owned(),
                    kind: Kind::Type,
                }],
                constraints: Vec::new(),
                body: Box::new(Type::pure_fun(
                    Type::app(Type::prim("Schema"), x.clone()),
                    Type::app(Type::prim("Schema"), Type::app(Type::Var(w.var.clone()), x)),
                )),
            }
        } else {
            Type::app(Type::prim("Schema"), Type::Var(w.var.clone()))
        };
        Type::pure_fun(param, acc)
    })
}

/// The name of a companion value of type `ty`: `Email$parse`.
pub fn companion_name(ty: &str, member: &str) -> String {
    format!("{ty}${member}")
}

impl Checker<'_> {
    /// The witness plan of a value of type `ty` the module sends, named
    /// `what` in a message.
    pub(super) fn plan_of(&self, ty: &Type, what: &str) -> Result<SchemaPlan, (String, String)> {
        let module = self.env.module.clone();
        let ctx = PlanContext {
            params: &[],
            group: &[],
            module: &module,
        };
        self.plan(ty, what, &ctx)
    }
}

/// Whether a `deriving` clause of `decl` names `witness`.
fn derives(decl: &Node, text: &str, witness: &str) -> bool {
    decl.nodes()
        .filter(|n| n.kind == K::DerivingClause)
        .flat_map(Node::child_tokens)
        .any(|t| t.kind == K::Con && t.text(text) == witness)
}

/// A check exporters and the compiler know: `Checks.email`.
fn known_check(predicate: &Node, text: &str) -> Option<String> {
    let body = predicate.nodes().next()?;
    let written: String = body.text(text).split_whitespace().collect();
    written
        .strip_prefix("Checks.")
        .filter(|c| matches!(*c, "email" | "non_empty" | "positive"))
        .map(str::to_owned)
}

/// Whether `t` mentions the variable `var`.
fn mentions(t: &Type, var: &str) -> bool {
    let mut found = false;
    t.visit(&mut |c| {
        if matches!(c, Type::Var(v) if v == var) {
            found = true;
        }
    });
    found
}

/// The witness of `schema T`, in `T`'s module.
pub fn witness_entity(module: &str, ty: &str) -> Entity {
    Entity {
        module: module.to_owned(),
        namespace: Namespace::Value,
        name: companion_name(ty, "schema"),
    }
}

impl SchemaItem {
    pub fn name(&self) -> String {
        companion_name(&self.ty, "schema")
    }
}
