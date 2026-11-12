//! Sets of services and errors in a row: `{...A, B, -C; ...E, -F | r}`. A set
//! is a `type alias` of a row. It is expanded here, so what comes after sees a
//! plain row, and nothing about sets reaches inference, the IR or the model.
//!
//! `services = plus₁ − minus₁`, `errors = (errors of plus₁ ∪ plus₂) − minus₂`,
//! with the index the side of `;`. A row with services holds its errors as one
//! `Fail {…}`, which an arrow and `Eff` move to their row of errors.

use super::{Free, is_type};
use crate::diagnostics::codes;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::Checker;
use crate::types::Elem;
use crate::types::env::Form;
use crate::types::kinds::Kind;
use crate::types::lang::{self, Lang};
use crate::types::ty::{Norm, Row, Type, Written};

/// One side of `;`, before anything is taken out of it.
pub(super) struct Slot<'n> {
    pub(super) row: Type,
    /// The errors of `Fail e` and of the sets on this side.
    fails: Vec<Type>,
    omitted: Vec<&'n Node>,
    /// The labels of the elements this side names itself.
    named: Vec<Elem>,
}

impl Slot<'_> {
    fn empty() -> Self {
        Slot {
            row: Type::empty_row(),
            fails: Vec::new(),
            omitted: Vec::new(),
            named: Vec::new(),
        }
    }
}

/// What a set holds.
#[derive(Default)]
pub(super) struct Set {
    pub(super) services: Vec<(Elem, Type)>,
    pub(super) fails: Vec<Type>,
}

