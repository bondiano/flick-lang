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
    /// `Int64`, `UInt64`, `BigInt`, `Unit`.
    Other,
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

/// The function of the Lua kernel that computes `op` exactly, when one does.
fn kernel_op(op: BinOp) -> Option<&'static str> {
    Some(match op {
        BinOp::Arith(arith, Num::Int) => match arith {
            Arith::Add => "add32",
            Arith::Sub => "sub32",
            Arith::Mul => "mul32",
            Arith::Div => "div32",
            Arith::Rem => "rem32",
            Arith::Pow => "pow32",
        },
        BinOp::Concat => "concat",
        BinOp::ListConcat => "list_concat",
        _ => return None,
    })
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
            Lit::Int64(_) | Lit::UInt64(_) | Lit::BigInt(_) => {
                self.unsupported(span, "a 64-bit or big integer literal", UNCOMPUTED)
            }
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
        if x == 0.0 && x.is_sign_negative() && !self.dialect.negative_zero {
            let message = format!(
                "the literal `-0.0` is not available on the {} target with the dialect `{}` yet: \
                 {} folds it into an equal `0` constant, and {UNCOMPUTED}",
                self.target, self.dialect.tag, self.dialect.title
            );
            return self.refuse(span, message);
        }
        Self::number(float_text(x.abs()), x.is_sign_negative(), span)
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
                Lit::Int64(_) | Lit::UInt64(_) | Lit::BigInt(_) | Lit::Unit => Scalar::Other,
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
            return kernel_call(name, args, span);
        }
        let ordering = match op {
            BinOp::Lt => Some(LuaOp::Lt),
            BinOp::Le => Some(LuaOp::Le),
            BinOp::Gt => Some(LuaOp::Gt),
            BinOp::Ge => Some(LuaOp::Ge),
            _ => None,
        };
        // structural, and SameValueZero on floats: the kernel's
        if matches!(op, BinOp::Eq | BinOp::NotEq)
            && matches!(scalar, None | Some(Scalar::Float | Scalar::Other))
        {
            let args = self.ordered(&[lhs, rhs]);
            let equal = kernel_call("equals", args, span);
            return if op == BinOp::Eq {
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
                (BinOp::Eq | BinOp::NotEq, None | Some(Scalar::Other)) => {
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
        Expr::binary(lua, l, r, span)
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
            return Expr::unary(UnOp::Neg, inner, span);
        }
        if num == Num::Int {
            let inner = self.expr(inner);
            return kernel_call("neg32", vec![inner], span);
        }
        self.unsupported(span, &format!("negation of `{num:?}`"), UNCOMPUTED)
    }
}
