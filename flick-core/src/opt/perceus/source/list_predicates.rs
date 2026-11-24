//! Pure array observers: an explicit shallow array ABI normalizer feeds the
//! closed ordinary-code loops. No input/callback crosses into host code.
//! Materialized tails retain the collection templates' quadratic baseline.
use super::kept_for_call;
use flick_ir::{Adapter, Copies, Foreign, ForeignMode, ResultAdapter, Shape};

use super::{Arith, BinOp, Builder, Expr, ExprKind, Lit, Num, Span};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Observer {
    Length,
    Any,
}

fn adapter(observer: Observer, copied_input: bool) -> Adapter {
    let (callbacks, args, callback_copies) = if observer == Observer::Length {
        (vec![None], vec![copied_input], vec![None])
    } else {
        (
            vec![Some(1), None],
            vec![false, true],
            vec![Some((vec![true], false)), None],
        )
    };
    Adapter {
        optional: vec![],
        owned: vec![],
        result: ResultAdapter {
            nullable: false,
            unit: false,
        },
        shapes: vec![Shape::Data; args.len()],
        result_shape: Shape::Data,
        callbacks,
        copies: Copies {
            args,
            result: false,
            callbacks: callback_copies,
        },
        words: vec![],
        retention: None,
    }
}

pub(super) fn expression(value: &Foreign, span: Span) -> Option<Expr> {
    let observer = match value.binding.export_name().unwrap_or("") {
        "listLength" => Observer::Length,
        "listAny" => Observer::Any,
        _ => return None,
    };
    let arity = if observer == Observer::Length { 1 } else { 2 };
    if value.binding.from() != Some("flick:rt")
        || value.mode != ForeignMode::Pure
        || value.arity != arity
        || value.dicts != 0
        || value.stages != vec![false; arity]
        || value.catch.is_some()
        || !(kept_for_call(&value.adapter) == adapter(observer, true)
            || (observer == Observer::Length && value.adapter == adapter(observer, false)))
    {
        return None;
    }
    let builder = Builder(span);
    let body = match observer {
        Observer::Length => Builder(span).length(),
        Observer::Any => Builder(span).any(),
    };
    Some(builder.lambda(&["normalize"], body))
}

impl Builder {
    fn length(&self) -> Expr {
        let next = self.binary(
            BinOp::Arith(Arith::Add, Num::Int),
            self.local("count"),
            self.lit(Lit::Int(1)),
        );
        let body = self.at(ExprKind::Match(
            Box::new(self.local("items")),
            vec![
                Self::arm(Self::empty(), self.local("count")),
                Self::arm(
                    Self::cons("unused", "tail"),
                    self.apply("loop", vec![next, self.local("tail")]),
                ),
            ],
        ));
        self.block(
            vec![self.function("loop", &["count", "items"], body)],
            self.lambda(
                &["input"],
                self.apply(
                    "loop",
                    vec![
                        self.lit(Lit::Int(0)),
                        self.apply("normalize", vec![self.local("input")]),
                    ],
                ),
            ),
        )
    }

    fn any(&self) -> Expr {
        let body = self.at(ExprKind::Match(
            Box::new(self.local("items")),
            vec![
                Self::arm(Self::empty(), self.lit(Lit::Bool(false))),
                Self::arm(
                    Self::cons("head", "tail"),
                    self.branch(
                        self.apply("callback", vec![self.local("head")]),
                        self.lit(Lit::Bool(true)),
                        self.apply("loop", vec![self.local("tail")]),
                    ),
                ),
            ],
        ));
        self.lambda(
            &["callback"],
            self.block(
                vec![self.function("loop", &["items"], body)],
                self.lambda(
                    &["input"],
                    self.apply(
                        "loop",
                        vec![self.apply("normalize", vec![self.local("input")])],
                    ),
                ),
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::{
        build,
        ir::{ItemKind, memory},
        opt::perceus::{LowerError, lower},
    };

    #[test]
    fn audited_length_and_any_adapters_lower_without_host_or_callback_erasure() {
        let mut seen = BTreeSet::new();
        let mut rejected = 0;
        for module in build::builtin_modules() {
            for item in &module.items {
                let ItemKind::Foreign(original) = &item.kind else {
                    continue;
                };
                if original.binding.from() != Some("flick:rt")
                    || !matches!(
                        original.binding.export_name().unwrap_or(""),
                        "listLength" | "listAny"
                    )
                {
                    continue;
                }
                let entry = memory::Name::new(&module.name, &original.name);
                if original.mode != ForeignMode::Pure {
                    assert_eq!((&*module.name, &*original.name), ("Js.Array", "length"));
                    assert_eq!(original.mode, ForeignMode::Sync);
                    assert!(expression(original, item.span).is_none());
                    assert!(
                        matches!(lower([module], &entry), Err(LowerError::Unsupported { span, .. }) if span == item.span)
                    );
                    continue;
                }
                seen.insert((module.name.as_str(), original.name.as_str()));
                let template = expression(original, item.span)
                    .unwrap_or_else(|| panic!("canonical list observer: {original:#?}"));
                assert!(super::super::super::free::free(&template).is_empty());
                super::super::tests::pure(&template);
                assert_eq!(
                    super::super::tests::callback_mentions(&template),
                    usize::from(original.binding.export_name() == Some("listAny")),
                );
                let program = lower([module], &entry).expect("ordinary pure observer template");
                assert_eq!(memory::validate_program(&program), Ok(()));
                let mut other_copy = original.clone();
                other_copy.adapter.copies.args[0] = !other_copy.adapter.copies.args[0];
                for mutate in super::super::tests::mutations(original) {
                    let mut changed = module.clone();
                    let value = changed
                        .items
                        .iter_mut()
                        .find_map(|candidate| match &mut candidate.kind {
                            ItemKind::Foreign(value) if value.name == original.name => Some(value),
                            _ => None,
                        })
                        .expect("matching declaration");
                    mutate(value);
                    if original.binding.export_name() == Some("listLength") && *value == other_copy
                    {
                        // List and nominal NonEmpty share this read-only code:
                        // neither publishes the input or mutates its elements.
                        assert!(expression(value, item.span).is_some());
                        assert!(lower([&changed], &entry).is_ok());
                    } else {
                        rejected += 1;
                        assert!(expression(value, item.span).is_none(), "{value:#?}");
                        assert!(
                            matches!(lower([&changed], &entry), Err(LowerError::Unsupported { span, .. }) if span == item.span)
                        );
                    }
                }
            }
        }
        assert_eq!(
            seen,
            BTreeSet::from([
                ("List.Core", "any"),
                ("List.Core", "length"),
                ("NonEmpty", "length")
            ])
        );
        eprintln!("3 exact observer contracts, {rejected} rejected metadata mutations");
    }
}
