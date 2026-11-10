//! The refined plugin interprets syntax facts; the host supplies no verdict.
use flick_plugin::{Constant, ExpressionFact, ExpressionKind as E};

pub(super) fn holds(expression: &ExpressionFact, constant: &Constant) -> Option<bool> {
    match expression.nodes.get(expression.root as usize)? {
        E::Reference { module, name } if module == "Checks" => super::passes(name, constant),
        E::Lambda { parameter, body } => {
            if matches!(constant, Constant::Number(n) if !n.is_finite()) {
                return None;
            }
            let Value::Scalar(Constant::Boolean(b)) = eval(expression, *body, parameter, constant)?
            else {
                return None;
            };
            Some(b)
        }
        _ => None,
    }
}

enum Value {
    Scalar(Constant),
    Integer(i128),
}

fn eval(e: &ExpressionFact, id: u32, parameter: &str, value: &Constant) -> Option<Value> {
    use Value::{Integer, Scalar};
    let child = |index| {
        // Facts form an arena: no cycles or unbounded recursion from a host.
        if index >= id {
            return None;
        }
        eval(e, index, parameter, value)
    };
    match e.nodes.get(id as usize)? {
        E::Constant(c) => Some(Scalar(c.clone())),
        E::BigInt(n) => n.parse().ok().map(Integer),
        E::Variable(v) if v == parameter => Some(Scalar(value.clone())),
        E::Unary { op, value } => match (op.as_str(), child(*value)?) {
            ("-", Scalar(Constant::Number(n))) => Some(Scalar(Constant::Number(-n))),
            ("-", Integer(n)) => n.checked_neg().map(Integer),
            ("not", Scalar(Constant::Boolean(b))) => Some(Scalar(Constant::Boolean(!b))),
            _ => None,
        },
        E::Binary { op, left, right } => match (child(*left)?, child(*right)?) {
            (Scalar(a), Scalar(b)) => binary(op, a, b).map(Scalar),
            (Integer(a), Integer(b)) => integer_binary(op, a, b),
            _ => None,
        },
        E::If {
            condition,
            then,
            otherwise,
        } => {
            let Scalar(Constant::Boolean(b)) = child(*condition)? else {
                return None;
            };
            child(if b { *then } else { *otherwise })
        }
        _ => None,
    }
}

/// Keep the host's previous exact-integer folding range. Outside it a
/// predicate remains a runtime check, including overflowing powers.
fn integer_binary(op: &str, a: i128, b: i128) -> Option<Value> {
    use Value::{Integer, Scalar};
    let boolean = |b| Scalar(Constant::Boolean(b));
    Some(match op {
        "+" => Integer(a.checked_add(b)?),
        "-" => Integer(a.checked_sub(b)?),
        "*" => Integer(a.checked_mul(b)?),
        // Flick's integer operators define division and remainder by zero as zero.
        "/" | "%" if b == 0 => Integer(0),
        "/" => Integer(a.checked_div(b)?),
        "%" => Integer(a.checked_rem(b)?),
        "**" => Integer(a.checked_pow(u32::try_from(b).ok()?)?),
        "==" => boolean(a == b),
        "!=" => boolean(a != b),
        "<" => boolean(a < b),
        "<=" => boolean(a <= b),
        ">" => boolean(a > b),
        ">=" => boolean(a >= b),
        _ => return None,
    })
}

#[cfg(test)]
pub(super) fn reference(module: &str, name: &str) -> ExpressionFact {
    ExpressionFact {
        root: 0,
        nodes: vec![E::Reference {
            module: module.into(),
            name: name.into(),
        }]
        .into(),
    }
}

#[cfg(test)]
pub(super) fn literal(b: bool) -> ExpressionFact {
    ExpressionFact {
        root: 1,
        nodes: vec![
            E::Constant(Constant::Boolean(b)),
            E::Lambda {
                parameter: "x".into(),
                body: 0,
            },
        ]
        .into(),
    }
}

#[allow(clippy::float_cmp, reason = "JS compares numbers exactly")]
fn binary(op: &str, lhs: Constant, rhs: Constant) -> Option<Constant> {
    use Constant::{Boolean as Bool, Number, Text as String};
    Some(match (lhs, rhs) {
        (Number(a), Number(b)) => match op {
            "+" => Number(a + b),
            "-" => Number(a - b),
            "*" => Number(a * b),
            "/" => Number(a / b),
            // Rust `%` on f64 is C `fmod`: exact, sign of the dividend, like JS
            "%" => Number(a % b),
            "**" => Number(power(a, b)?),
            // SameValueZero: NaN equals NaN, +0 equals -0
            "==" => Bool(a == b || (a.is_nan() && b.is_nan())),
            "!=" => Bool(!(a == b || (a.is_nan() && b.is_nan()))),
            "<" => Bool(a < b),
            "<=" => Bool(a <= b),
            ">" => Bool(a > b),
            ">=" => Bool(a >= b),
            _ => return None,
        },
        (String(a), String(b)) => {
            let order = || a.encode_utf16().cmp(b.encode_utf16());
            match op {
                "++" => String(a.clone() + &b),
                "==" => Bool(a == b),
                "!=" => Bool(a != b),
                "<" => Bool(order().is_lt()),
                "<=" => Bool(order().is_le()),
                ">" => Bool(order().is_gt()),
                ">=" => Bool(order().is_ge()),
                _ => return None,
            }
        }
        (Bool(a), Bool(b)) => match op {
            "&&" => Bool(a && b),
            "||" => Bool(a || b),
            "==" => Bool(a == b),
            "!=" => Bool(a != b),
            _ => return None,
        },
        _ => return None,
    })
}

