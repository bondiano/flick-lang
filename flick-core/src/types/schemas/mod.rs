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
use super::typing::{SchemaItem, SchemaPlan};

mod plan;

use crate::iface::{Entity, Namespace};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::Lang;
use plan::PlanContext;

/// A witness parameter: the type variable it is for, and whether it is over
/// type constructors.
#[derive(Debug, Clone)]
pub(super) struct Witness {
    pub(super) var: String,
    pub(super) higher: bool,
}

impl Checker<'_> {
    /// Defines the companions of the module's refined types, `T$parse` and
    /// `T$value`; the witnesses of `deriving Schema`, `Url` and `Form` are
    /// the code std's plugin gives.
    pub fn declare_schemas(&mut self, decls: &[&Node], refined: &[(&Node, Type)]) {
        let module = self.env.module.clone();
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
                    Type::lang(Lang::Result),
                    [Type::lang(Lang::SchemaError), this.clone()],
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
