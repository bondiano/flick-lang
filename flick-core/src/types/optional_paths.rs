//! Steps of paths of fields, with optional ones: `user.?address.city` and
//! `.?address.city`. A step written `.?label` takes one `Option` off the
//! value before it and reads the field of its `Some`. A path with such a
//! step is the `Option` of its last field, whose own type stays whole:
//! `None` once a step finds `None`.

use super::Checker;
use super::ty::Type;
use crate::diagnostics::{Diagnostic, codes};
use crate::iface::{Entity, Namespace};
use crate::source::Span;
use crate::syntax::cst::{Node, Token};
use crate::syntax::fields::is_optional;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::{self, Lang};

impl Checker<'_> {
    /// What the steps of a path read from `value`: the last field, in an
    /// `Option` when a step is optional.
    pub(super) fn read_steps(&mut self, mut value: Type, steps: &[&Node]) -> Type {
        let mut optional = false;
        for step in steps {
            // the parser reported a step without its label
            let Some(label) = step.child_tokens().find(|t| t.kind == K::Ident) else {
                return Type::Error;
            };
            if is_optional(step) {
                optional = true;
                let Some(payload) = self.option_payload(&value, step, label) else {
                    return Type::Error;
                };
                value = payload;
            }
            if self.option_receiver(&value, step, label) {
                return Type::Error;
            }
            value = self.field_of(&value, label, self.span(step));
        }
        match steps.last() {
            Some(last) if optional && value != Type::Error => self.some_of(&value, self.span(last)),
            _ => value,
        }
    }

    /// `Some value`, typed as the constructor applied to it, as in the
    /// `match` the path lowers to: the payload and the `Option` share the
    /// constructor's capture bound.
    fn some_of(&mut self, value: &Type, at: Span) -> Type {
        let item = lang::item(Lang::Some);
        let entity = Entity {
            module: item.module.clone(),
            namespace: Namespace::Constructor,
            name: item.name.clone(),
        };
        let scheme = self.env.value(&entity).cloned();
        let Some(Type::Fun { param, result, .. }) = scheme.map(|s| self.instantiate(&s, at)) else {
            return Type::app(Type::lang(Lang::Option), value.clone());
        };
        self.expect(value, &param, at);
        *result
    }

    /// The payload of the `Option` the optional step `.?label` reads from,
    /// noted at its `?` for lowering; `None` after reporting a value of
    /// another type.
    fn option_payload(&mut self, value: &Type, step: &Node, label: Token) -> Option<Type> {
        let (bare, captures) = self.whnf_captured(value);
        let (head, args) = bare.spine();
        let head = self.whnf(head);
        let item = match args[..] {
            [item] if head.is_lang(Lang::Option) => item.clone(),
            _ if bare == Type::Error => return None,
            _ => {
                let item = self.fresh();
                let option = Type::app(Type::lang(Lang::Option), item.clone());
                if self.unify(&bare, &option).is_err() {
                    let shown = self.show(value);
                    let label = label.text(self.text);
                    self.error(
                        codes::T003,
                        self.segment(step),
                        format!(
                            "`.?{label}` reads a field of an `Option`, but this has type `{shown}`"
                        ),
                    );
                    return None;
                }
                item
            }
        };
        let item = match &captures {
            Some(c) => self.data_field(&item, c),
            None => item,
        };
        if let Some(mark) = step.child_tokens().find(|t| t.kind == K::Question) {
            self.note_value((mark.start, mark.end), &item);
        }
        Some(item)
    }

    /// Reports reading the field `label` of an `Option`, which has none: a
    /// plain step takes no `Option` off, and an optional one only one.
    fn option_receiver(&mut self, value: &Type, step: &Node, label: Token) -> bool {
        let (bare, _) = self.whnf_captured(value);
        let (head, args) = bare.spine();
        if args.len() != 1 || !self.whnf(head).is_lang(Lang::Option) {
            return false;
        }
        // the capture bound a constructed `Option` carries is no layer of it
        let shown = self.show(&bare);
        let label = label.text(self.text);
        let (message, help) = if is_optional(step) {
            (
                format!("`.?{label}` takes one `Option` off, and `{shown}` is left"),
                format!(
                    "a step takes off one layer: handle the inner `Option` before reading `{label}`"
                ),
            )
        } else {
            (
                format!("`{shown}` is an `Option`, which has no field `{label}`"),
                format!(
                    "read `.?{label}` for an `Option` of the field, or handle the `None` first"
                ),
            )
        };
        self.diagnostics.push(
            Diagnostic::error(codes::T003, message, self.segment(step))
                .with_fix(|| (help, Vec::new())),
        );
        true
    }

    /// The span of a step alone, from its `.` to its label: that of a
    /// `FieldExpr` also covers the path before it.
    fn segment(&self, step: &Node) -> Span {
        let whole = self.span(step);
        match step.child_tokens().find(|t| t.kind == K::Dot) {
            Some(dot) => Span::new(whole.file, dot.start, whole.end),
            None => whole,
        }
    }
}
