//! Dictionaries flick-core leaves to the runtime: derived instances, which
//! have no methods in the IR, and the compiler-owned instances of records
//! and tuples. Each is written as λFlick, following `derive` and
//! `structural` of `runtime/src/core/data.ts`.

use std::rc::Rc;

use flick_core::types::Constructor;

use super::expr::{con, cond};
use crate::native::{self, concat};
use crate::syntax::{Arm, Const, Lit, Name, Pat, Prim, Term};

fn var(x: &str) -> Term {
    Term::Var(x.into())
}

fn method(dict: &Term, name: &str) -> Term {
    Term::Field(Rc::new(dict.clone()), name.into())
}

fn arm(pat: Pat, body: Term) -> Arm {
    Arm {
        pat,
        guard: None,
        body,
    }
}

/// `$name0 … $name(n-1)`.
fn names(name: &str, n: usize) -> Vec<Name> {
    (0..n).map(|k| format!("${name}{k}").into()).collect()
}

fn con_pat(c: &Constructor, xs: &[Name]) -> Pat {
    Pat::Con(con(c), xs.iter().map(|x| Pat::Var(x.clone())).collect())
}

fn native(export: &str) -> Term {
    let n = native::find("flick:rt", export).expect("a native of the table");
    Term::konst(Const::Native(n))
}

/// The methods of a derived instance of `class`: `dicts` are those of the
/// fields of every constructor in order, or of the one field of a type
/// without variants.
pub fn methods(
    class: &str,
    dicts: &[Term],
    constructors: Option<&[Constructor]>,
) -> Vec<(Name, Term)> {
    let Some(constructors) = constructors else {
        // the one field is the representation
        let name = match class {
            "Eq" => "eq",
            "Ord" => "compare",
            "Hash" => "hash",
            _ => "show",
        };
        return vec![(name.into(), method(&dicts[0], name))];
    };
    let mut offset = 0;
    let mut variants = Vec::new();
    for c in constructors {
        variants.push((c, &dicts[offset..offset + c.arity]));
        offset += c.arity;
    }
    match class {
        "Show" => vec![("show".into(), show(&variants))],
        "Eq" => vec![("eq".into(), eq(&variants))],
        "Ord" => vec![("compare".into(), compare(&variants))],
        "Hash" => vec![("hash".into(), hash(&variants))],
        other => vec![(
            "hash".into(),
            Term::Stuck(format!("a derived `{other}`").into()),
        )],
    }
}

/// The index of the constructor, combined with the hash of each field as
/// `(Math.imul(h, 31) + x) | 0`, which `Int` arithmetic is.
fn hash(variants: &[Variant]) -> Term {
    use flick_core::numeric::{Arith, Num};
    let arms = variants.iter().enumerate().map(|(i, (c, dicts))| {
        let xs = names("f", c.arity);
        let i = i32::try_from(i).expect("fewer constructors than `i32::MAX`");
        let body = xs
            .iter()
            .zip(dicts.iter())
            .fold(Term::Lit(Lit::Int(i)), |h, (x, d)| {
                let times = Term::apps(
                    Term::prim(Prim::Arith(Arith::Mul, Num::Int)),
                    [h, Term::Lit(Lit::Int(31))],
                );
                let field = Term::app(method(d, "hash"), Term::Var(x.clone()));
                Term::apps(
                    Term::prim(Prim::Arith(Arith::Add, Num::Int)),
                    [times, field],
                )
            });
        arm(con_pat(c, &xs), body)
    });
    Term::lam("$v", Term::Match(Rc::new(var("$v")), arms.collect()))
}

type Variant<'a> = (&'a Constructor, &'a [Term]);

/// `K x₁ … xₙ` as `K` and each argument in parentheses when it has a space.
fn show(variants: &[Variant]) -> Term {
    let arms = variants.iter().map(|(c, dicts)| {
        let xs = names("f", c.arity);
        let mut parts = vec![Term::string(&c.name)];
        for (x, d) in xs.iter().zip(dicts.iter()) {
            parts.push(Term::string(" "));
            let shown = Term::app(method(d, "show"), Term::Var(x.clone()));
            parts.push(Term::app(Term::prim(Prim::Atom), shown));
        }
        arm(con_pat(c, &xs), concat(parts))
    });
    Term::lam("$v", Term::Match(Rc::new(var("$v")), arms.collect()))
}

/// Both of one constructor, with equal fields.
fn eq(variants: &[Variant]) -> Term {
    let mut arms: Vec<Arm> = variants
        .iter()
        .map(|(c, dicts)| {
            let (xs, ys) = (names("x", c.arity), names("y", c.arity));
            let pat = Pat::Tuple(Rc::from([con_pat(c, &xs), con_pat(c, &ys)]));
            let fields = xs.iter().zip(&ys).zip(dicts.iter()).rev();
            let body = fields.fold(Term::bool(true), |rest, ((x, y), d)| {
                let same = Term::apps(
                    method(d, "eq"),
                    [Term::Var(x.clone()), Term::Var(y.clone())],
                );
                cond(same, rest, Term::bool(false))
            });
            arm(pat, body)
        })
        .collect();
    arms.push(arm(Pat::Wild, Term::bool(false)));
    let pair = Term::Tuple(Rc::from([var("$a"), var("$b")]));
    Term::lam(
        "$a",
        Term::lam("$b", Term::Match(Rc::new(pair), arms.into())),
    )
}

/// By constructor first, in declaration order, then field by field.
fn compare(variants: &[Variant]) -> Term {
    let equal = native::data(native::prelude("Ordering", "EQ", 0), Vec::new());
    let mut arms: Vec<Arm> = variants
        .iter()
        .map(|(c, dicts)| {
            let (xs, ys) = (names("x", c.arity), names("y", c.arity));
            let pat = Pat::Tuple(Rc::from([con_pat(c, &xs), con_pat(c, &ys)]));
            let fields = xs.iter().zip(&ys).zip(dicts.iter()).rev();
            let body = fields.fold(equal.clone(), |rest, ((x, y), d)| {
                let o = Term::apps(
                    method(d, "compare"),
                    [Term::Var(x.clone()), Term::Var(y.clone())],
                );
                let on_equal = arm(
                    Pat::Con(native::prelude("Ordering", "EQ", 0), Rc::from([])),
                    rest,
                );
                Term::Match(
                    Rc::new(o),
                    Rc::from([on_equal, arm(Pat::Var("$o".into()), var("$o"))]),
                )
            });
            arm(pat, body)
        })
        .collect();
    let index = |x: &str| {
        let arms = variants.iter().enumerate().map(|(i, (c, _))| {
            let i = i32::try_from(i).expect("fewer constructors than `i32::MAX`");
            arm(con_pat(c, &names("_", c.arity)), Term::Lit(Lit::Int(i)))
        });
        Term::Match(Rc::new(var(x)), arms.collect())
    };
    arms.push(arm(
        Pat::Wild,
        Term::apps(native("compare"), [index("$a"), index("$b")]),
    ));
    let pair = Term::Tuple(Rc::from([var("$a"), var("$b")]));
    Term::lam(
        "$a",
        Term::lam("$b", Term::Match(Rc::new(pair), arms.into())),
    )
}

/// The compiler-owned instance of class `class` for a record or a tuple.
pub fn structural(class: &str, fields: Vec<Term>) -> Term {
    Term::Structural(class.into(), fields.into())
}
