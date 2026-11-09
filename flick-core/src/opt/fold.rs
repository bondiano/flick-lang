//! Literal expressions evaluated when the program compiles, with the JS
//! semantics of [`consteval`](crate::consteval) bit for bit, and branches of
//! constant conditions chosen. A result that is not certain to match JS, or
//! that JS would throw, stays a run-time computation.

use crate::ir::ExprLang;
use std::fmt::Write as _;

use crate::consteval::{self, Const};
use crate::ir::{BinOp, Expr, ExprKind, Lit};
use crate::numeric::{self, Arith, Num};
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::literal;
use crate::types::Dict;
use crate::types::lang::{self, Lang};

/// Folds `e` itself, its operands already folded; whether it changed.
pub fn fold(e: &mut Expr) -> bool {
    let folded = match &e.kind {
        ExprKind::Binary(BinOp::Arith(op, Num::Int), l, r) => int(l)
            .zip(int(r))
            .map(|(l, r)| Const::Number(numeric::int_arith(*op, l, r).into())),
        ExprKind::Binary(op, l, r) => token(*op)
            .zip(constant(l).zip(constant(r)))
            .and_then(|(op, (l, r))| consteval::binary(op, l, r)),
        ExprKind::Neg(Num::Int, inner) => {
            int(inner).map(|x| Const::Number(x.wrapping_neg().into()))
        }
        ExprKind::Not(inner) => constant(inner).and_then(|c| consteval::prefix(K::NotKw, c)),
        ExprKind::App { func, arg, .. } => match &func.kind {
            ExprKind::Method {
                dict: Dict::Instance { module, name, args },
                name: method,
            } if *module == lang::item(Lang::Instances).module
                && args.is_empty()
                && *method == lang::item(Lang::ShowMethod).name =>
            {
                constant(arg).and_then(|c| show(name, c)).map(Const::String)
            }
            ExprKind::Method {
                dict: Dict::Instance { module, name, args },
                name: method,
            } if *module == lang::item(Lang::Instances).module
                && args.is_empty()
                && *method == lang::item(Lang::DisplayMethod).name =>
            {
                constant(arg)
                    .and_then(|c| display(name, c))
                    .map(Const::String)
            }
            _ => None,
        },
        ExprKind::If(c, t, f) => match constant(c) {
            Some(Const::Bool(true)) => {
                *e = (**t).clone();
                return true;
            }
            Some(Const::Bool(false)) => {
                *e = (**f).clone();
                return true;
            }
            _ => None,
        },
        _ => None,
    };
    let num = match &e.kind {
        ExprKind::Binary(BinOp::Arith(_, num), ..) | ExprKind::Neg(num, _) => *num,
        _ => Num::Float,
    };
    match folded.map(|c| expr(&c, num, e)) {
        Some(value) => {
            *e = value;
            true
        }
        None => false,
    }
}

/// `show` of a built-in instance as the runtime `show` spells it.
fn show(instance: &str, c: Const) -> Option<String> {
    Some(match (instance, c) {
        ("Show Float", Const::Number(x)) => js_number(x),
        ("Show Int", Const::Number(x)) => format!("{x}"),
        ("Show BigInt", Const::BigInt(x)) => format!("{x}n"),
        ("Show Bool", Const::Bool(b)) => b.to_string(),
        ("Show String", Const::String(s)) => json_string(&s),
        _ => return None,
    })
}

/// `display` of a built-in instance as its runtime method spells it.
fn display(instance: &str, c: Const) -> Option<String> {
    match (instance, c) {
        ("Display String", Const::String(s)) => Some(s),
        ("Display Float", c) => show("Show Float", c),
        ("Display Int", c) => show("Show Int", c),
        ("Display Bool", c) => show("Show Bool", c),
        _ => None,
    }
}

/// JS `String(x)`, but `-0` for negative zero like the runtime `show`.
/// Rust's `{:e}` gives the same shortest digits; only the layout differs.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "at most 17 digits, and each cast follows its range check"
)]
pub fn js_number(x: f64) -> String {
    if x.is_nan() {
        return "NaN".to_owned();
    }
    let sign = if x.is_sign_negative() { "-" } else { "" };
    if x.is_infinite() {
        return format!("{sign}Infinity");
    }
    if x == 0.0 {
        return format!("{sign}0");
    }
    let sci = format!("{:e}", x.abs());
    let (mantissa, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let digits = even_tie(x.abs(), mantissa.replace('.', ""), exp);
    let k = digits.len() as i32;
    // the decimal point sits after `n` digits
    let n = exp.parse::<i32>().unwrap_or(0) + 1;
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        let (int, frac) = digits.split_at(n as usize);
        format!("{int}.{frac}")
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let (first, rest) = digits.split_at(1);
        let dot = if rest.is_empty() { "" } else { "." };
        let e = n - 1;
        format!(
            "{first}{dot}{rest}e{}{}",
            if e < 0 { "-" } else { "+" },
            e.abs()
        )
    };
    format!("{sign}{body}")
}

