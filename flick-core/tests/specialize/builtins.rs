//! Binary operators, structural equality, formatting, and built-in foreign
//! helpers for the real-Flick specialization fixture backend.

use flick_ir::{Arith, BinOp, Num};

use crate::backend::Val;
use crate::ops::unbox_if_boxed;

pub(crate) fn eval_binary(op: BinOp, lhs: Val, rhs: Val) -> Val {
    match op {
        BinOp::Concat => match (lhs, rhs) {
            (Val::String(a), Val::String(b)) => Val::String(a + &b),
            (l, r) => panic!("Concat mismatch: {l:?}, {r:?}"),
        },
        BinOp::Arith(arith, Num::Int) => {
            let (Val::Int(a), Val::Int(b)) = (&lhs, &rhs) else {
                panic!("Int arith mismatch: {lhs:?}, {rhs:?}");
            };
            let (a, b) = (*a, *b);
            Val::Int(match arith {
                Arith::Add => a.wrapping_add(b),
                Arith::Sub => a.wrapping_sub(b),
                Arith::Mul => a.wrapping_mul(b),
                Arith::Div => {
                    if b == 0 {
                        0
                    } else {
                        a.wrapping_div(b)
                    }
                }
                Arith::Rem => {
                    if b == 0 {
                        0
                    } else {
                        a.wrapping_rem(b)
                    }
                }
                Arith::Pow => a.wrapping_pow(u32::try_from(b).unwrap_or(0)),
            })
        }
        BinOp::Arith(arith, Num::Float) => {
            let (Val::Float(a), Val::Float(b)) = (&lhs, &rhs) else {
                panic!("Float arith mismatch: {lhs:?}, {rhs:?}");
            };
            let (a, b) = (*a, *b);
            Val::Float(match arith {
                Arith::Add => a + b,
                Arith::Sub => a - b,
                Arith::Mul => a * b,
                Arith::Div => a / b,
                Arith::Rem => a % b,
                Arith::Pow => a.powf(b),
            })
        }
        BinOp::Eq | BinOp::Same => Val::Bool(vals_eq(&lhs, &rhs)),
        BinOp::NotEq | BinOp::NotSame => Val::Bool(!vals_eq(&lhs, &rhs)),
        BinOp::Lt => match (lhs, rhs) {
            (Val::Int(a), Val::Int(b)) => Val::Bool(a < b),
            (Val::Float(a), Val::Float(b)) => Val::Bool(a < b),
            _ => panic!("Lt mismatch"),
        },
        BinOp::Le => match (lhs, rhs) {
            (Val::Int(a), Val::Int(b)) => Val::Bool(a <= b),
            (Val::Float(a), Val::Float(b)) => Val::Bool(a <= b),
            _ => panic!("Le mismatch"),
        },
        BinOp::Gt => match (lhs, rhs) {
            (Val::Int(a), Val::Int(b)) => Val::Bool(a > b),
            (Val::Float(a), Val::Float(b)) => Val::Bool(a > b),
            _ => panic!("Gt mismatch"),
        },
        BinOp::Ge => match (lhs, rhs) {
            (Val::Int(a), Val::Int(b)) => Val::Bool(a >= b),
            (Val::Float(a), Val::Float(b)) => Val::Bool(a >= b),
            _ => panic!("Ge mismatch"),
        },
        BinOp::And => match (lhs, rhs) {
            (Val::Bool(a), Val::Bool(b)) => Val::Bool(a && b),
            _ => panic!("And mismatch"),
        },
        BinOp::Or => match (lhs, rhs) {
            (Val::Bool(a), Val::Bool(b)) => Val::Bool(a || b),
            _ => panic!("Or mismatch"),
        },
        BinOp::ListConcat => match (lhs, rhs) {
            (Val::List(mut a), Val::List(b)) => {
                a.extend(b);
                Val::List(a)
            }
            _ => panic!("ListConcat mismatch"),
        },
        other @ BinOp::Arith(..) => panic!("unsupported binop {other:?}"),
    }
}

