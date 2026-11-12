//! The methods of a derived instance of `Eq`, `Ord`, `Hash` or `Show`,
//! written out for its type: a `match` on the constructors that asks the
//! instance of each field in turn. A backend compiles them as it compiles
//! any method, with the fields where its values keep them, and the
//! optimizer may call them where the instance is known.
//!
//! - `eq`: the same constructor and every field equal, left to right.
//! - `compare`: by constructor in the order declared, then field by field
//!   to the first that is not `EQ`.
//! - `hash`: the index of the constructor, then `h * 31 + hash field` for
//!   each field, in `Int` arithmetic.
//! - `show`: the name, then each field shown, in parentheses when it has a
//!   space and opens no bracket or string (`Instances.show_arg`).
//!
//! A type without variants is its one field: its methods are that field's.

use crate::ir::{Arm, BinOp, Expr, ExprKind, Lit, Param, Pat, Stage, Stmt};
use crate::numeric::{Arith, Num};
use crate::source::Span;
use crate::types::{Constructor, Dict, Rep};

/// The module of the instances of built-in types and of `show_arg`.
const INSTANCES: &str = "Instances";

/// A constructor with the dictionaries of its fields.
type Variant<'a> = (&'a Constructor, &'a [Dict]);

struct At(Span);

impl At {
    fn of(&self, kind: ExprKind) -> Expr {
        Expr::new(kind, self.0)
    }

    fn local(&self, name: &str) -> Expr {
        self.of(ExprKind::Local(name.to_owned()))
    }

    fn int(&self, n: usize) -> Expr {
        let n = i32::try_from(n).expect("fewer constructors than `i32::MAX`");
        self.of(ExprKind::Lit(Lit::Int(n)))
    }

    fn call(&self, func: Expr, args: impl IntoIterator<Item = Expr>) -> Expr {
        args.into_iter().fold(func, |func, arg| {
            self.of(ExprKind::App {
                func: Box::new(func),
                arg: Box::new(arg),
                stage: Stage::Pure,
                arg_first: false,
            })
        })
    }

    /// The method `name` of the instance `dict`, applied to `args`.
    fn method(&self, dict: &Dict, name: &str, args: impl IntoIterator<Item = Expr>) -> Expr {
        let method = ExprKind::Method {
            dict: dict.clone(),
            name: name.to_owned(),
        };
        self.call(self.of(method), args)
    }

    fn binary(&self, op: BinOp, lhs: Expr, rhs: Expr) -> Expr {
        self.of(ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)))
    }

    fn matched(&self, subject: &str, arms: Vec<Arm>) -> Expr {
        self.of(ExprKind::Match(Box::new(self.local(subject)), arms))
    }

    fn lambda(&self, params: &[&str], body: Expr) -> Expr {
        let params = params.iter().map(|p| Param {
            pat: Pat::Bind((*p).to_owned()),
            stage: Stage::Pure,
            ty: None,
        });
        self.of(ExprKind::Lambda {
            params: params.collect(),
            body: Box::new(body),
        })
    }
}

fn arm(pat: Pat, body: Expr) -> Arm {
    Arm {
        pat,
        guard: None,
        body,
    }
}

/// The fields of constructor `i` as locals named after `side`: every name
/// of a method is its own.
fn names(side: &str, i: usize, arity: usize) -> Vec<String> {
    (0..arity).map(|k| format!("{side}{i}_{k}")).collect()
}

fn pattern(c: &Constructor, fields: &[String]) -> Pat {
    Pat::Con(c.clone(), fields.iter().cloned().map(Pat::Bind).collect())
}

/// The methods of a derived instance of `class`: `dicts` are the instances
/// of the fields of every constructor in order, or of the one field of a
/// type without variants. None for a class the runtime derives, and for a
/// constructor that holds dictionaries of its own.
pub fn methods(
    class: &str,
    dicts: &[Dict],
    constructors: Option<&[Constructor]>,
    span: Span,
) -> Vec<(String, Expr)> {
    let at = At(span);
    let name = match class {
        "Eq" => "eq",
        "Ord" => "compare",
        "Hash" => "hash",
        "Show" => "show",
        _ => return Vec::new(),
    };
    let Some(constructors) = constructors else {
        // the one field is the representation
        return match dicts.first() {
            Some(dict) => vec![(name.to_owned(), at.method(dict, name, []))],
            None => Vec::new(),
        };
    };
    let fields: usize = constructors.iter().map(|c| c.arity).sum();
    if constructors.iter().any(|c| c.dicts > 0) || fields != dicts.len() {
        return Vec::new();
    }
    let mut offset = 0;
    let variants: Vec<Variant> = (constructors.iter())
        .map(|c| {
            offset += c.arity;
            (c, &dicts[offset - c.arity..offset])
        })
        .collect();
    let body = match class {
        "Eq" => eq(&at, &variants),
        "Ord" => compare(&at, &variants),
        "Hash" => hash(&at, &variants),
        _ => show(&at, &variants),
    };
    vec![(name.to_owned(), body)]
}

