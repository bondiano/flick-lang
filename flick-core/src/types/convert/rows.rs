//! Rows in types: the service and error rows of a runtime arrow, row types in
//! braces, record fields and capture qualifiers.

use super::{Free, is_type, with_article};
use crate::diagnostics::codes;
use crate::iface::Namespace;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::env::{Form, Found};
use crate::types::kinds::Kind;
use crate::types::ty::{Effects, Row, Type};
use crate::types::{Checker, ty};

impl Checker<'_> {
    /// `{r; e}` of a runtime arrow; `{}` leaves both rows empty.
    pub(super) fn effects(&mut self, node: &Node, free: Free) -> Effects {
        let mut rows = node.nodes().filter(|n| n.kind == K::Row);
        let mut side = |this: &mut Self, kind: Kind| {
            rows.next()
                .map_or_else(Type::empty_row, |row| this.row(row, &kind, free))
        };
        let services = side(self, Kind::ServiceRow);
        let errors = side(self, Kind::ErrorRow);
        Effects::new(services, errors)
    }

    /// A row: `none`, `r`, `r1 + r2`, `A, B`, `A, B | r`, in braces or not.
    pub fn row(&mut self, node: &Node, kind: &Kind, free: Free) -> Type {
        let items: Vec<&Node> = node.nodes().filter(|n| is_type(n.kind)).collect();
        let tail = node.nodes().find(|n| n.kind == K::RowTail);
        let kind = self.row_kind(&items, kind, node);
        // `{t | e}` and `{NotFound, t}`: beside other elements, a variable of
        // an error row is an element, the error type it will stand for
        let element_variable = kind == Kind::ErrorRow && (items.len() > 1 || tail.is_some());
        let single_row = |item: &Node| {
            item.kind == K::UnionType
                || (item.kind == K::NameType && item.has_token(K::NoneKw))
                || (item.kind == K::NameType
                    && kind != Kind::CaptureRow
                    && !element_variable
                    && item
                        .child_tokens()
                        .next()
                        .is_some_and(|t| t.kind == K::Ident))
        };
        if let [item] = items[..]
            && tail.is_none()
            && single_row(item)
        {
            return self.ty(item, &kind, free);
        }
        let element = match &kind {
            Kind::ServiceRow => Kind::Service,
            Kind::CaptureRow => Kind::Region,
            _ => Kind::Type,
        };
        let mut entries: Vec<(String, Type)> = Vec::new();
        for item in items {
            if single_row(item) {
                self.error(
                    codes::T002,
                    self.span(item),
                    "a row variable goes after `|` at the end of the row: `{A, B | r}`",
                );
                continue;
            }
            let reported = self.diagnostics.len();
            let ty = self.ty(item, &element, free);
            if self.diagnostics.len() > reported {
                continue;
            }
            // an alias of a service or an error stands for it: the row holds what it names
            let ty = match &kind {
                Kind::ServiceRow | Kind::ErrorRow => match self.whnf(&ty) {
                    Type::Error => ty,
                    expanded => expanded,
                },
                _ => ty,
            };
            let Some(label) = self.element_label(&ty, &kind, item) else {
                continue;
            };
            // a row tells its elements apart by name, not by arguments
            match entries.iter().find(|(l, _)| *l == label) {
                Some((_, held)) if *held != ty => {
                    let (held, ty) = (self.show(held), self.show(&ty));
                    self.error(
                        codes::T003,
                        self.span(item),
                        format!(
                            "the row already holds `{held}`, so it cannot hold `{ty}` too: a row has one element of each name"
                        ),
                    );
                }
                Some(_) => {}
                None => entries.push((label, ty)),
            }
        }
        let tail = tail
            .and_then(|t| t.nodes().next())
            .and_then(|t| self.row_tail(t))
            .map(|t| Box::new(self.ty(t, &kind, free)));
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        Type::Row(Row { entries, tail })
    }

    /// The kind of a row: the expected one, or what its elements tell.
    fn row_kind(&mut self, items: &[&Node], expected: &Kind, node: &Node) -> Kind {
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
        let guess = items.iter().find_map(|item| {
            let tokens: Vec<_> = item.child_tokens().filter(|t| t.kind == K::Con).collect();
            let name = tokens.last()?.text(self.text);
            let Found::Entity(entity) = self.env.find(Namespace::Type, name) else {
                return None;
            };
            match self.env.type_def(&entity.module, name)?.form {
                Form::Service { .. } => Some(Kind::ServiceRow),
                Form::Error => Some(Kind::ErrorRow),
                _ => None,
            }
        });
        let kind = guess.unwrap_or(Kind::ServiceRow);
        let _ = self.kinds.unify(&expected, &kind);
        kind
    }

    fn element_label(&mut self, ty: &Type, kind: &Kind, node: &Node) -> Option<String> {
        let (head, _) = ty.spine();
        match head {
            Type::Con { module, name } => {
                let is_error = matches!(
                    self.env.type_def(module, name).map(|d| &d.form),
                    Some(Form::Error)
                );
                if *kind == Kind::ErrorRow && !is_error {
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
            K::RowType => self.row(node, &Kind::CaptureRow, free),
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
        Row { entries, tail }
    }

    /// The tail after `|`, which names a row: an applied type there is
    /// `FLICK-T002`.
    fn row_tail<'n>(&mut self, tail: &'n Node) -> Option<&'n Node> {
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
