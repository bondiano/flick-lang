//! Literals and operators, each only where a Lua number, boolean or
//! string computes exactly what Flick's does.
//!
//! - `and`, `or` and `not` on booleans, and `==` of values whose identity is
//!   their equality, are Lua's own.
//! - Integers are doubles holding integers, which compare exactly; integer
//!   arithmetic wraps, divides and rounds as Lua's does not, so `Int`'s is
//!   the kernel's (`K.add32`, …), and that of the other widths is refused.
//! - `+`, `-`, `*` and `/` of `Float`, and its negation and comparisons, are
//!   IEEE 754 operations on doubles in both. `==` of `Float` holds of NaN and
//!   NaN in Flick, so it is the kernel's, as is `==` of structured values;
//!   `%` truncates and `**` has its own special cases, so those are refused,
//!   as is all `Float32` arithmetic, which rounds to 32 bits.
//! - Strings are WTF-8, equal when their bytes are; `++` joins the halves of
//!   a surrogate pair and the order goes by UTF-16 code units, so both are
//!   the kernel's (`K.concat`, `K.compare_strings`), as is `++` of lists.

use flick_ir::{Arith, BinOp, EnumLit, Expr as IrExpr, ExprKind as Ir, Lang, Lit, Num, Span};

use super::super::ast::{BinOp as LuaOp, Block, Expr, ExprKind, Name, Stmt, StmtKind, UnOp};
use super::super::names::KERNEL;
use super::Lowering;

/// Why an operator the kernel owns, which it does not compute yet, is refused.
const UNCOMPUTED: &str = "the Lua kernel does not compute it exactly yet";

/// What a scalar operand is, as far as the operators here care.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scalar {
    /// An integer type of at most 32 bits: a double holding it exactly.
    Int,
    /// `Float`, or `Float32`: a double holding a 32-bit float exactly.
    Float,
    Bool,
    String,
    /// `Int64`, `UInt64` or `BigInt`: a `BigInt` value of the kernel.
    Wide,
    Unit,
}

/// The shortest spelling of a finite, non-negative `x` that reads back as
/// exactly `x`: Rust writes the shortest digits that round-trip.
fn float_text(x: f64) -> String {
    let plain = format!("{x}");
    let exponent = format!("{x:e}");
    if exponent.len() < plain.len() {
        exponent
    } else {
        plain
    }
}

/// The suffix of the kernel's functions of a numeric type but `Int` and
/// `Float`: `add_i8`, `neg_u64`, …
fn width(num: Num) -> Option<&'static str> {
    Some(match num {
        Num::Int8 => "i8",
        Num::Int16 => "i16",
        Num::UInt8 => "u8",
        Num::UInt16 => "u16",
        Num::UInt32 => "u32",
        Num::Float32 => "f32",
        Num::Int64 => "i64",
        Num::UInt64 => "u64",
        Num::Int | Num::Float | Num::BigInt => return None,
    })
}

fn arith_name(arith: Arith) -> &'static str {
    match arith {
        Arith::Add => "add",
        Arith::Sub => "sub",
        Arith::Mul => "mul",
        Arith::Div => "div",
        Arith::Rem => "rem",
        Arith::Pow => "pow",
    }
}

/// The function of the Lua kernel that computes `op` exactly, when one does.
fn kernel_op(op: BinOp) -> Option<String> {
    Some(match op {
        BinOp::Arith(arith, Num::Int) => format!("{}32", arith_name(arith)),
        BinOp::Arith(Arith::Rem, Num::Float) => "fmod".to_owned(),
        BinOp::Arith(Arith::Pow, Num::Float) => "fpow".to_owned(),
        BinOp::Arith(arith, Num::BigInt) => format!("big_{}", arith_name(arith)),
        BinOp::Arith(arith, num) => format!("{}_{}", arith_name(arith), width(num)?),
        BinOp::Concat => "concat".to_owned(),
        BinOp::ListConcat => "list_concat".to_owned(),
        _ => return None,
    })
}

/// The magnitude of a whole number of decimal digits, in limbs of 24 bits
/// from the least, as the kernel's `BigInt` keeps it.
fn limbs(digits: &str) -> Vec<u64> {
    let mut out: Vec<u64> = Vec::new();
    for d in digits.bytes().filter(u8::is_ascii_digit) {
        let mut carry = u64::from(d - b'0');
        for limb in &mut out {
            let t = *limb * 10 + carry;
            *limb = t % (1 << 24);
            carry = t >> 24;
        }
        while carry > 0 {
            out.push(carry % (1 << 24));
            carry >>= 24;
        }
    }
    out
}

