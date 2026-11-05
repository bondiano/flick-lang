//! `unboxed type` and `untagged type`: a variant type that names its JS
//! representation. An unboxed type is its one constructor's one field; an
//! untagged one is the one field of each constructor, which JS tells apart
//! by type. The checker makes sure the values can be told apart, so
//! matching stays exact.

use super::Checker;
use super::env::{Abi, Form, JsClass};
use super::ty::Type;
use crate::diagnostics::codes;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::Lang;

impl Checker<'_> {
    /// Checks the representation a type declaration names and records it.
    pub(super) fn check_abi(&mut self, decl: &Node, name: &str) {
        // an enum, or a literal written as a field, reported where it is read
        if self.enum_literals(decl, name) || decl.kind != K::TypeDecl {
            return;
        }
        let Some(word) = decl
            .child_tokens()
            .take_while(|t| t.kind != K::TypeKw)
            .find(|t| t.kind == K::Ident)
            .map(|t| t.text(self.text).to_owned())
        else {
            return;
        };
        let span = self.span(decl);
        let gadt = decl.nodes().filter(|n| n.kind == K::Variant).any(|v| {
            v.nodes()
                .any(|n| matches!(n.kind, K::VariantResult | K::Context))
        });
        if gadt {
            self.error(
                codes::T056,
                span,
                format!("`{word} type` names the JS form of its values, which a constructor of a GADT does not have: `{name}`"),
            );
            return;
        }
        let module = self.env.module.clone();
        let Some(Form::Adt { constructors, .. }) =
            self.env.type_def(&module, name).map(|d| &d.form)
        else {
            self.error(
                codes::T020,
                span,
                format!("`{word} type` names the representation of a variant type: `{name}` has no constructors"),
            );
            return;
        };
        let constructors = constructors.clone();
        let fields: Vec<Vec<Type>> = constructors
            .iter()
            .map(|c| self.constructor_fields(c))
            .collect();
        let abi = if word == "unboxed" {
            if !matches!(&fields[..], [one] if one.len() == 1) {
                self.error(
                    codes::T020,
                    span,
                    format!("an `unboxed type` has one constructor with one field, its values: `{name}` has not"),
                );
                return;
            }
            Abi::Unboxed
        } else {
            let mut classes: Vec<JsClass> = Vec::new();
            for (c, fields) in constructors.iter().zip(&fields) {
                let [field] = &fields[..] else {
                    self.error(
                        codes::T020,
                        span,
                        format!(
                            "each constructor of an `untagged type` has one field: `{c}` has {}",
                            fields.len()
                        ),
                    );
                    return;
                };
                let Some(class) = self.js_class(field) else {
                    let shown = self.show(field);
                    self.error(
                        codes::T020,
                        span,
                        format!(
                            "the field `{shown}` of `{c}` has no JS type of its own: an `untagged type` holds `Float`, `String`, `Bool`, `BigInt` or one list or tuple"
                        ),
                    );
                    return;
                };
                if let Some(i) = classes.iter().position(|k| *k == class) {
                    self.error(
                        codes::T020,
                        span,
                        format!(
                            "`{}` and `{c}` both hold a JS {}: their values could not be told apart",
                            constructors[i],
                            class.name()
                        ),
                    );
                    return;
                }
                classes.push(class);
            }
            if classes.len() < 2 {
                self.error(
                    codes::T020,
                    span,
                    format!("an `untagged type` has two constructors at least; for one write `unboxed type {name}`"),
                );
                return;
            }
            Abi::Untagged(classes)
        };
        self.record_abi(name, abi);
    }

    /// Keeps the representation of the type `name`.
    pub(super) fn record_abi(&mut self, name: &str, abi: Abi) {
        self.typing.abis.insert(name.to_owned(), abi.clone());
        if let Some(def) = self.env.type_def_mut(name)
            && let Form::Adt { abi: slot, .. } = &mut def.form
        {
            *slot = abi;
        }
    }

    /// What JS type every value of `t` has, if one does.
    fn js_class(&self, t: &Type) -> Option<JsClass> {
        let t = self.whnf(t);
        if let Type::Tuple(_) = t {
            return Some(JsClass::Array);
        }
        let Type::Con { module, name } = t.spine().0 else {
            return None;
        };
        if t.is_lang(Lang::Int) || t.is_lang(Lang::Float) {
            return Some(JsClass::Number);
        }
        if t.is_lang(Lang::String) {
            return Some(JsClass::String);
        }
        if t.is_lang(Lang::Bool) {
            return Some(JsClass::Boolean);
        }
        if t.is_lang(Lang::BigInt) {
            return Some(JsClass::Bigint);
        }
        if t.spine().0.is_lang(Lang::List) {
            return Some(JsClass::Array);
        }
        // an unboxed wrapper is its field
        match &self.env.type_def(module, name)?.form {
            Form::Adt {
                constructors,
                abi: Abi::Unboxed,
            } => {
                let field = self
                    .constructor_fields(constructors.first()?)
                    .into_iter()
                    .next()?;
                if t.spine().1.is_empty() {
                    self.js_class(&field)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}