/// `match a with | K x… -> match b with | K y… -> same | _ -> other`: what
/// two values of one constructor give by their fields, and `other` of the
/// index of the constructor of `a` for two of different ones.
fn both(
    at: &At,
    variants: &[Variant],
    same: impl Fn(usize, &[String], &[String], &[Dict]) -> Expr,
    other: impl Fn(usize) -> Expr,
) -> Expr {
    let arms = variants.iter().enumerate().map(|(i, (c, dicts))| {
        let (xs, ys) = (names("x", i, c.arity), names("y", i, c.arity));
        let mut inner = vec![arm(pattern(c, &ys), same(i, &xs, &ys, dicts))];
        if variants.len() > 1 {
            inner.push(arm(Pat::Wild, other(i)));
        }
        arm(pattern(c, &xs), at.matched("b", inner))
    });
    at.matched("a", arms.collect())
}

fn eq(at: &At, variants: &[Variant]) -> Expr {
    let no = || at.of(ExprKind::Lit(Lit::Bool(false)));
    let same = |_: usize, xs: &[String], ys: &[String], dicts: &[Dict]| {
        let fields = xs
            .iter()
            .zip(ys)
            .zip(dicts)
            .map(|((x, y), d)| at.method(d, "eq", [at.local(x), at.local(y)]));
        fields
            .reduce(|all, next| at.binary(BinOp::And, all, next))
            .unwrap_or_else(|| at.of(ExprKind::Lit(Lit::Bool(true))))
    };
    at.lambda(&["a", "b"], both(at, variants, same, |_| no()))
}

fn compare(at: &At, variants: &[Variant]) -> Expr {
    let prelude = |name: &str| Constructor {
        module: "Prelude".to_owned(),
        ty: "Ordering".to_owned(),
        name: name.to_owned(),
        arity: 0,
        rep: Rep::Tagged,
        dicts: 0,
    };
    let ord_int = Dict::Instance {
        module: INSTANCES.to_owned(),
        name: "Ord Int".to_owned(),
        args: Vec::new(),
    };
    let same = |i: usize, xs: &[String], ys: &[String], dicts: &[Dict]| {
        let fields = xs.iter().zip(ys).zip(dicts).enumerate().rev();
        let equal = at.of(ExprKind::Con(prelude("EQ")));
        fields.fold(equal, |rest, (k, ((x, y), d))| {
            // the order of this field unless it is `EQ`
            let other = format!("o{i}_{k}");
            let order = at.method(d, "compare", [at.local(x), at.local(y)]);
            let arms = vec![
                arm(Pat::Con(prelude("EQ"), Vec::new()), rest),
                arm(Pat::Bind(other.clone()), at.local(&other)),
            ];
            at.of(ExprKind::Match(Box::new(order), arms))
        })
    };
    // of different constructors: the order of their places in the declaration
    let other = |i: usize| at.method(&ord_int, "compare", [at.int(i), at.local("j")]);
    let body = both(at, variants, same, other);
    if variants.len() < 2 {
        return at.lambda(&["a", "b"], body);
    }
    let index = variants.iter().enumerate().map(|(i, (c, _))| {
        let fields = vec![Pat::Wild; c.arity];
        arm(Pat::Con((*c).clone(), fields), at.int(i))
    });
    let place = Stmt::Let(Pat::Bind("j".to_owned()), at.matched("b", index.collect()));
    at.lambda(
        &["a", "b"],
        at.of(ExprKind::Block(vec![place], Box::new(body))),
    )
}

fn hash(at: &At, variants: &[Variant]) -> Expr {
    let int = |op| BinOp::Arith(op, Num::Int);
    let arms = variants.iter().enumerate().map(|(i, (c, dicts))| {
        let xs = names("x", i, c.arity);
        let body = xs.iter().zip(*dicts).fold(at.int(i), |h, (x, d)| {
            let times = at.binary(int(Arith::Mul), h, at.int(31));
            at.binary(int(Arith::Add), times, at.method(d, "hash", [at.local(x)]))
        });
        arm(pattern(c, &xs), body)
    });
    at.lambda(&["a"], at.matched("a", arms.collect()))
}

fn show(at: &At, variants: &[Variant]) -> Expr {
    let text = |s: &str| at.of(ExprKind::Lit(Lit::String(s.to_owned())));
    let show_arg = || {
        at.of(ExprKind::Global {
            module: INSTANCES.to_owned(),
            name: "show_arg".to_owned(),
        })
    };
    let arms = variants.iter().enumerate().map(|(i, (c, dicts))| {
        let xs = names("x", i, c.arity);
        let body = xs.iter().zip(*dicts).fold(text(&c.name), |shown, (x, d)| {
            let field = at.call(show_arg(), [at.method(d, "show", [at.local(x)])]);
            let spaced = at.binary(BinOp::Concat, shown, text(" "));
            at.binary(BinOp::Concat, spaced, field)
        });
        arm(pattern(c, &xs), body)
    });
    at.lambda(&["a"], at.matched("a", arms.collect()))
}
