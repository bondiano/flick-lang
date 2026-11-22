//! Scalar, string, list, and equality operations for the bounded evaluator.

use std::cmp::Ordering;
use std::rc::Rc;

use flick_ir::{Arith, BinOp, Expr, Lit, Num};

use super::Machine;
use crate::consteval::{self, Const, Op as ConstOp};
use crate::numeric;
use crate::opt::eval::StopReason;
use crate::opt::eval::pattern::{value_eq, value_same};
use crate::opt::eval::value::{FIELD_BYTES, SLOT_BYTES, Value, bigint_value};
use crate::opt::fold;

impl<'a> Machine<'a, '_> {
    pub(super) fn eval_lit(&mut self, lit: &Lit) -> Result<Value<'a>, StopReason> {
        match lit {
            Lit::Unit => Ok(Value::Unit),
            Lit::Bool(b) => Ok(Value::Bool(*b)),
            Lit::Int(n) => Ok(Value::Int(*n)),
            Lit::Float(x) => Ok(Value::Float(*x)),
            Lit::BigInt(digits) => {
                self.budget.charge_bytes(16)?;
                let value = bigint_value(digits).ok_or(StopReason::UnknownOp)?;
                Ok(Value::BigInt(value))
            }
            Lit::String(s) => {
                let len = u32::try_from(s.len()).unwrap_or(u32::MAX);
                self.budget.charge_bytes(len)?;
                Ok(Value::String(Rc::from(s.as_str())))
            }
        }
    }

    pub(super) fn eval_neg(&mut self, num: Num, value: Value<'a>) -> Result<Value<'a>, StopReason> {
        match (num, value) {
            (Num::Int, Value::Int(n)) => Ok(Value::Int(n.wrapping_neg())),
            (Num::Float, Value::Float(x)) => Ok(Value::Float(-x)),
            (Num::BigInt, Value::BigInt(n)) => {
                self.budget.charge_bytes(16)?;
                let negated = n.checked_neg().ok_or(StopReason::UnknownOp)?;
                Ok(Value::BigInt(negated))
            }
            _ => Err(StopReason::UnsafeContext),
        }
    }

    pub(super) fn eval_binary(
        &mut self,
        op: BinOp,
        lhs: Value<'a>,
        rhs: Value<'a>,
    ) -> Result<Value<'a>, StopReason> {
        match op {
            BinOp::Eq => Ok(Value::Bool(value_eq(&lhs, &rhs, self.budget)?)),
            BinOp::NotEq => Ok(Value::Bool(!value_eq(&lhs, &rhs, self.budget)?)),
            BinOp::Same => Ok(Value::Bool(value_same(&lhs, &rhs, self.budget)?)),
            BinOp::NotSame => Ok(Value::Bool(!value_same(&lhs, &rhs, self.budget)?)),
            BinOp::Arith(arith, Num::Int) => match (lhs, rhs) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(numeric::int_arith(arith, a, b))),
                _ => Err(StopReason::UnsafeContext),
            },
            BinOp::Arith(Arith::Pow, Num::BigInt) => match (lhs, rhs) {
                (Value::BigInt(a), Value::BigInt(b)) => {
                    if b < 0 {
                        // JS throws RangeError and Go panics on negative BigInt exponents.
                        return Err(StopReason::PossibleTrap);
                    }
                    let exp = u32::try_from(b).map_err(|_| StopReason::UnknownOp)?;
                    self.budget.charge_bytes(16)?;
                    let res = a.checked_pow(exp).ok_or(StopReason::UnknownOp)?;
                    Ok(Value::BigInt(res))
                }
                _ => Err(StopReason::UnsafeContext),
            },
            BinOp::Concat => self.eval_string_concat(lhs, rhs),
            BinOp::ListConcat => self.eval_list_concat(lhs, rhs),
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => self.eval_order(op, &lhs, &rhs),
            BinOp::Arith(_, Num::Float | Num::BigInt) => {
                let token = fold::token(op).ok_or(StopReason::UnknownOp)?;
                let lc = value_to_const(&lhs)?;
                let rc = value_to_const(&rhs)?;
                let out = consteval::binary(token, lc, rc).ok_or(StopReason::UnknownOp)?;
                self.const_to_value(out, op)
            }
            BinOp::And | BinOp::Or => Err(StopReason::UnsafeContext),
        }
    }

    fn eval_order(
        &mut self,
        op: BinOp,
        lhs: &Value<'a>,
        rhs: &Value<'a>,
    ) -> Result<Value<'a>, StopReason> {
        let token = fold::token(op).ok_or(StopReason::UnknownOp)?;
        match (lhs, rhs) {
            (Value::String(a), Value::String(b)) => {
                let scan = u32::try_from(a.len().min(b.len()) / 16).unwrap_or(u32::MAX);
                self.budget.charge_steps(scan)?;
            }
            (Value::Utf16(a), Value::Utf16(b)) => {
                let scan = u32::try_from(a.len().min(b.len()) / 8).unwrap_or(u32::MAX);
                self.budget.charge_steps(scan)?;
                let ord = a.iter().cmp(b.iter());
                return Ok(Value::Bool(order_matches(token, ord)?));
            }
            (Value::String(a), Value::Utf16(b)) => {
                let scan = u32::try_from(b.len() / 8).unwrap_or(u32::MAX);
                self.budget.charge_steps(scan)?;
                let ord = a.encode_utf16().cmp(b.iter().copied());
                return Ok(Value::Bool(order_matches(token, ord)?));
            }
            (Value::Utf16(a), Value::String(b)) => {
                let scan = u32::try_from(a.len() / 8).unwrap_or(u32::MAX);
                self.budget.charge_steps(scan)?;
                let ord = a.iter().copied().cmp(b.encode_utf16());
                return Ok(Value::Bool(order_matches(token, ord)?));
            }
            _ => {}
        }
        let lc = value_to_const(lhs)?;
        let rc = value_to_const(rhs)?;
        let out = consteval::binary(token, lc, rc).ok_or(StopReason::UnknownOp)?;
        self.const_to_value(out, op)
    }

    fn eval_string_concat(
        &mut self,
        lhs: Value<'a>,
        rhs: Value<'a>,
    ) -> Result<Value<'a>, StopReason> {
        match (lhs, rhs) {
            (Value::String(a), Value::String(b)) => {
                let total = a.len().saturating_add(b.len());
                let bytes = u32::try_from(total).unwrap_or(u32::MAX);
                self.budget.charge_bytes(bytes)?;
                self.budget.charge_steps(bytes / 16)?;
                let mut out = String::with_capacity(total);
                out.push_str(&a);
                out.push_str(&b);
                Ok(Value::String(Rc::from(out)))
            }
            (Value::Utf16(a), Value::Utf16(b)) => {
                let total_units = a.len().saturating_add(b.len());
                let bytes = u32::try_from(total_units)
                    .unwrap_or(u32::MAX)
                    .saturating_mul(2);
                self.budget.charge_bytes(bytes)?;
                self.budget.charge_steps(bytes / 16)?;
                let mut out = Vec::with_capacity(total_units);
                out.extend_from_slice(&a);
                out.extend_from_slice(&b);
                Ok(Value::Utf16(Rc::from(out)))
            }
            (Value::String(a), Value::Utf16(b)) => {
                // UTF-16 unit count is always <= UTF-8 byte length; charge scan before counting.
                let scan = u32::try_from(a.len() / 16).unwrap_or(u32::MAX);
                self.budget.charge_steps(scan)?;
                let a_units = a.encode_utf16().count();
                let total_units = a_units.saturating_add(b.len());
                let bytes = u32::try_from(total_units)
                    .unwrap_or(u32::MAX)
                    .saturating_mul(2);
                self.budget.charge_bytes(bytes)?;
                self.budget.charge_steps(bytes / 16)?;
                let mut out = Vec::with_capacity(total_units);
                out.extend(a.encode_utf16());
                out.extend_from_slice(&b);
                Ok(Value::Utf16(Rc::from(out)))
            }
            (Value::Utf16(a), Value::String(b)) => {
                let scan = u32::try_from(b.len() / 16).unwrap_or(u32::MAX);
                self.budget.charge_steps(scan)?;
                let b_units = b.encode_utf16().count();
                let total_units = a.len().saturating_add(b_units);
                let bytes = u32::try_from(total_units)
                    .unwrap_or(u32::MAX)
                    .saturating_mul(2);
                self.budget.charge_bytes(bytes)?;
                self.budget.charge_steps(bytes / 16)?;
                let mut out = Vec::with_capacity(total_units);
                out.extend_from_slice(&a);
                out.extend(b.encode_utf16());
                Ok(Value::Utf16(Rc::from(out)))
            }
            _ => Err(StopReason::UnsafeContext),
        }
    }

    fn eval_list_concat(
        &mut self,
        lhs: Value<'a>,
        rhs: Value<'a>,
    ) -> Result<Value<'a>, StopReason> {
        let (Value::List(a), Value::List(b)) = (lhs, rhs) else {
            return Err(StopReason::UnsafeContext);
        };
        let total = a.len().saturating_add(b.len());
        let bytes = u32::try_from(total)
            .unwrap_or(u32::MAX)
            .saturating_mul(SLOT_BYTES);
        self.budget.charge_bytes(bytes)?;
        let steps = u32::try_from(total / 8).unwrap_or(u32::MAX);
        self.budget.charge_steps(steps)?;
        let mut out = Vec::with_capacity(total);
        out.extend(a.iter().cloned());
        out.extend(b.iter().cloned());
        Ok(Value::List(Rc::from(out)))
    }

    fn const_to_value(&mut self, c: Const, op: BinOp) -> Result<Value<'a>, StopReason> {
        match c {
            Const::Number(x) => match op {
                BinOp::Arith(_, Num::Int) =>
                {
                    #[allow(
                        clippy::cast_possible_truncation,
                        reason = "Int arithmetic already wrapped in 32 bits"
                    )]
                    Ok(Value::Int(x as i32))
                }
                _ => Ok(Value::Float(x)),
            },
            Const::BigInt(n) => {
                self.budget.charge_bytes(16)?;
                Ok(Value::BigInt(n))
            }
            Const::String(s) => {
                let len = u32::try_from(s.len()).unwrap_or(u32::MAX);
                self.budget.charge_bytes(len)?;
                Ok(Value::String(Rc::from(s)))
            }
            Const::Bool(b) => Ok(Value::Bool(b)),
        }
    }
}