impl Checker<'_> {
    /// The services and the errors of `{services; errors}`.
    pub(super) fn slots(
        &mut self,
        services: Option<&Node>,
        errors: Option<&Node>,
        free: Free,
    ) -> (Type, Type) {
        let first = services.map_or_else(Slot::empty, |n| self.slot(n, &Kind::ServiceRow, free));
        let second = errors.map_or_else(Slot::empty, |n| self.slot(n, &Kind::ErrorRow, free));
        let (gone_services, mut gone_errors) = self.taken(&first.omitted, &Kind::ServiceRow, free);
        gone_errors.extend(self.taken(&second.omitted, &Kind::ErrorRow, free).1);
        let services = self.without(first.row, gone_services, &first.named);
        let fails = first.fails.into_iter().chain(second.fails);
        let errors = self.joined(second.row, fails);
        (services, self.without(errors, gone_errors, &second.named))
    }

    /// `services` with `errors` in one `Fail {…}`, as a row of one kind holds them.
    pub(super) fn with_errors(&self, services: Type, errors: Type) -> Type {
        let norm = self.store.normalize(&errors);
        if norm.entries.is_empty() && norm.tails.is_empty() {
            return services;
        }
        let label = Elem::named(lang::label(Lang::Fail).unwrap_or("Fail"));
        let fail = (label, Type::app(Type::lang(Lang::Fail), errors));
        match services {
            Type::Row(mut row) => {
                row.entries.push(fail);
                Row::sort_entries(&mut row.entries);
                Type::Row(row)
            }
            other => Type::Row(Row {
                entries: vec![fail],
                tail: Some(Box::new(other)),
                ..Row::default()
            }),
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn slot<'n>(&mut self, node: &'n Node, kind: &Kind, free: Free) -> Slot<'n> {
        let items: Vec<&Node> = node.nodes().filter(|n| is_type(n.kind)).collect();
        // where each set is merged, and the name of the set
        let mut spreads: Vec<(&Node, &Node)> = (node.nodes())
            .filter(|n| n.kind == K::RowSpread)
            .filter_map(|n| Some((n, n.nodes().next()?)))
            .collect();
        let omitted: Vec<&Node> = node.nodes().filter(|n| n.kind == K::RowOmit).collect();
        let tail = node.nodes().find(|n| n.kind == K::RowTail);
        let mut fails = Vec::new();
        // `{t | e}` and `{NotFound, t}`: beside other elements, a variable of
        // an error row is an element, the error type it will stand for
        let element_variable =
            *kind == Kind::ErrorRow && (items.len() > 1 || tail.is_some() || !spreads.is_empty());
        let single_row = |item: &Node| {
            item.kind == K::UnionType
                || (item.kind == K::NameType && item.has_token(K::NoneKw))
                || (item.kind == K::NameType
                    && *kind != Kind::CaptureRow
                    && !element_variable
                    && item
                        .child_tokens()
                        .next()
                        .is_some_and(|t| t.kind == K::Ident))
        };
        if let [item] = items[..]
            && tail.is_none()
            && spreads.is_empty()
            && single_row(item)
        {
            let row = take_fails(self.ty(item, kind, free), &mut fails);
            return Slot {
                row,
                fails,
                omitted,
                named: Vec::new(),
            };
        }
        let element = match kind {
            Kind::ServiceRow => Kind::Service,
            Kind::CaptureRow => Kind::Region,
            _ => Kind::Type,
        };
        let mut entries: Vec<(Elem, Type)> = Vec::new();
        let mut named = Vec::new();
        // with a set the row needs no `;`: the first fix that adds `...` takes it away
        let mut needless_semi = self.needless_semi(node);
        for item in items {
            if single_row(item) {
                self.error(
                    codes::T002,
                    self.span(item),
                    "a row variable goes after `|` at the end of the row: `{A, B | r}`",
                );
                continue;
            }
            if matches!(kind, Kind::ServiceRow | Kind::ErrorRow) && self.set_kind(item).is_some() {
                self.set_needs_dots(item, needless_semi.take());
                spreads.push((item, item));
                continue;
            }
            let reported = self.diagnostics.len();
            let ty = self.ty(item, &element, free);
            if self.diagnostics.len() > reported {
                continue;
            }
            // an alias of a service or an error stands for it: the row holds what it names
            let ty = match kind {
                Kind::ServiceRow | Kind::ErrorRow => match self.whnf(&ty) {
                    Type::Error => ty,
                    expanded => expanded,
                },
                _ => ty,
            };
            if *kind == Kind::ServiceRow
                && let Some(failed) = failed(&ty)
            {
                fails.push(failed);
                continue;
            }
            let Some(label) = self.element_label(&ty, kind, item) else {
                continue;
            };
            named.push(label.clone());
            self.add_entry(&mut entries, label, ty, item);
        }
        for (spread, item) in spreads {
            let Some(set) = self.set_of(spread, item, free) else {
                continue;
            };
            if *kind == Kind::ErrorRow && !set.services.is_empty() {
                self.services_after_semi(item);
                continue;
            }
            for (label, ty) in set.services {
                self.add_entry(&mut entries, label, ty, spread);
            }
            fails.extend(set.fails);
        }
        let tail = tail
            .and_then(|t| t.nodes().next())
            .and_then(|t| self.row_tail(t))
            .map(|t| Box::new(self.ty(t, kind, free)));
        Row::sort_entries(&mut entries);
        let row = Type::Row(Row {
            entries,
            tail,
            ..Row::default()
        });
        Slot {
            row: take_fails(row, &mut fails),
            fails,
            omitted,
            named,
        }
    }

    /// Where `;` and an empty row of errors follow `node`, the services of an
    /// arrow, up to the `}`.
    fn needless_semi(&self, node: &Node) -> Option<Span> {
        if node.kind != K::Row {
            return None;
        }
        let span = self.span(node);
        let after = &self.text[span.range().end..];
        let rest = after.trim_start().strip_prefix(';')?;
        let brace = after.len() - rest.trim_start().strip_prefix('}')?.len() - 1;
        let end = span.end + u32::try_from(brace).ok()?;
        Some(Span::new(span.file, span.end, end))
    }

    /// Adds an element of a row, which tells its elements apart by name, not by arguments.
    fn add_entry(&mut self, entries: &mut Vec<(Elem, Type)>, label: Elem, ty: Type, node: &Node) {
        match entries.iter().find(|(l, _)| *l == label) {
            Some((_, held)) if *held != ty => {
                let (held, ty) = (self.show(held), self.show(&ty));
                self.error(
                    codes::T003,
                    self.span(node),
                    format!(
                        "the row already holds `{held}`, so it cannot hold `{ty}` too: a row has one element of each name"
                    ),
                );
            }
            Some(_) => {}
            None => entries.push((label, ty)),
        }
    }

    /// What the set `item` of `...item`, written at `node`, holds; `None` after a diagnostic.
    pub(super) fn set_of(&mut self, node: &Node, item: &Node, free: Free) -> Option<Set> {
        let Some(kind) = self.set_kind(item) else {
            self.not_a_set(node, item, free);
            return None;
        };
        let body = self.ty(item, &kind, free);
        // still its name: the alias is in a cycle, which name resolution reports
        if matches!(body, Type::Error | Type::Con { .. } | Type::App { .. }) {
            return None;
        }
        let Norm { entries, tails } = self.store.normalize(&body);
        if !tails.is_empty() {
            let text = self.text_of(item);
            self.error(
                codes::T002,
                self.span(item),
                format!(
                    "`{text}` is an open row, so it has no elements to merge: give the set no tail, and write `| r` where it is used"
                ),
            );
            return None;
        }
        if kind == Kind::ErrorRow {
            return Some(Set {
                services: Vec::new(),
                fails: vec![body],
            });
        }
        let mut set = Set::default();
        for (_, ty) in entries {
            if let Some(failed) = failed(&ty) {
                set.fails.push(failed);
            } else if let Some(label) = self.element_label(&ty, &Kind::ServiceRow, item) {
                set.services.push((label, ty));
            }
        }
        Some(set)
    }

    /// `row` with `fails`, flat; `row` itself when there are none.
    fn joined(&self, row: Type, fails: impl IntoIterator<Item = Type>) -> Type {
        let mut fails = fails.into_iter().peekable();
        if fails.peek().is_none() {
            return row;
        }
        let union = fails.fold(row, |row, f| Type::Union(Box::new(f), Box::new(row)));
        let Norm { entries, tails } = self.store.normalize(&union);
        rebuilt(entries, tails)
    }

    /// What a converted type becomes: a row alias its row, and `Eff` with the
    /// errors of its services among its errors.
    pub(super) fn rows_settled(&mut self, ty: Type, kind: &Kind, node: &Node) -> Type {
        if !matches!(ty, Type::Con { .. } | Type::App { .. }) {
            return ty;
        }
        if self.kinds.resolve(kind).is_row() {
            return self.expanded_row(ty);
        }
        let (head, args) = ty.spine();
        if !args.iter().any(|arg| holds_fail(arg)) {
            return ty;
        }
        if let ([services, errors, result], true) = (&args[..], head.is_lang(Lang::Eff)) {
            let mut fails = Vec::new();
            let services = take_fails((*services).clone(), &mut fails);
            let errors = self.joined((*errors).clone(), fails);
            return Type::apps(head.clone(), [services, errors, (*result).clone()]);
        }
        let text = self.text_of(node);
        self.error(
            codes::T002,
            self.span(node),
            format!(
                "`{text}` gets a row with errors, and only an arrow or `Eff` has a place for them: name the services in an alias of their own"
            ),
        );
        ty
    }

    /// The row a transparent alias stands for. It no longer is where its
    /// alias is written, so a fix does not edit the alias through it.
    fn expanded_row(&self, ty: Type) -> Type {
        let Type::Con { module, name } = ty.spine().0 else {
            return ty;
        };
        if !matches!(
            self.env.type_def(module, name).map(|d| &d.form),
            Some(Form::Alias { opaque: false, .. })
        ) {
            return ty;
        }
        match self.whnf(&ty) {
            Type::Error => ty,
            Type::Row(row) => Type::Row(Row {
                written: Written::default(),
                ..row
            }),
            expanded => expanded,
        }
    }
}