/// The number Lua 5.1 folds `e` into at compile time, if it does: its
/// arithmetic of number constants, but for a division or modulo by zero
/// and a NaN result (`constfolding` of `lcode.c`).
fn folded(expr: &Expr) -> Option<f64> {
    let value = match &expr.kind {
        ExprKind::Number(text) => text.parse().ok()?,
        ExprKind::Unary(UnOp::Neg, inner) => -folded(inner)?,
        ExprKind::Binary(op, lhs, rhs) => {
            let (left, right) = (folded(lhs)?, folded(rhs)?);
            match op {
                LuaOp::Add => left + right,
                LuaOp::Sub => left - right,
                LuaOp::Mul => left * right,
                LuaOp::Div if right != 0.0 => left / right,
                LuaOp::Mod if right != 0.0 => left - (left / right).floor() * right,
                LuaOp::Pow => left.powf(right),
                _ => return None,
            }
        }
        _ => return None,
    };
    (!value.is_nan()).then_some(value)
}

/// `K.name(args…)`.
pub(super) fn kernel_call(name: &str, args: Vec<Expr>, span: Span) -> Expr {
    Expr::name(&Name::new(KERNEL.to_owned()), span)
        .field(name)
        .call(args)
}

impl Lowering<'_> {
    fn number(text: String, negative: bool, span: Span) -> Expr {
        let n = Expr::new(ExprKind::Number(text), span);
        if negative {
            Expr::unary(UnOp::Neg, n, span)
        } else {
            n
        }
    }

    fn integer(n: i64, span: Span) -> Expr {
        Self::number(n.unsigned_abs().to_string(), n < 0, span)
    }

    pub(super) fn literal(&mut self, lit: &Lit, span: Span) -> Expr {
        match lit {
            Lit::Int(n) => Self::integer(i64::from(*n), span),
            Lit::Int8(n) => Self::integer(i64::from(*n), span),
            Lit::Int16(n) => Self::integer(i64::from(*n), span),
            Lit::UInt8(n) => Self::integer(i64::from(*n), span),
            Lit::UInt16(n) => Self::integer(i64::from(*n), span),
            Lit::UInt32(n) => Self::integer(i64::from(*n), span),
            Lit::Float(x) => self.float(*x, span),
            Lit::Float32(x) => self.float(f64::from(*x), span),
            Lit::Int64(n) => Self::wide(&n.to_string(), span),
            Lit::UInt64(n) => Self::wide(&n.to_string(), span),
            Lit::BigInt(text) => Self::wide(text, span),
            Lit::String(s) => Expr::str(s, span),
            Lit::Bool(b) => Expr::new(ExprKind::Bool(*b), span),
            Lit::Unit => Self::unit(span),
        }
    }

    /// A double: NaN and the infinities as the divisions Lua 5.1 does not
    /// fold at compile time, `-0.0` only where a literal keeps its sign.
    fn float(&mut self, x: f64, span: Span) -> Expr {
        let one = |n: &str| Expr::new(ExprKind::Number(n.to_owned()), span);
        if x.is_nan() {
            return Expr::binary(LuaOp::Div, one("0"), one("0"), span);
        }
        if x.is_infinite() {
            let top = Self::number("1".to_owned(), x < 0.0, span);
            return Expr::binary(LuaOp::Div, top, one("0"), span);
        }
        let literal = Self::number(float_text(x.abs()), x.is_sign_negative(), span);
        self.unfolded_zero(literal)
    }

    /// `K.big(sign, limbs…)`, the `BigInt` value of decimal digits.
    fn wide(text: &str, span: Span) -> Expr {
        let digits = limbs(text);
        let sign = if digits.is_empty() {
            "0"
        } else if text.starts_with('-') {
            "-1"
        } else {
            "1"
        };
        let magnitude = Expr::new(
            ExprKind::Number(sign.trim_start_matches('-').to_owned()),
            span,
        );
        let mut args = vec![if sign.starts_with('-') {
            Expr::unary(UnOp::Neg, magnitude, span)
        } else {
            magnitude
        }];
        args.extend(
            digits
                .iter()
                .map(|&l| Expr::new(ExprKind::Number(l.to_string()), span)),
        );
        kernel_call("big", args, span)
    }

    /// The literal an enum's constructor is.
    pub(super) fn enum_literal(literal: &EnumLit, span: Span) -> Expr {
        match literal {
            EnumLit::Int(n) => Self::integer(i64::from(*n), span),
            EnumLit::String(s) => Expr::str(s, span),
        }
    }

    /// The scalar type of `e`, from its literal or its type.
    fn scalar(&self, e: &IrExpr) -> Option<Scalar> {
        if let Ir::Lit(lit) = &e.kind {
            return Some(match lit {
                Lit::Int(_)
                | Lit::Int8(_)
                | Lit::Int16(_)
                | Lit::UInt8(_)
                | Lit::UInt16(_)
                | Lit::UInt32(_) => Scalar::Int,
                Lit::Float(_) | Lit::Float32(_) => Scalar::Float,
                Lit::Bool(_) => Scalar::Bool,
                Lit::String(_) => Scalar::String,
                Lit::Int64(_) | Lit::UInt64(_) | Lit::BigInt(_) => Scalar::Wide,
                Lit::Unit => Scalar::Unit,
            });
        }
        let (module, name) = e.ty.as_deref()?.as_bare_con()?;
        let is = |lang| self.known.is(lang, module, name);
        if [
            Lang::Int,
            Lang::Int8,
            Lang::Int16,
            Lang::UInt8,
            Lang::UInt16,
            Lang::UInt32,
        ]
        .into_iter()
        .any(is)
        {
            Some(Scalar::Int)
        } else if is(Lang::Float) || is(Lang::Float32) {
            Some(Scalar::Float)
        } else if is(Lang::Int64) || is(Lang::UInt64) || is(Lang::BigInt) {
            Some(Scalar::Wide)
        } else if is(Lang::Bool) {
            Some(Scalar::Bool)
        } else if is(Lang::String) {
            Some(Scalar::String)
        } else {
            None
        }
    }

    pub(super) fn binary(&mut self, op: BinOp, lhs: &IrExpr, rhs: &IrExpr, span: Span) -> Expr {
        if matches!(op, BinOp::And | BinOp::Or) {
            return self.logical(op, lhs, rhs, span);
        }
        let scalar = self.scalar(lhs).or_else(|| self.scalar(rhs));
        if let Some(name) = kernel_op(op) {
            let args = self.ordered(&[lhs, rhs]);
            return kernel_call(&name, args, span);
        }
        let equality = matches!(op, BinOp::Eq | BinOp::NotEq | BinOp::Same | BinOp::NotSame);
        // a `BigInt` value is a table: by its value, not its identity
        if scalar == Some(Scalar::Wide) && !equality {
            let compare = match op {
                BinOp::Lt => Some(LuaOp::Lt),
                BinOp::Le => Some(LuaOp::Le),
                BinOp::Gt => Some(LuaOp::Gt),
                BinOp::Ge => Some(LuaOp::Ge),
                _ => None,
            };
            if let Some(compare) = compare {
                let args = self.ordered(&[lhs, rhs]);
                let order = kernel_call("big_compare", args, span);
                let zero = Expr::new(ExprKind::Number("0".to_owned()), span);
                return Expr::binary(compare, order, zero, span);
            }
        }
        let ordering = match op {
            BinOp::Lt => Some(LuaOp::Lt),
            BinOp::Le => Some(LuaOp::Le),
            BinOp::Gt => Some(LuaOp::Gt),
            BinOp::Ge => Some(LuaOp::Ge),
            _ => None,
        };
        // structural, and SameValueZero on floats: the kernel's
        let structural = matches!(op, BinOp::Eq | BinOp::NotEq)
            && matches!(scalar, None | Some(Scalar::Float | Scalar::Unit));
        if structural || (equality && scalar == Some(Scalar::Wide)) {
            let args = self.ordered(&[lhs, rhs]);
            let equal = kernel_call("equals", args, span);
            return if matches!(op, BinOp::Eq | BinOp::Same) {
                equal
            } else {
                Expr::unary(UnOp::Not, equal, span)
            };
        }
        if let (Some(ordering), Some(Scalar::String)) = (ordering, scalar) {
            let args = self.ordered(&[lhs, rhs]);
            let order = kernel_call("compare_strings", args, span);
            let zero = Expr::new(ExprKind::Number("0".to_owned()), span);
            return Expr::binary(ordering, order, zero, span);
        }
        let lua = match (op, scalar) {
            (BinOp::Same, _) | (BinOp::Eq, Some(Scalar::Int | Scalar::Bool | Scalar::String)) => {
                Some(LuaOp::Eq)
            }
            (BinOp::NotSame, _)
            | (BinOp::NotEq, Some(Scalar::Int | Scalar::Bool | Scalar::String)) => Some(LuaOp::Ne),
            (BinOp::Lt, Some(Scalar::Int | Scalar::Float)) => Some(LuaOp::Lt),
            (BinOp::Le, Some(Scalar::Int | Scalar::Float)) => Some(LuaOp::Le),
            (BinOp::Gt, Some(Scalar::Int | Scalar::Float)) => Some(LuaOp::Gt),
            (BinOp::Ge, Some(Scalar::Int | Scalar::Float)) => Some(LuaOp::Ge),
            (BinOp::Arith(arith, Num::Float), _) => match arith {
                Arith::Add => Some(LuaOp::Add),
                Arith::Sub => Some(LuaOp::Sub),
                Arith::Mul => Some(LuaOp::Mul),
                Arith::Div => Some(LuaOp::Div),
                Arith::Rem | Arith::Pow => None,
            },
            _ => None,
        };
        let Some(lua) = lua else {
            let what = match (op, scalar) {
                (BinOp::Eq | BinOp::NotEq, None | Some(Scalar::Unit)) => {
                    format!("`{}` of a structured or unknown type", op.symbol())
                }
                (_, Some(Scalar::Int)) => format!("`{}` of integers", op.symbol()),
                (_, Some(Scalar::Float)) => format!("`{}` of floats", op.symbol()),
                (_, Some(Scalar::String)) => format!("`{}` of strings", op.symbol()),
                (BinOp::Arith(_, num), _) => format!("`{}` of `{num:?}`", op.symbol()),
                _ => format!("the operator `{}`", op.symbol()),
            };
            return self.unsupported(span, &what, UNCOMPUTED);
        };
        let mut values = self.ordered(&[lhs, rhs]).into_iter();
        let l = values.next().expect("two operands");
        let r = values.next().expect("two operands");
        self.unfolded_zero(Expr::binary(lua, l, r, span))
    }

    /// `e`, or where Lua 5.1 would fold it into the constant `-0`, the
    /// kernel's `-0`: its compiler keeps one constant for `0` and `-0`, so
    /// that one would stand for the other in the same function.
    fn unfolded_zero(&self, e: Expr) -> Expr {
        let negative_zero = |x: f64| x == 0.0 && x.is_sign_negative();
        if self.dialect.negative_zero || !folded(&e).is_some_and(negative_zero) {
            return e;
        }
        Expr::name(&Name::new(KERNEL.to_owned()), e.span).field("negzero")
    }

    /// `&&` and `||`: the right operand runs only when the left one does not
    /// decide, after the statements it needs.
    fn logical(&mut self, op: BinOp, lhs: &IrExpr, rhs: &IrExpr, span: Span) -> Expr {
        let lua = if op == BinOp::And {
            LuaOp::And
        } else {
            LuaOp::Or
        };
        let l = self.expr(lhs);
        let (stmts, r) = self.within_value(|g| g.expr(rhs));
        if stmts.is_empty() {
            return Expr::binary(lua, l, r, span);
        }
        let result: Name = self.local("b", l);
        let mut then: Block = stmts;
        then.push(Stmt::new(
            StmtKind::Assign(Expr::name(&result, span), r),
            span,
        ));
        let test = Expr::name(&result, span);
        let test = if op == BinOp::And {
            test
        } else {
            Expr::unary(UnOp::Not, test, span)
        };
        self.push(StmtKind::If(vec![(test, then)], None), span);
        Expr::name(&result, span)
    }

    /// `-inner`: of a float, Lua's; of an `Int`, the kernel's, which wraps.
    pub(super) fn neg(&mut self, num: Num, inner: &IrExpr, span: Span) -> Expr {
        if matches!(num, Num::Float | Num::Float32) {
            let inner = self.expr(inner);
            return self.unfolded_zero(Expr::unary(UnOp::Neg, inner, span));
        }
        let name = match num {
            Num::Int => Some("neg32".to_owned()),
            Num::BigInt => Some("big_neg".to_owned()),
            _ => width(num).map(|w| format!("neg_{w}")),
        };
        if let Some(name) = name {
            let inner = self.expr(inner);
            return kernel_call(&name, vec![inner], span);
        }
        self.unsupported(span, &format!("negation of `{num:?}`"), UNCOMPUTED)
    }
}
