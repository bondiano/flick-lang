//! `deriving Eq, Ord, Hash, Show`: an instance over the fields of every
//! constructor, requiring the class of each type parameter. `Hash` and `Ord`
//! are derived only together with `Eq`, so all of them use the same fields.
//! `deriving Cli.Value` parses a choice among nullary constructors, or the
//! base of a refined type through its predicate.

use std::ptr;

use super::declare::Declared;
use super::solve::Outcome;
use super::{Dict, STRUCTURAL, has_metas};
use crate::diagnostics::{Diagnostic, codes};
use crate::iface::Namespace;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::Checker;
use crate::types::cli::CLI;
use crate::types::env::{Form, Instance};
use crate::types::kinds::Kind;
use crate::types::ty::{Binder, PRELUDE, Type};

impl<'n> Checker<'_> {
    pub(super) fn derive(
        &mut self,
        decl: &'n Node,
        refined: &[(&Node, Type)],
    ) -> Vec<Declared<'n>> {
        let Some(clause) = decl.nodes().find(|n| n.kind == K::DerivingClause) else {
            return Vec::new();
        };
        let span = self.span(clause);
        let module = self.env.module.clone();
        let Some(name) = self.con_token(decl) else {
            return Vec::new();
        };
        let Some(def) = self.env.type_def(&module, &name).cloned() else {
            return Vec::new();
        };
        let classes = self.derived_classes(clause);
        if classes.is_empty() {
            return Vec::new();
        }
        let Some(fields) = self.fields_of(decl, &name, &def.form, refined) else {
            return Vec::new();
        };
        let has_eq = classes.iter().any(|(_, c)| c == "Eq");
        let mut kinds = Vec::new();
        let mut kind = def.kind.clone();
        while let Kind::Arrow(param, result) = kind {
            kinds.push(*param);
            kind = *result;
        }
        let params: Vec<Binder> = decl
            .nodes()
            .filter(|n| n.kind == K::TypeParam)
            .filter_map(|p| p.child_tokens().find(|t| t.kind == K::Ident))
            .zip(kinds)
            .map(|(t, kind)| Binder {
                name: t.text(self.text).to_owned(),
                kind,
            })
            .collect();
        let mut out = Vec::new();
        for (class_module, class) in classes {
            if class_module == CLI && !self.cli_value_derivable(decl, &name, &def.form, span) {
                continue;
            }
            if (class == "Hash" || class == "Ord") && !has_eq {
                self.error(
                    codes::C013,
                    span,
                    format!(
                        "derive `Eq` together with `{class}`, so that both use the same fields"
                    ),
                );
                continue;
            }
            let con = Type::con(&class_module, &class);
            let head = Type::app(
                con.clone(),
                Type::apps(
                    Type::con(&module, &name),
                    params.iter().map(|p| Type::Var(p.name.clone())),
                ),
            );
            let context = params
                .iter()
                .filter(|p| p.kind == Kind::Type)
                .map(|p| Type::app(con.clone(), Type::Var(p.name.clone())))
                .collect();
            let instance = Instance {
                module: module.clone(),
                name: self.show(&head),
                vars: params.clone(),
                context,
                head,
            };
            if self.admit(&instance, span) {
                out.push(Declared {
                    decl,
                    span,
                    instance,
                    fields: Some(fields.clone()),
                });
            }
        }
        out
    }

    /// The types of the fields of a declaration, which derived instances
    /// cover; `None` after a diagnostic.
    fn fields_of(
        &mut self,
        decl: &Node,
        name: &str,
        form: &Form,
        refined: &[(&Node, Type)],
    ) -> Option<Vec<Type>> {
        let span = decl
            .nodes()
            .find(|n| n.kind == K::DerivingClause)
            .map_or_else(|| self.span(decl), |c| self.span(c));
        let message = match form {
            Form::Adt { constructors, .. } => {
                return Some(
                    constructors
                        .iter()
                        .flat_map(|c| self.constructor_fields(c))
                        .collect(),
                );
            }
            Form::Error => return Some(self.constructor_fields(name)),
            Form::Alias {
                body, opaque: true, ..
            } => return Some(vec![body.clone()]),
            Form::Nominal if decl.has_token(K::RefinedKw) => {
                let base = refined
                    .iter()
                    .find(|(predicate, _)| decl.nodes().any(|n| ptr::eq(n, *predicate)));
                return Some(base.map(|(_, b)| vec![b.clone()]).unwrap_or_default());
            }
            Form::Alias { .. } => format!(
                "`{name}` is a transparent alias: its form already has the structural instances, so there is nothing to derive"
            ),
            _ => format!("`{name}` has no definition to derive instances from"),
        };
        self.error(codes::C013, span, message);
        None
    }

    /// The classes a `deriving` clause names; others are reported.
    fn derived_classes(&mut self, clause: &Node) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let tokens: Vec<_> = clause
            .child_tokens()
            .filter(|t| !matches!(t.kind, K::DerivingKw))
            .collect();
        for path in tokens.split(|t| t.kind == K::Comma) {
            let parts: Vec<&str> = path
                .iter()
                .filter(|t| t.kind != K::Dot)
                .map(|t| t.text(self.text))
                .collect();
            let Some((last, qualifier)) = parts.split_last() else {
                continue;
            };
            let found = self.find_type(clause, &qualifier.join("."), last);
            // the class name has no node of its own: hover and rename find it by its token
            if let (Some((module, _)), Some(token)) = (&found, path.last()) {
                let named = (module.clone(), (*last).to_owned());
                self.typing
                    .type_names
                    .insert((token.start, token.end), named);
            }
            match found {
                Some((module, def))
                    if module == PRELUDE
                        && matches!(def.form, Form::Class { .. })
                        && STRUCTURAL.contains(last) =>
                {
                    out.push((PRELUDE.to_owned(), (*last).to_owned()));
                }
                Some((module, def))
                    if module == CLI && *last == "Value" && matches!(def.form, Form::Class { .. }) =>
                {
                    out.push((CLI.to_owned(), "Value".to_owned()));
                }
                // witnesses, not classes: see `types::schemas`
                Some((module, _)) if module == PRELUDE && matches!(*last, "Schema" | "Url" | "Form") => {}
                None => {}
                Some(_) => self.error(
                    codes::C013,
                    self.span(clause),
                    format!(
                        "`{last}` cannot be derived: deriving covers `Eq`, `Ord`, `Hash`, `Show` and `Cli.Value`"
                    ),
                ),
            }
        }
        out
    }

    /// `Cli.Value` is derived for constructors without fields, a choice
    /// among their names, or for a refined type, whose base parses.
    fn cli_value_derivable(&mut self, decl: &Node, name: &str, form: &Form, span: Span) -> bool {
        let ok = match form {
            Form::Adt { constructors, .. } => constructors
                .iter()
                .all(|c| self.constructor_fields(c).is_empty()),
            Form::Nominal => decl.has_token(K::RefinedKw),
            _ => false,
        };
        if !ok {
            self.error(
                codes::C013,
                span,
                format!("cannot derive `Cli.Value` for `{name}`: it is derived for a choice among constructors without fields, or for a refined type"),
            );
        }
        ok
    }

    /// The field types of a constructor of this module, over the type's parameters.
    pub(crate) fn constructor_fields(&self, constructor: &str) -> Vec<Type> {
        let entity = self.env.entity(Namespace::Constructor, constructor);
        let mut ty = match self.env.value(&entity) {
            Some(Type::Forall { body, .. }) => (**body).clone(),
            Some(t) => t.clone(),
            None => return Vec::new(),
        };
        let mut fields = Vec::new();
        while let Type::Fun { param, result, .. } = ty {
            fields.push(*param);
            ty = *result;
        }
        fields
    }

    /// Each field of a derived instance needs the class too; the dictionary
    /// of each field.
    pub(super) fn derived_fields(&mut self, d: &Declared, fields: &[Type]) -> Vec<Dict> {
        let (head, _) = d.instance.head.spine();
        let head = head.clone();
        let class = super::split(&d.instance.head).map(|((_, n), _)| n.to_owned());
        let mut dicts = Vec::new();
        for field in fields {
            let c = Type::app(head.clone(), field.clone());
            let outcome = self.entails(&c, &d.instance.context);
            if let Outcome::Solved(dict) = outcome {
                dicts.push(dict);
                continue;
            }
            if has_metas(field) {
                dicts.push(Dict::Error);
                continue;
            }
            // the field's type as written: its implicit capture row is not
            // part of what the reader declared
            let (bare, _) = self.whnf_captured(field);
            let (shown, class) = (self.show(&bare), class.clone().unwrap_or_default());
            let name = &d.instance.name;
            self.diagnostics.push(
                Diagnostic::error(
                    codes::C013,
                    format!("cannot derive `{name}`: a field of type `{shown}` has no `{class}` instance"),
                    d.span,
                ).with_fix(|| (format!("write `instance {name}` by hand, or wrap the field in a nominal type with its own `{class}`"), Vec::new())),
            );
            return dicts;
        }
        dicts
    }
}