pub(super) fn value_to_const(value: &Value<'_>) -> Result<Const, StopReason> {
    match value {
        Value::Int(n) => Ok(Const::Number(f64::from(*n))),
        Value::Float(x) => Ok(Const::Number(*x)),
        Value::BigInt(n) => Ok(Const::BigInt(*n)),
        Value::String(s) => Ok(Const::String(s.to_string())),
        Value::Bool(b) => Ok(Const::Bool(*b)),
        _ => Err(StopReason::UnknownOp),
    }
}

fn order_matches(op: ConstOp, ord: Ordering) -> Result<bool, StopReason> {
    match op {
        ConstOp::Lt => Ok(ord.is_lt()),
        ConstOp::Le => Ok(ord.is_le()),
        ConstOp::Gt => Ok(ord.is_gt()),
        ConstOp::Ge => Ok(ord.is_ge()),
        _ => Err(StopReason::UnknownOp),
    }
}

pub(super) fn fields_bytes(fields: &[(String, Expr)]) -> u32 {
    let mut total = u32::try_from(fields.len())
        .unwrap_or(u32::MAX)
        .saturating_mul(FIELD_BYTES);
    for (label, _) in fields {
        total = total.saturating_add(u32::try_from(label.len()).unwrap_or(u32::MAX));
    }
    total
}
