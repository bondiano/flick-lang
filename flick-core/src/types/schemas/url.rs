//! Routes of a type that derives `Url`: a constructor is its name in kebab
//! case (`Home` and `Index` are the root), its fields the segments after it,
//! and a final record field the query. A segment is text, a number, a
//! boolean or a refined type over them, which the type's schema checks, or
//! a type whose constructors have no fields, each its name in kebab case.
//! The last segment may be an `Option`: `None` leaves it out of the URL.

use crate::diagnostics::codes;
use crate::iface::{Entity, Namespace};
use crate::source::Span;
use crate::types::Checker;
use crate::types::env::{Abi, Form};
use crate::types::ty::Type;
use crate::types::typing::SchemaPlan;

/// A constructor of a type that derives `Url`.
#[derive(Debug, Clone, PartialEq)]
pub struct UrlRoute {
    pub constructor: String,
    /// The first segment, in kebab case; empty for the root.
    pub path: String,
    /// The fields that are the following segments.
    pub segments: Vec<UrlSegment>,
    /// Whether the last segment is an `Option`, left out of the URL for `None`.
    pub optional: bool,
    /// The fields of a final record field, the query: a label, the witness
    /// of the value, and whether the field is an `Option`.
    pub query: Option<Vec<(String, SchemaPlan, bool)>>,
}

/// A segment of a route.
#[derive(Debug, Clone, PartialEq)]
pub enum UrlSegment {
    /// A scalar field, by its witness.
    Value(SchemaPlan),
    /// A field of type `ty` of `module`, whose constructors have no fields:
    /// each is its name in kebab case.
    Words {
        module: String,
        ty: String,
        constructors: Vec<String>,
    },
}

impl UrlSegment {
    /// A constructor's text in a URL: its name in kebab case.
    pub fn word(constructor: &str) -> String {
        kebab(constructor)
    }
}

