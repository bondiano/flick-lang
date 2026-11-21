//! Widening closed (or rigid-tailed) service and error rows in covariant
//! positions when a value fixes an open type variable under subsumption.

use std::mem;
use std::rc::Rc;

use super::{Check, Checker};
use crate::types::{Fields, Row, Stage, Type, Var};
use crate::variance::Sign;

impl Checker<'_> {
    /// `ty` with closed (or rigid-tailed) service and error rows in covariant
    /// positions widened by fresh open row metas (`LeLive` allows growing rows
    /// covariantly, never in contravariant parameters or invariant `Ref`/data).
    pub(super) fn widen_for_meta(&mut self, ty: &Type) -> Type {
        let head = self.heads(ty);
        if self.alias(&head).is_some() {
            return head;
        }
        match head {
            Type::Captured(inner, c) => Type::Captured(Rc::new(self.widen_for_meta(&inner)), c),
            Type::Bounded(inner, c) => Type::Bounded(Rc::new(self.widen_for_meta(&inner)), c),
            Type::Fun(p, Stage::Runtime(r, e), res) => Type::Fun(
                p,
                Stage::Runtime(self.widen_row(&r), self.widen_row(&e)),
                Rc::new(self.widen_for_meta(&res)),
            ),
            Type::Fun(p, Stage::Pure, res) => {
                Type::Fun(p, Stage::Pure, Rc::new(self.widen_for_meta(&res)))
            }
            Type::Eff(r, e, res) => Type::Eff(
                self.widen_row(&r),
                self.widen_row(&e),
                Rc::new(self.widen_for_meta(&res)),
            ),
            Type::Layer(i, e, o) => Type::Layer(self.widen_row(&i), self.widen_row(&e), o),
            Type::Tuple(xs) => Type::Tuple(xs.iter().map(|x| self.widen_for_meta(x)).collect()),
            Type::Record(f) if f.tail.is_none() => Type::Record(Fields {
                fields: f
                    .fields
                    .iter()
                    .map(|(l, t)| (l.clone(), self.widen_for_meta(t)))
                    .collect(),
                tail: None,
            }),
            a @ Type::App(..) => {
                let Some((p, xs)) = self.data_args(&a) else {
                    return a;
                };
                let Some(signs) = self
                    .decls
                    .signs
                    .get(&p)
                    .filter(|s| s.len() == xs.len())
                    .cloned()
                else {
                    return a;
                };
                let args: Vec<Type> = xs
                    .into_iter()
                    .zip(signs)
                    .map(|(x, s)| match s {
                        Sign::Pos => self.widen_for_meta(&x),
                        Sign::Neg | Sign::Inv | Sign::Bi => x,
                    })
                    .collect();
                args.into_iter().fold(Type::Con(p), Type::app)
            }
            Type::Dict(c, xs) => {
                let Some(signs) = self
                    .decls
                    .class_signs
                    .get(&c)
                    .filter(|s| s.len() == xs.len())
                    .cloned()
                else {
                    return Type::Dict(c, xs);
                };
                let args: Rc<[Type]> = xs
                    .iter()
                    .zip(signs)
                    .map(|(x, s)| match s {
                        Sign::Pos => self.widen_for_meta(x),
                        Sign::Neg | Sign::Inv | Sign::Bi => x.clone(),
                    })
                    .collect();
                Type::Dict(c, args)
            }
            other => other,
        }
    }

    /// Adds a fresh row meta to a row that has no row meta yet (preserving
    /// already-open rows such as `-Log | ?s`).
    fn widen_row(&mut self, r: &Row) -> Row {
        let mut r = self.zonk_row(r);
        if r.vars.iter().any(|v| matches!(v, Var::Meta(_))) {
            return r;
        }
        let tail = Var::Meta(self.meta());
        r.vars.insert(tail.clone());
        self.widened_rows.push(Row::var(tail));
        r
    }

    /// Closes any remaining unsolved metas added by subsumption widening to
    /// `Row::none()`. Called only during final settlement (`finish`), never
    /// mid-checking during GADT row-upper inference (`infer_row_upper`).
    pub(super) fn close_widened_rows(&mut self) -> Check<()> {
        let widened = mem::take(&mut self.widened_rows);
        for row in widened {
            for v in &self.zonk_row(&row).vars {
                if let Var::Meta(m) = v {
                    self.bind_row(*m, Row::none())?;
                }
            }
        }
        Ok(())
    }
}