pub(super) fn rebuilt(entries: Vec<(Elem, Type)>, tails: Vec<Type>) -> Type {
    let tail = tails
        .into_iter()
        .reduce(|a, b| Type::Union(Box::new(a), Box::new(b)));
    Type::Row(Row {
        entries,
        tail: tail.map(Box::new),
        ..Row::default()
    })
}

/// The errors of `ty` when it is `Fail e`.
fn failed(ty: &Type) -> Option<Type> {
    let (head, args) = ty.spine();
    match args[..] {
        [errors] if head.is_lang(Lang::Fail) => Some(errors.clone()),
        _ => None,
    }
}

/// `row` without its `Fail e`, whose errors go to `fails`.
fn take_fails(row: Type, fails: &mut Vec<Type>) -> Type {
    match row {
        Type::Row(mut row) => {
            row.entries.retain(|(_, ty)| match failed(ty) {
                Some(errors) => {
                    fails.push(errors);
                    false
                }
                None => true,
            });
            row.tail = row.tail.map(|tail| Box::new(take_fails(*tail, fails)));
            Type::Row(row)
        }
        Type::Union(a, b) => {
            let a = take_fails(*a, fails);
            Type::Union(Box::new(a), Box::new(take_fails(*b, fails)))
        }
        other => other,
    }
}

fn holds_fail(row: &Type) -> bool {
    match row {
        Type::Row(row) => {
            row.entries.iter().any(|(_, ty)| failed(ty).is_some())
                || row.tail.as_deref().is_some_and(holds_fail)
        }
        Type::Union(a, b) => holds_fail(a) || holds_fail(b),
        _ => false,
    }
}
