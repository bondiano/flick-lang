//! Which names in a row are sets, and what is said when a name is written as
//! the other thing: a set without `...`, or one element with it.

use super::Free;
use crate::diagnostics::{Diagnostic, Edit, codes};
use crate::iface::Namespace;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::Checker;
use crate::types::env::{Form, Found, TypeDef};
use crate::types::kinds::Kind;
use crate::types::ty::Type;

impl Checker<'_> {
    pub(super) fn not_a_set(&mut self, node: &Node, item: &Node, free: Free) {
        // an alias whose kind is still unknown is in a cycle, which name resolution reports
        if let Some(TypeDef {
            form: Form::Alias { .. },
            kind: Kind::Meta(_),
        }) = self.declared(item)
        {
            return;
        }
        let reported = self.diagnostics.len();
        let unknown = self.kinds.fresh();
        let ty = self.ty(item, &unknown, free);
        if self.diagnostics.len() > reported || matches!(ty, Type::Error) {
            return;
        }
        let text = self.text_of(item);
        let span = self.span(item);
        if !matches!(ty.spine().0, Type::Con { .. }) {
            self.error(
                codes::T002,
                span,
                format!(
                    "`...` merges a set, and `{text}` is a row variable: write it after `|`, as `{{A | {text}}}`"
                ),
            );
            return;
        }
        let dots = node.child_tokens().find(|t| t.kind == K::DotDotDot);
        let Some(dots) = dots.filter(|_| self.row_side(item).is_some()) else {
            self.error(
                codes::T002,
                span,
                format!(
                    "`{text}` is not a set: a set is a `type alias` of a row of services and errors"
                ),
            );
            return;
        };
        let edits = vec![Edit {
            span: self.token_span(dots),
            replacement: String::new(),
        }];
        self.diagnostics.push(
            Diagnostic::error(
                codes::T002,
                format!("`{text}` is one element, not a set"),
                span,
            )
            .with_safe_fix(|| (format!("write `{text}` without `...`"), edits)),
        );
    }

    /// A set written as one element; the fix may also take away `semi`, a `;`
    /// the row no longer needs.
    pub(super) fn set_needs_dots(&mut self, item: &Node, semi: Option<Span>) {
        let text = self.text_of(item);
        let span = self.span(item);
        let mut edits = vec![Edit {
            span: Span::new(span.file, span.start, span.start),
            replacement: "...".to_owned(),
        }];
        edits.extend(semi.map(|span| Edit {
            span,
            replacement: String::new(),
        }));
        self.diagnostics.push(
            Diagnostic::error(
                codes::T002,
                format!("`{text}` is a set, not one element"),
                span,
            )
            .with_safe_fix(|| (format!("write `...{text}`"), edits)),
        );
    }

    pub(super) fn services_after_semi(&mut self, item: &Node) {
        let text = self.text_of(item);
        self.error(
            codes::T002,
            self.span(item),
            format!("`{text}` holds services, which go before `;`"),
        );
    }

    /// The kind of the row `item` names, when it is an alias of one.
    pub(super) fn set_kind(&self, item: &Node) -> Option<Kind> {
        let def = self.declared(item)?;
        if !matches!(def.form, Form::Alias { .. }) {
            return None;
        }
        let kind = self.alias_result(def);
        matches!(kind, Kind::ServiceRow | Kind::ErrorRow).then_some(kind)
    }

    /// What `item` holds, as far as its declaration tells: services or errors.
    pub(super) fn row_side(&self, item: &Node) -> Option<Kind> {
        let item = if item.kind == K::RowSpread {
            item.nodes().next()?
        } else {
            item
        };
        let def = self.declared(item)?;
        match &def.form {
            Form::Service { .. } => Some(Kind::ServiceRow),
            Form::Error => Some(Kind::ErrorRow),
            Form::Alias { body, .. } => match self.alias_result(def) {
                Kind::ServiceRow | Kind::Service => Some(Kind::ServiceRow),
                Kind::ErrorRow => Some(Kind::ErrorRow),
                Kind::Type => match body.spine().0 {
                    Type::Con { module, name }
                        if matches!(
                            self.env.type_def(module, name).map(|d| &d.form),
                            Some(Form::Error)
                        ) =>
                    {
                        Some(Kind::ErrorRow)
                    }
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        }
    }

    fn alias_result(&self, def: &TypeDef) -> Kind {
        let mut kind = self.kinds.resolve(&def.kind);
        while let Kind::Arrow(_, result) = kind {
            kind = self.kinds.resolve(&result);
        }
        kind
    }

    /// The declaration of the name `item` is or applies, without a diagnostic.
    fn declared(&self, item: &Node) -> Option<&TypeDef> {
        let head = if item.kind == K::AppType {
            item.nodes().next()?
        } else {
            item
        };
        if head.kind != K::NameType {
            return None;
        }
        let tokens = head.child_tokens().filter(|t| t.kind != K::Dot);
        let parts: Vec<&str> = tokens.map(|t| t.text(self.text)).collect();
        let (name, qualifier) = parts.split_last()?;
        let module = if qualifier.is_empty() {
            let Found::Entity(entity) = self.env.find(Namespace::Type, name) else {
                return None;
            };
            entity.module
        } else {
            let (_, interface) = self.env.qualified_module(&qualifier.join("."))?;
            let export = interface?.find(Namespace::Type, name)?;
            export.entity.module.clone()
        };
        self.env.type_def(&module, name)
    }
}
