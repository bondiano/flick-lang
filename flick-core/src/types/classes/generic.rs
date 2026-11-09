//! `deriving Generic`: the instance `Generic (T ā) rep` of a type, whose
//! representation the compiler builds from its constructors. Each is a
//! `Con` of its name holding its fields in pairs ended by `Unit`, labelled
//! `_0`, `_1`, … or, for one record field, by its labels; several are
//! nested `Sum`s. Lowering writes `to_rep` and `from_rep` from the
//! [`GenericInfo`] the instance records.

use super::declare::Declared;
use crate::diagnostics::codes;
use crate::iface::{Entity, Namespace};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::types::Checker;
use crate::types::env::{Form, Instance};
use crate::types::gadt::scheme::is_indexed;
use crate::types::lang::{self, Lang};
use crate::types::ty::{Binder, Type};
use crate::types::typing::{Constructor, GenericInfo};

impl<'n> Checker<'_> {
    /// The instance `deriving Generic` declares for `name` of this module,
    /// with `params`, at `span`; `None` after a diagnostic.
    pub(super) fn generic_instance(
        &mut self,
        decl: &'n Node,
        name: &str,
        params: &[Binder],
        span: Span,
    ) -> Option<Declared<'n>> {
        let module = self.env.module.clone();
        let names = match self.env.type_def(&module, name).map(|d| &d.form) {
            Some(Form::Adt { constructors, .. }) if !constructors.is_empty() => {
                constructors.clone()
            }
            Some(Form::Error) => vec![name.to_owned()],
            _ => {
                let message = format!("`{name}` has no constructors for `Generic` to represent");
                self.error(codes::C013, span, message);
                return None;
            }
        };
        // a sum of products has no constructor that refines its type
        let indexed = names.iter().find(|c| {
            let entity = self.env.entity(Namespace::Constructor, c);
            self.env
                .value(&entity)
                .is_some_and(|s| is_indexed(s, &module, name))
        });
        if let Some(c) = indexed {
            let message = format!(
                "`Generic` represents a type as a sum of products, and the constructor `{c}` of `{name}` is a constructor of a GADT"
            );
            self.error(codes::C013, span, message);
            return None;
        }
        let sum = self.lang_constructors(Lang::GenericSum);
        let [inl, inr] = &sum[..] else {
            return None;
        };
        let (Some(con), Some(field)) = (
            self.lang_constructors(Lang::GenericCon).pop(),
            self.lang_constructors(Lang::GenericField).pop(),
        ) else {
            return None;
        };
        let lang_type = |c: &Constructor| Type::con(&c.module, &c.ty);
        let mut cases = Vec::new();
        let mut reps = Vec::new();
        for c in names {
            let (rep, case) = self.generic_case(name, c, &lang_type(&con), &lang_type(&field));
            reps.push(rep);
            cases.push(case);
        }
        let rep =
            (reps.into_iter().rev()).reduce(|rest, c| Type::apps(lang_type(inl), [c, rest]))?;
        let generic = lang::table().get(Lang::Generic);
        let target = Type::apps(
            Type::con(&module, name),
            params.iter().map(|p| Type::Var(p.name.clone())),
        );
        let head = Type::apps(Type::con(&generic.module, &generic.name), [target, rep]);
        let instance = Instance {
            implicit: false,
            module,
            name: self.show_exact(&head),
            vars: params.to_vec(),
            context: Vec::new(),
            head,
        };
        if !self.admit(&instance, span) {
            return None;
        }
        let info = GenericInfo {
            cases,
            inl: inl.clone(),
            inr: inr.clone(),
            con,
            field,
        };
        Some(Declared {
            decl,
            span,
            instance,
            fields: None,
            generic: Some(info),
            enumerates: None,
        })
    }

    /// The representation of the constructor `c` of `name`, of this module:
    /// a `Con` of its fields, labelled; and the constructor with the labels
    /// of its one record field when it has one.
    fn generic_case(
        &self,
        name: &str,
        c: String,
        con: &Type,
        field: &Type,
    ) -> (Type, (Constructor, Option<Vec<String>>)) {
        let module = self.env.module.clone();
        let fields: Vec<Type> = (self.constructor_fields(&c).iter())
            .map(|t| self.whnf_captured(t).0)
            .collect();
        let (labels, labelled): (Option<Vec<String>>, Vec<(String, Type)>) = match &fields[..] {
            [Type::Record(row)] if row.tail.is_none() => {
                let mut entries = row.entries.clone();
                entries.sort_by(|a, b| a.0.cmp(&b.0));
                (
                    Some(entries.iter().map(|(l, _)| l.clone()).collect()),
                    entries,
                )
            }
            _ => {
                let numbered = fields.iter().enumerate();
                (
                    None,
                    numbered
                        .map(|(i, t)| (format!("_{i}"), t.clone()))
                        .collect(),
                )
            }
        };
        let product = (labelled.into_iter().rev()).fold(Type::lang(Lang::Unit), |rest, (l, t)| {
            let one = Type::apps(field.clone(), [Type::Symbol(l), t]);
            Type::Tuple(vec![one, rest])
        });
        let rep = Type::apps(con.clone(), [Type::Symbol(c.clone()), product]);
        let constructor = Constructor {
            arity: fields.len(),
            rep: self.env.constructor_rep(&module, name, &c),
            module,
            ty: name.to_owned(),
            name: c,
            dicts: 0,
        };
        (rep, (constructor, labels))
    }

    /// The constructors of the type the lang item `lang` marks, in order.
    fn lang_constructors(&self, lang: Lang) -> Vec<Constructor> {
        let item = lang::table().get(lang);
        let (module, ty) = (&item.module, &item.name);
        let names = match self.env.type_def(module, ty).map(|d| &d.form) {
            Some(Form::Adt { constructors, .. }) => constructors.clone(),
            _ => Vec::new(),
        };
        (names.into_iter())
            .map(|name| {
                let entity = Entity {
                    module: module.clone(),
                    namespace: Namespace::Constructor,
                    name: name.clone(),
                };
                Constructor {
                    module: module.clone(),
                    ty: ty.clone(),
                    arity: self.entity_fields(&entity).len(),
                    rep: self.env.constructor_rep(module, ty, &name),
                    name,
                    dicts: 0,
                }
            })
            .collect()
    }
}