/// `a ** b` only where the result is certain to equal JS `Math.pow`: a zero
/// exponent, or an integer power with an exactly representable result.
/// Engines' `pow` may differ in the last bit elsewhere.
#[allow(clippy::float_cmp, reason = "exactness is the point")]
fn power(base: f64, exponent: f64) -> Option<f64> {
    if exponent == 0.0 {
        return Some(1.0);
    }
    let integral = |x: f64| x.is_finite() && x.fract() == 0.0 && x.abs() < 9_007_199_254_740_992.0;
    if !integral(base) || !integral(exponent) || exponent < 0.0 {
        return None;
    }
    let odd = exponent % 2.0 == 1.0;
    match base {
        // keeps the sign of `-0` for odd exponents, like JS
        0.0 => return Some(if odd { base } else { 0.0 }),
        1.0 => return Some(1.0),
        -1.0 => return Some(if odd { -1.0 } else { 1.0 }),
        _ => {}
    }
    // |base| >= 2, so the loop leaves the exact range within 53 steps
    let mut result = 1.0_f64;
    let mut left = exponent;
    while left > 0.0 {
        result *= base;
        left -= 1.0;
        if result.abs() > 9_007_199_254_740_992.0 {
            return None;
        }
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::{Value, binary, holds, integer_binary};
    use flick_plugin::{Constant as C, ExpressionFact, ExpressionKind as E};

    #[test]
    fn local_predicates_are_evaluated_without_host_verdicts() {
        let even = ExpressionFact {
            root: 5,
            nodes: vec![
                E::Variable("n".into()),
                E::Constant(C::Number(2.0)),
                E::Binary {
                    op: "%".into(),
                    left: 0,
                    right: 1,
                },
                E::Constant(C::Number(0.0)),
                E::Binary {
                    op: "==".into(),
                    left: 2,
                    right: 3,
                },
                E::Lambda {
                    parameter: "n".into(),
                    body: 4,
                },
            ]
            .into(),
        };
        assert_eq!(holds(&even, &C::Number(4.0)), Some(true));
        assert_eq!(holds(&even, &C::Number(3.0)), Some(false));
        assert_eq!(holds(&even, &C::Number(f64::INFINITY)), None);
        assert_eq!(holds(&even, &C::Text("4".into())), None);
        let unknown = ExpressionFact {
            root: 1,
            nodes: vec![
                E::Unknown,
                E::Lambda {
                    parameter: "x".into(),
                    body: 0,
                },
            ]
            .into(),
        };
        assert_eq!(holds(&unknown, &C::Number(4.0)), None);
    }

    #[test]
    fn predicate_operators_preserve_javascript_number_and_text_semantics() {
        assert_eq!(
            binary("==", C::Number(f64::NAN), C::Number(f64::NAN)),
            Some(C::Boolean(true))
        );
        assert_eq!(
            binary("%", C::Number(-3.0), C::Number(2.0)),
            Some(C::Number(-1.0))
        );
        assert_eq!(
            binary("<", C::Text("𐀀".into()), C::Text("\u{e000}".into())),
            Some(C::Boolean(true))
        );
        assert_eq!(binary("**", C::Number(2.0), C::Number(0.5)), None);
    }

    #[test]
    fn integer_literals_keep_exact_arithmetic_and_overflow_stays_unknown() {
        for (op, a, b, expected) in [
            ("+", 2, 3, 5),
            ("-", 2, 3, -1),
            ("*", -3, 2, -6),
            ("/", -7, 2, -3),
            ("%", -7, 2, -1),
            ("/", 7, 0, 0),
            ("%", 7, 0, 0),
            ("**", 2, 10, 1024),
        ] {
            let Some(Value::Integer(actual)) = integer_binary(op, a, b) else {
                panic!("{a} {op} {b} has an exact integer result");
            };
            assert_eq!(actual, expected);
        }
        assert!(integer_binary("+", i128::MAX, 1).is_none());
        assert!(integer_binary("**", 2, 127).is_none());
        let expression = ExpressionFact {
            nodes: vec![
                E::BigInt("3".into()),
                E::Unary {
                    op: "-".into(),
                    value: 0,
                },
                E::BigInt("-3".into()),
                E::Binary {
                    op: "==".into(),
                    left: 1,
                    right: 2,
                },
                E::Lambda {
                    parameter: "n".into(),
                    body: 3,
                },
            ]
            .into(),
            root: 4,
        };
        assert_eq!(holds(&expression, &C::Number(1.0)), Some(true));
    }
}
