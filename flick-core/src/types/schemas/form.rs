//! The form of a record type that derives `Form`: a text input per field.
//! A field is text, a number, a boolean or a refined type over them, which
//! the type's schema checks, or an `Option` of one, empty when its text is.

use crate::diagnostics::codes;
use crate::source::Span;
use crate::types::Checker;
use crate::types::lang::Lang;
use crate::types::ty::{Row, Type};
use crate::types::typing::SchemaPlan;

/// A field of a form: its label, the witness of its value, and whether it
/// is an `Option`.
pub type FormField = (String, SchemaPlan, bool);

impl Checker<'_> {
    /// `T$form` and `T$fields` of a type that derives `Form`, or
    /// `FLICK-D010` at `span`.
    pub(super) fn declare_form(
        &mut self,
        name: &str,
        plan: &SchemaPlan,
        span: Span,
    ) -> Option<Vec<FormField>> {
        match self.form_fields(name, plan) {
            Ok(fields) => {
                let this = Type::con(&self.env.module, name);
                self.companion(name, "form", &[], Type::app(Type::lang(Lang::Form), this));
                let field = Type::con("Form", "Field");
                let record = fields.iter().map(|(l, ..)| (l.clone(), field.clone()));
                let ty = Type::Record(Row::closed(record.collect()));
                self.companion(name, "fields", &[], ty);
                Some(fields)
            }
            Err((path, message)) => {
                self.error(codes::D010, span, format!("`{path}`: {message}"));
                None
            }
        }
    }

    /// The fields of `name`'s form from its witness plan, or the path and
    /// reason of a field without a text input.
    pub(super) fn form_fields(
        &self,
        name: &str,
        plan: &SchemaPlan,
    ) -> Result<Vec<FormField>, (String, String)> {
        let SchemaPlan::Record(fields) = plan else {
            return Err((name.to_owned(), "a form is of a record type".to_owned()));
        };
        fields
            .iter()
            .map(|(label, plan)| match plan {
                SchemaPlan::Option(inner) if self.is_scalar(inner) => {
                    Ok((label.clone(), (**inner).clone(), true))
                }
                _ if self.is_scalar(plan) => Ok((label.clone(), plan.clone(), false)),
                _ => Err((
                    format!("{name}.{label}"),
                    "a form field is a `String`, a `Float`, a `Bool`, a refined type over them or an `Option` of one".to_owned(),
                )),
            })
            .collect()
    }
}