fn vals_eq(a: &Val, b: &Val) -> bool {
    match (unbox_if_boxed(a.clone()), unbox_if_boxed(b.clone())) {
        (Val::Unit, Val::Unit) => true,
        (Val::Bool(x), Val::Bool(y)) => x == y,
        (Val::Int(x), Val::Int(y)) => x == y,
        (Val::Float(x), Val::Float(y)) => x == y || (x.is_nan() && y.is_nan()),
        (Val::String(x), Val::String(y)) => x == y,
        (Val::Tuple(xs), Val::Tuple(ys)) | (Val::List(xs), Val::List(ys)) => {
            xs.len() == ys.len() && xs.iter().zip(&ys).all(|(x, y)| vals_eq(x, y))
        }
        (
            Val::Con {
                name: n1,
                fields: f1,
                ..
            },
            Val::Con {
                name: n2,
                fields: f2,
                ..
            },
        ) => n1 == n2 && f1.len() == f2.len() && f1.iter().zip(&f2).all(|(x, y)| vals_eq(x, y)),
        _ => false,
    }
}

pub(crate) fn eval_builtin_foreign(
    module: &str,
    name: &str,
    polymorphic: bool,
    args: &[Val],
) -> Val {
    let wrap = |v: Val| {
        if polymorphic {
            Val::Boxed(Box::new(v))
        } else {
            v
        }
    };
    match (module, name) {
        ("Instances", "show_prim") => wrap(Val::String(show_val(&unbox_if_boxed(args[0].clone())))),
        ("Instances", "eq_prim") => wrap(Val::Bool(vals_eq(&args[0], &args[1]))),
        ("Instances", "index_of") => {
            let (Val::String(needle), Val::String(hay)) = (
                unbox_if_boxed(args[0].clone()),
                unbox_if_boxed(args[1].clone()),
            ) else {
                panic!("index_of expected strings");
            };
            let res = match hay.find(&needle) {
                Some(idx) => Val::Con {
                    name: "Some".into(),
                    fields: vec![Val::Int(i32::try_from(idx).expect("index fits i32"))],
                    dicts: Vec::new(),
                },
                None => Val::Con {
                    name: "None".into(),
                    fields: Vec::new(),
                    dicts: Vec::new(),
                },
            };
            wrap(res)
        }
        ("Instances", "text_contains") => {
            let (Val::String(part), Val::String(text)) = (
                unbox_if_boxed(args[0].clone()),
                unbox_if_boxed(args[1].clone()),
            ) else {
                panic!("text_contains expected strings");
            };
            wrap(Val::Bool(text.contains(&part)))
        }
        ("Instances", "text_starts_with") => {
            let (Val::String(prefix), Val::String(text)) = (
                unbox_if_boxed(args[0].clone()),
                unbox_if_boxed(args[1].clone()),
            ) else {
                panic!("text_starts_with expected strings");
            };
            wrap(Val::Bool(text.starts_with(&prefix)))
        }
        ("Instances", "count_of") => {
            let Val::List(xs) = unbox_if_boxed(args[0].clone()) else {
                panic!("count_of expected List");
            };
            wrap(Val::Int(i32::try_from(xs.len()).expect("len fits i32")))
        }
        ("Instances", "item_at") => {
            let (Val::Int(i), Val::List(xs)) = (
                unbox_if_boxed(args[0].clone()),
                unbox_if_boxed(args[1].clone()),
            ) else {
                panic!("item_at expected (Int, List)");
            };
            let res = usize::try_from(i)
                .ok()
                .and_then(|u| xs.get(u).cloned())
                .map_or_else(
                    || Val::Con {
                        name: "None".into(),
                        fields: Vec::new(),
                        dicts: Vec::new(),
                    },
                    |v| Val::Con {
                        name: "Some".into(),
                        fields: vec![v],
                        dicts: Vec::new(),
                    },
                );
            wrap(res)
        }
        _ => panic!("no foreign hook registered for `{module}.{name}`"),
    }
}

fn show_val(v: &Val) -> String {
    match v {
        Val::Unit => "()".to_owned(),
        Val::Bool(b) => b.to_string(),
        Val::Int(n) => n.to_string(),
        Val::Float(f) => format!("{f}"),
        Val::String(s) => format!("\"{s}\""),
        Val::Boxed(inner) => show_val(inner),
        Val::List(items) => {
            let parts: Vec<String> = items.iter().map(show_val).collect();
            format!("[{}]", parts.join(", "))
        }
        Val::Con { name, fields, .. } => {
            if fields.is_empty() {
                name.clone()
            } else {
                let parts: Vec<String> = fields.iter().map(show_val).collect();
                format!("{name} {}", parts.join(" "))
            }
        }
        other => format!("{other:?}"),
    }
}