impl Checker<'_> {
    /// `T$url` of a type that derives `Url`, or `FLICK-D009` at `span`.
    pub(super) fn declare_url(
        &mut self,
        name: &str,
        plan: &SchemaPlan,
        witnesses: usize,
        span: Span,
    ) -> Option<Vec<UrlRoute>> {
        match self.url_routes(name, plan, witnesses) {
            Ok(routes) => {
                let this = Type::con(&self.env.module, name);
                self.companion(name, "url", &[], Type::app(Type::prim("Url"), this));
                Some(routes)
            }
            Err((path, message)) => {
                self.error(codes::D009, span, format!("`{path}`: {message}"));
                None
            }
        }
    }

    /// The routes of `name`'s constructors from its witness plan, or the
    /// path and reason of a field without a URL form.
    pub(super) fn url_routes(
        &self,
        name: &str,
        plan: &SchemaPlan,
        witnesses: usize,
    ) -> Result<Vec<UrlRoute>, (String, String)> {
        let SchemaPlan::Adt { variants, .. } = plan else {
            return Err((name.to_owned(), "a route type is a variant type".to_owned()));
        };
        if witnesses > 0 {
            return Err((name.to_owned(), "a generic type has no URL form".to_owned()));
        }
        let mut routes: Vec<UrlRoute> = Vec::new();
        for (constructor, fields) in variants {
            let at = |i: usize| format!("{name}.{constructor}[{i}]");
            let (query, plans) = match fields.split_last() {
                Some((SchemaPlan::Record(query), rest)) => (Some(query), rest),
                _ => (None, &fields[..]),
            };
            let optional = matches!(plans.last(), Some(SchemaPlan::Option(_)));
            let segments = plans
                .iter()
                .enumerate()
                .map(|(i, plan)| {
                    let plan = match plan {
                        SchemaPlan::Option(inner) if i + 1 == plans.len() => &**inner,
                        _ => plan,
                    };
                    self.segment(plan).ok_or_else(|| {
                        (
                            at(i),
                            "a segment is a `String`, a `Float`, a `Bool`, a refined type over them or a type whose constructors have no fields; only the last may be an `Option`"
                                .to_owned(),
                        )
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let query = query
                .map(|query| {
                    query
                        .iter()
                        .map(|(label, plan)| match plan {
                            SchemaPlan::Option(inner) if self.is_scalar(inner) => {
                                Ok((label.clone(), (**inner).clone(), true))
                            }
                            _ if self.is_scalar(plan) => Ok((label.clone(), plan.clone(), false)),
                            _ => Err((
                                format!("{}.{label}", at(segments.len())),
                                "a query parameter is a segment's type or an `Option` of one"
                                    .to_owned(),
                            )),
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?;
            let path = if matches!(constructor.as_str(), "Home" | "Index") {
                String::new()
            } else {
                kebab(constructor)
            };
            // the counts of segments a route's URLs have
            let counts = |n: usize, optional: bool| (n - usize::from(optional))..=n;
            let mine = counts(segments.len(), optional);
            if let Some(other) = routes.iter().find(|r| {
                let theirs = counts(r.segments.len(), r.optional);
                r.path == path && mine.start() <= theirs.end() && theirs.start() <= mine.end()
            }) {
                return Err((
                    format!("{name}.{constructor}"),
                    format!("its URLs are those of `{}`", other.constructor),
                ));
            }
            routes.push(UrlRoute {
                constructor: constructor.clone(),
                path,
                segments,
                optional,
                query,
            });
        }
        Ok(routes)
    }

    /// The segment of a field of the witness `plan`, if it has a URL form.
    fn segment(&self, plan: &SchemaPlan) -> Option<UrlSegment> {
        if self.is_scalar(plan) {
            return Some(UrlSegment::Value(plan.clone()));
        }
        let SchemaPlan::Named {
            module, ty, args, ..
        } = plan
        else {
            return None;
        };
        let Some(Form::Adt {
            constructors,
            abi: Abi::Tagged,
        }) = self.env.type_def(module, ty).map(|d| &d.form)
        else {
            return None;
        };
        let nullary = |name: &String| {
            let entity = Entity {
                module: module.clone(),
                namespace: Namespace::Constructor,
                name: name.clone(),
            };
            match self.env.value(&entity) {
                Some(Type::Forall { body, .. }) => !matches!(**body, Type::Fun { .. }),
                Some(ty) => !matches!(ty, Type::Fun { .. }),
                None => false,
            }
        };
        let mut words: Vec<String> = constructors.iter().map(|c| kebab(c)).collect();
        words.sort();
        words.dedup();
        (args.is_empty()
            && !constructors.is_empty()
            && words.len() == constructors.len()
            && constructors.iter().all(nullary))
        .then(|| UrlSegment::Words {
            module: module.clone(),
            ty: ty.clone(),
            constructors: constructors.clone(),
        })
    }

    /// Text, a number, a boolean, or a refined type, whose schema checks it.
    pub(super) fn is_scalar(&self, plan: &SchemaPlan) -> bool {
        match plan {
            SchemaPlan::Prim(p) => matches!(p.as_str(), "String" | "Int" | "Float" | "Bool"),
            SchemaPlan::Refined { base, .. } => self.is_scalar(base),
            SchemaPlan::Named {
                module, ty, args, ..
            } => {
                args.is_empty()
                    && self
                        .env
                        .type_def(module, ty)
                        .is_some_and(|def| matches!(def.form, Form::Nominal))
            }
            _ => false,
        }
    }
}

/// `UserProfile` → `user-profile`.
fn kebab(name: &str) -> String {
    let mut out = String::new();
    let mut previous_lower = false;
    for c in name.chars() {
        if c.is_uppercase() && previous_lower {
            out.push('-');
        }
        previous_lower = c.is_lowercase() || c.is_ascii_digit();
        out.extend(c.to_lowercase());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::kebab;

    #[test]
    fn constructors_are_kebab_case() {
        assert_eq!(kebab("UserProfile"), "user-profile");
        assert_eq!(kebab("Search"), "search");
        assert_eq!(kebab("Page2Edit"), "page2-edit");
    }
}
