//! Rows in types: the service and error rows of a runtime arrow, row types in
//! braces, record fields and capture qualifiers.

use super::{Free, is_type, with_article};
use crate::diagnostics::codes;
use crate::source::{FileId, Span};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::env::Form;
use crate::types::kinds::Kind;
use crate::types::lang::{self, Lang};
use crate::types::services::OpClass;
use crate::types::ty::{Effects, Row, Shape, Type, Written};
use crate::types::{Checker, ty};

impl Checker<'_> {
    /// `{r; e}` of a runtime arrow; `{}` leaves both rows empty.
    pub(super) fn effects(&mut self, node: &Node, free: Free) -> Effects {
        let mut rows = node.nodes().filter(|n| n.kind == K::Row);
        let (services, errors) = (rows.next(), rows.next());
        let (services, errors) = self.slots(services, errors, free);
        Effects {
            written: self.row_written(node),
            direct: self.direct_row(&services, &errors),
            ..Effects::new(services, errors)
        }
    }

    /// Only intrinsically synchronous UI rows are atomic. A closed runtime
    /// row can still recurse forever: scheduler preemption is independent of
    /// whether its operations can suspend.
    pub(in crate::types) fn direct_row(&self, services: &Type, errors: &Type) -> bool {
        self.options.stages_by_class && ty::sync_rows(services, errors)
    }

    /// Whether the service or effect `module.name` is known, is not `Async` and
    /// has no `control` operation.
    pub(in crate::types) fn never_suspends(&self, module: &str, name: &str) -> bool {
        let local = module == self.env.module && self.control_effects.contains(name);
        !local
            && !lang::is(Lang::Async, module, name)
            && matches!(
                self.env.type_def(module, name).map(|d| &d.form),
                Some(Form::Service { operations, .. })
                    if operations.iter().all(|op| op.class != OpClass::Control)
            )
    }

    /// Where `node`, an effect annotation, is written, for a fix that adds
    /// to its services; none for `none` and `r1 + r2`.
    fn row_written(&self, node: &Node) -> Written {
        let Some(brace) = node.child_tokens().find(|t| t.kind == K::LBrace) else {
            return Written::default();
        };
        let shape = match node.nodes().find(|n| n.kind == K::Row) {
            None => Shape::Empty,
            Some(row) => {
                // what a fix adds to a row with `-` could be what `-` takes out
                if row.nodes().any(|n| n.kind == K::RowOmit) {
                    return Written::default();
                }
                let items: Vec<&Node> = row.nodes().filter(|n| is_type(n.kind)).collect();
                let tail = row.nodes().any(|n| n.kind == K::RowTail);
                let spread = row.nodes().any(|n| n.kind == K::RowSpread);
                let variable = |n: &Node| {
                    n.kind == K::NameType
                        && !n.has_token(K::NoneKw)
                        && (n.text(self.text).trim()).starts_with(|c: char| c.is_ascii_lowercase())
                };
                match items[..] {
                    [] if !spread => Shape::NoServices,
                    [only] if !tail && !spread && variable(only) => Shape::TailOnly,
                    _ if items
                        .iter()
                        .any(|n| n.kind == K::UnionType || n.has_token(K::NoneKw)) =>
                    {
                        return Written::default();
                    }
                    _ => Shape::Items,
                }
            }
        };
        let at = Span::new(self.span(node).file, brace.end, brace.end);
        Written(Some((at, shape)))
    }

    /// A row: `none`, `r`, `r1 + r2`, `A, B`, `A, B | r`, in braces or not; a
    /// set `{...A, B, -C; E}` of services and errors holds the errors in `Fail`.
    pub fn row(&mut self, node: &Node, kind: &Kind, free: Free) -> Type {
        let kind = self.row_kind(kind, node);
        let errors = node.nodes().find(|n| n.kind == K::Row);
        let first_side = node
            .nodes()
            .any(|n| is_type(n.kind) || matches!(n.kind, K::RowSpread | K::RowOmit));
        let set_syntax = errors.is_some()
            || node
                .nodes()
                .any(|n| matches!(n.kind, K::RowSpread | K::RowOmit));
        match &kind {
            Kind::ServiceRow => {
                let (services, errors) = self.slots(Some(node), errors, free);
                self.with_errors(services, errors)
            }
            Kind::ErrorRow if errors.is_some() && first_side => {
                let text = self.text_of(node);
                self.error(
                    codes::T002,
                    self.span(node),
                    format!("`{text}` holds services, but a row of errors is expected here"),
                );
                Type::Error
            }
            Kind::ErrorRow => self.slots(None, Some(errors.unwrap_or(node)), free).1,
            _ if set_syntax => {
                let text = self.text_of(node);
                self.error(
                    codes::T002,
                    self.span(node),
                    format!("`{text}`: `...`, `-` and `;` are for rows of services and errors"),
                );
                Type::Error
            }
            _ => self.slot(node, &kind, free).row,
        }
    }

    /// The kind of a row: the expected one, or what its elements tell: errors
    /// when they name no service, and services otherwise.
    fn row_kind(&mut self, expected: &Kind, node: &Node) -> Kind {
        let expected = self.kinds.resolve(expected);
        if expected.is_row() {
            return expected;
        }
        if !matches!(expected, Kind::Meta(_)) {
            let text = self.text_of(node);
            self.error(
                codes::T002,
                self.span(node),
                format!(
                    "`{text}` is a row, but {} is expected here",
                    with_article(&expected)
                ),
            );
            return Kind::ServiceRow;
        }
        let mut first = node
            .nodes()
            .filter(|n| is_type(n.kind) || n.kind == K::RowSpread);
        let kind = if node.nodes().any(|n| n.kind == K::Row) {
            if first.next().is_none() {
                Kind::ErrorRow
            } else {
                Kind::ServiceRow
            }
        } else {
            let sides: Vec<Kind> = first.filter_map(|item| self.row_side(item)).collect();
            if !sides.is_empty() && sides.iter().all(|k| *k == Kind::ErrorRow) {
                Kind::ErrorRow
            } else {
                Kind::ServiceRow
            }
        };
        let _ = self.kinds.unify(&expected, &kind);
        kind
    }

    pub(super) fn element_label(&mut self, ty: &Type, kind: &Kind, node: &Node) -> Option<String> {
        let (head, _) = ty.spine();
        match head {
            Type::Con { module, name } => {
                // a set may name an error of a module not imported here: it was checked there
                let not_error = self
                    .env
                    .type_def(module, name)
                    .is_some_and(|d| !matches!(d.form, Form::Error));
                if *kind == Kind::ErrorRow && not_error {
                    self.error(
                        codes::T002,
                        self.span(node),
                        format!(
                            "`{name}` is not an error: an error row holds types declared with `type error`"
                        ),
                    );
                    return None;
                }
                Some(format!("{module}.{name}"))
            }
            _ if *kind == Kind::CaptureRow => ty::capture_label(ty),
            _ if *kind == Kind::ErrorRow => ty::error_label(ty),
            Type::Var(name) => Some(name.clone()),
            Type::Skolem(s) => Some(self.store.skolem_name(*s).to_owned()),
            _ => None,
        }
    }

    /// `^c`, `^{s, t}`, `^(c + d)`.
    pub(super) fn capture(&mut self, node: &Node, free: Free) -> Type {
        match node.kind {
            K::RowType => {
                let mut row = self.row(node, &Kind::CaptureRow, free);
                place_braces(&mut row, node, self.file);
                row
            }
            _ => self.ty(node, &Kind::CaptureRow, free),
        }
    }

    pub(super) fn record_type(&mut self, node: &Node, free: Free) -> Row {
        let mut entries: Vec<(String, Type)> = Vec::new();
        let mut tail = None;
        for child in node.nodes() {
            match child.kind {
                K::RecordTypeField => {
                    let Some(label) = child.child_tokens().find(|t| t.kind == K::Ident) else {
                        continue;
                    };
                    let label_text = label.text(self.text).to_owned();
                    let ty = child
                        .nodes()
                        .next()
                        .map_or(Type::Error, |t| self.ty(t, &Kind::Type, free));
                    if entries.iter().any(|(l, _)| *l == label_text) {
                        self.duplicate_label(&label_text, self.token_span(label));
                    } else {
                        entries.push((label_text, ty));
                    }
                }
                K::RowTail => {
                    let kind = Kind::Row(Box::new(Kind::Type));
                    tail = child
                        .nodes()
                        .next()
                        .and_then(|t| self.row_tail(t))
                        .map(|t| Box::new(self.ty(t, &kind, free)));
                }
                _ => {}
            }
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        Row {
            entries,
            tail,
            ..Row::default()
        }
    }

    /// The tail after `|`, which names a row: an applied type there is
    /// `FLICK-T002`.
    pub(super) fn row_tail<'n>(&mut self, tail: &'n Node) -> Option<&'n Node> {
        if tail.kind == K::AppType {
            self.error(
                codes::T002,
                self.span(tail),
                "the tail of a row is a row variable: `{A, B | r}`",
            );
            return None;
        }
        Some(tail)
    }
}

/// The empty row `none`, written at `span`: a fix that adds to it replaces it.
pub(super) fn written_none(span: Span) -> Type {
    Type::Row(Row {
        written: Written(Some((span, Shape::None))),
        ..Row::default()
    })
}

/// `ty`, a row written in the braces of `node`, with where it is written; a row
/// with `-` has no such place, as what a fix adds could be what `-` takes out.
pub(super) fn place_braces(ty: &mut Type, node: &Node, file: FileId) {
    if let Type::Row(row) = ty
        && let Some(brace) = node.child_tokens().find(|t| t.kind == K::LBrace)
        && !node.nodes().any(|n| n.kind == K::RowOmit)
    {
        let at = Span::new(file, brace.end, brace.end);
        let first_side = node.nodes().any(|n| n.kind != K::Row);
        let shape = if node.nodes().next().is_none() {
            Shape::EmptyRow
        } else if first_side {
            Shape::Items
        } else {
            Shape::NoServices
        };
        row.written = Written(Some((at, shape)));
    }
}