/// Rust breaks an exact tie between two shortest spellings upwards, JS
/// towards the even one: `…562.25` is `…562.3` in Rust, `…562.2` in JS.
fn even_tie(x: f64, digits: String, exp: &str) -> String {
    let Some(last) = digits.bytes().last().filter(|d| d % 2 == 1) else {
        return digits;
    };
    let mut lower = digits[..digits.len() - 1].to_owned();
    lower.push(char::from(last - 1));
    // 800 places hold every digit of any f64 exactly
    let exact = format!("{x:.800e}");
    let (exact, exact_exp) = exact.split_once('e').unwrap_or((&exact, ""));
    let exact = exact.replace('.', "");
    let tie = exact_exp == exp && exact.trim_end_matches('0') == format!("{lower}5");
    let reads_back = format!("{}.{}e{exp}", &lower[..1], &lower[1..]).parse() == Ok(x);
    if tie && reads_back { lower } else { digits }
}

/// `JSON.stringify` of a string.
pub fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn token(op: BinOp) -> Option<K> {
    Some(match op {
        BinOp::Or => K::OrOr,
        BinOp::And => K::AndAnd,
        BinOp::Eq | BinOp::Same => K::EqEq,
        BinOp::NotEq | BinOp::NotSame => K::NotEq,
        BinOp::Lt => K::Lt,
        BinOp::Le => K::Le,
        BinOp::Gt => K::Gt,
        BinOp::Ge => K::Ge,
        BinOp::Concat => K::PlusPlus,
        BinOp::Arith(Arith::Add, _) => K::Plus,
        BinOp::Arith(Arith::Sub, _) => K::Minus,
        BinOp::Arith(Arith::Mul, _) => K::Star,
        BinOp::Arith(Arith::Div, _) => K::Slash,
        BinOp::Arith(Arith::Rem, _) => K::Percent,
        BinOp::Arith(Arith::Pow, _) => K::StarStar,
        BinOp::ListConcat => return None,
    })
}

/// The value of an `Int` constant.
fn int(e: &Expr) -> Option<i32> {
    match constant(e)? {
        #[allow(
            clippy::cast_possible_truncation,
            reason = "the value is checked to be an i32"
        )]
        #[allow(clippy::float_cmp, reason = "an i32 is exact in f64")]
        Const::Number(x) if f64::from(x as i32) == x => Some(x as i32),
        _ => None,
    }
}

/// The value of a literal, a negated number literal, or `Float.infinity`
/// and `Float.nan`.
pub fn constant(e: &Expr) -> Option<Const> {
    match &e.kind {
        ExprKind::Lit(Lit::Int(n)) => Some(Const::Number((*n).into())),
        ExprKind::Lit(Lit::Float(x)) => Some(Const::Number(*x)),
        ExprKind::Lit(Lit::BigInt(n)) => literal::bigint_digits_value(n).map(Const::BigInt),
        ExprKind::Lit(Lit::String(s)) => Some(Const::String(s.clone())),
        ExprKind::Lit(Lit::Bool(b)) => Some(Const::Bool(*b)),
        ExprKind::Neg(_, inner) => consteval::prefix(K::Minus, constant(inner)?),
        kind if kind.is_lang(Lang::FloatInfinity) => Some(Const::Number(f64::INFINITY)),
        kind if kind.is_lang(Lang::FloatNan) => Some(Const::Number(f64::NAN)),
        _ => None,
    }
}

/// `c` as a literal at the place of `at`; a number is of type `num`.
fn expr(c: &Const, num: Num, at: &Expr) -> Expr {
    let lit = match c {
        #[allow(
            clippy::cast_possible_truncation,
            reason = "an `Int` result is folded in 32 bits"
        )]
        Const::Number(x) if num == Num::Int => Lit::Int(*x as i32),
        Const::Number(x) => Lit::Float(*x),
        Const::BigInt(x) => Lit::BigInt(x.to_string()),
        Const::String(s) => Lit::String(s.clone()),
        Const::Bool(b) => Lit::Bool(*b),
    };
    Expr::new(ExprKind::Lit(lit), at.span)
}
