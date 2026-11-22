//! Scalar, string, list, and equality operations for the bounded evaluator.

use std::rc::Rc;

use flick_ir::{Arith, BinOp, Expr, Lit, Num};

use super::Machine;
use crate::consteval::{self, Const};
use crate::numeric;
use crate::opt::eval::StopReason;
use crate::opt::eval::pattern::{value_eq, value_same};
use crate::opt::eval::value::{
    FIELD_BYTES, SLOT_BYTES, Utf16Source, Value, bigint_value, cmp_utf16, rc_str,
};
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
                Ok(Value::String(rc_str(s)))
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
        lhs: &Value<'a>,
        rhs: &Value<'a>,
    ) -> Result<Value<'a>, StopReason> {
        match op {
            BinOp::Eq | BinOp::NotEq => {
                let eq = value_eq(lhs, rhs, self.budget)?;
                Ok(Value::Bool(eq ^ matches!(op, BinOp::NotEq)))
            }
            BinOp::Same | BinOp::NotSame => {
                let same = value_same(lhs, rhs, self.budget)?;
                Ok(Value::Bool(same ^ matches!(op, BinOp::NotSame)))
            }
            BinOp::Arith(arith, Num::Int) => match (lhs, rhs) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(numeric::int_arith(arith, *a, *b))),
                _ => Err(StopReason::UnsafeContext),
            },
            BinOp::Concat => self.eval_string_concat(lhs, rhs),
            BinOp::ListConcat => self.eval_list_concat(lhs, rhs),
            BinOp::Lt
            | BinOp::Le
            | BinOp::Gt
            | BinOp::Ge
            | BinOp::Arith(_, Num::Float | Num::BigInt) => {
                if matches!(op, BinOp::Arith(Arith::Pow, Num::BigInt)) {
                    match (lhs, rhs) {
                        (Value::BigInt(_), Value::BigInt(b)) if *b < 0 => {
                            // JS throws RangeError and Go panics on negative BigInt exponents.
                            return Err(StopReason::PossibleTrap);
                        }
                        (Value::BigInt(_), Value::BigInt(_)) => {}
                        _ => return Err(StopReason::UnsafeContext),
                    }
                }
                if matches!(op, BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge)
                    && let (Some((sa, a)), Some((sb, b))) = (as_utf16(lhs), as_utf16(rhs))
                {
                    let scan_steps = u32::try_from(sa.min(sb)).unwrap_or(u32::MAX);
                    self.budget.charge_steps(scan_steps)?;
                    let ord = cmp_utf16(a, b);
                    let matched = match op {
                        BinOp::Lt => ord.is_lt(),
                        BinOp::Le => ord.is_le(),
                        BinOp::Gt => ord.is_gt(),
                        BinOp::Ge => ord.is_ge(),
                        _ => return Err(StopReason::UnknownOp),
                    };
                    return Ok(Value::Bool(matched));
                }
                let token = fold::token(op).ok_or(StopReason::UnknownOp)?;
                let lc = value_to_const(lhs)?;
                let rc = value_to_const(rhs)?;
                let out = consteval::binary(token, lc, rc).ok_or(StopReason::UnknownOp)?;
                self.const_to_value(out)
            }
            BinOp::And | BinOp::Or => Err(StopReason::UnsafeContext),
        }
    }

    fn eval_string_concat(
        &mut self,
        lhs: &Value<'a>,
        rhs: &Value<'a>,
    ) -> Result<Value<'a>, StopReason> {
        match (lhs, rhs) {
            (Value::String(a), Value::String(b)) => {
                let total = a.len().saturating_add(b.len());
                let bytes = u32::try_from(total).unwrap_or(u32::MAX);
                self.budget.charge_bytes(bytes)?;
                self.budget.charge_steps(bytes / 16)?;
                let mut out = String::with_capacity(total);
                out.push_str(a);
                out.push_str(b);
                Ok(Value::String(rc_str(&out)))
            }
            (Value::Utf16(a), Value::Utf16(b)) => {
                let total_units = a.len().saturating_add(b.len());
                let bytes = u32::try_from(total_units)
                    .unwrap_or(u32::MAX)
                    .saturating_mul(2);
                self.budget.charge_bytes(bytes)?;
                self.budget.charge_steps(bytes / 16)?;
                let mut out = Vec::new();
                out.extend_from_slice(a);
                out.extend_from_slice(b);
                Ok(Value::Utf16(Rc::new(out)))
            }
            (Value::String(a), Value::Utf16(b)) => self.concat_str_utf16(a, b, true),
            (Value::Utf16(a), Value::String(b)) => self.concat_str_utf16(b, a, false),
            _ => Err(StopReason::UnsafeContext),
        }
    }

    fn concat_str_utf16(
        &mut self,
        s: &str,
        units: &[u16],
        str_first: bool,
    ) -> Result<Value<'a>, StopReason> {
        let scan = u32::try_from(s.len() / 16).unwrap_or(u32::MAX);
        self.budget.charge_steps(scan)?;
        let mut s_units = 0_usize;
        for ch in s.chars() {
            s_units += ch.len_utf16();
        }
        let total_units = s_units.saturating_add(units.len());
        let bytes = u32::try_from(total_units)
            .unwrap_or(u32::MAX)
            .saturating_mul(2);
        self.budget.charge_bytes(bytes)?;
        self.budget.charge_steps(bytes / 16)?;
        let mut out = Vec::new();
        if !str_first {
            out.extend_from_slice(units);
        }
        let mut buf = [0_u16; 2];
        for ch in s.chars() {
            out.extend_from_slice(ch.encode_utf16(&mut buf));
        }
        if str_first {
            out.extend_from_slice(units);
        }
        Ok(Value::Utf16(Rc::new(out)))
    }

    fn eval_list_concat(
        &mut self,
        lhs: &Value<'a>,
        rhs: &Value<'a>,
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
        let mut out = Vec::new();
        for x in a.iter() {
            out.push(x.clone());
        }
        for x in b.iter() {
            out.push(x.clone());
        }
        Ok(Value::List(Rc::new(out)))
    }

    fn const_to_value(&mut self, c: Const) -> Result<Value<'a>, StopReason> {
        match c {
            Const::Number(x) => Ok(Value::Float(x)),
            Const::BigInt(n) => {
                self.budget.charge_bytes(16)?;
                Ok(Value::BigInt(n))
            }
            Const::String(s) => {
                let len = u32::try_from(s.len()).unwrap_or(u32::MAX);
                self.budget.charge_bytes(len)?;
                Ok(Value::String(rc_str(&s)))
            }
            Const::Bool(b) => Ok(Value::Bool(b)),
        }
    }
}

fn as_utf16<'a>(v: &'a Value<'a>) -> Option<(usize, Utf16Source<'a>)> {
    match v {
        Value::String(s) => Some((s.len() / 16, Utf16Source::from_str(s))),
        Value::Utf16(u) => Some((u.len() / 8, Utf16Source::from_units(u))),
        _ => None,
    }
}

pub(super) fn value_to_const(value: &Value<'_>) -> Result<Const, StopReason> {
    match value {
        Value::Int(n) => Ok(Const::Number(f64::from(*n))),
        Value::Float(x) => Ok(Const::Number(*x)),
        Value::BigInt(n) => Ok(Const::BigInt(*n)),
        Value::Bool(b) => Ok(Const::Bool(*b)),
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
