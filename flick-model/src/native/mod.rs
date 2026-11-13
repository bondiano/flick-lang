//! Natives: the foreign bindings of the standard library that the model
//! runs, by JS module and export, as the Go backend binds them
//! (`packages/std/src/go/`). Each is a step under
//! hypothesis G1 of `docs/formal/core.md` §4. A native returns a term, not
//! only a value, so one that calls a Flick function hands the call back to
//! the machine.

#![allow(
    clippy::unnecessary_wraps,
    reason = "every native has the signature of `Native::run`, which may fail"
)]

mod numbers;
mod show;

use std::fmt;
use std::rc::Rc;

use crate::eval::equal;
use crate::syntax::{Arm, Con, Const, Head, Lit, Pat, Path, Prim, Term};

pub struct Native {
    pub module: &'static str,
    pub export: &'static str,
    /// The arguments it takes in Flick: dictionaries, then every stage.
    pub arity: usize,
    pub run: fn(&[Term], &mut String) -> Result<Term, String>,
}

impl fmt::Debug for Native {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.module, self.export)
    }
}

const RT: &str = "flick:rt";

macro_rules! natives {
    ($($module:expr, $export:literal, $arity:literal => $run:expr;)*) => {
        static NATIVES: &[Native] = &[
            $(Native { module: $module, export: $export, arity: $arity, run: $run },)*
        ];
    };
}

natives! {
    "global", "Number", 1 => numbers::number;
    "global", "BigInt", 1 => numbers::bigint;
    RT, "stdoutLine", 1 => stdout_line;
    RT, "show", 1 => show::show;
    RT, "listShow", 2 => show::list_show;
    RT, "equals", 2 => equals;
    RT, "compare", 2 => show::compare;
    RT, "hash", 1 => show::hash;
    RT, "numberStrictEqual", 2 => number_strict_equal;
    RT, "numberSameValue", 2 => number_same_value;
    RT, "listEquals", 3 => list_equals;
    RT, "listCompare", 3 => show::list_compare;
    RT, "listMap", 2 => list_map;
    RT, "listLength", 1 => list_length;
    RT, "listFlatMap", 2 => list_flat_map;
    RT, "listFold", 3 => list_fold;
    RT, "listMap2", 3 => list_map2;
    RT, "listRange", 2 => list_range;
    RT, "optionMap2", 3 => option_map2;
    RT, "resultMap2", 3 => result_map2;
    RT, "succeed", 1 => succeed;
    RT, "mapEff", 2 => map_eff;
    RT, "flatMapEff", 2 => flat_map_eff;
    RT, "map2Eff", 3 => map2_eff;
    RT, "fromResult", 1 => from_result;
    RT, "refModify", 2 => ref_modify;
    RT, "layerSucceed", 2 => layer_succeed;
    RT, "layerMerge", 2 => layer_merge;
    "flick:Layer", "effect", 2 => layer_effect;
    RT, "flickTodo", 1 => todo;
    STRING, "includes", 2 => includes;
    STRING, "startsWith", 2 => starts_with;
    MODEL, "structShow", 2 => struct_show;
    MODEL, "structEq", 3 => struct_eq;
}

/// The methods of JS strings, the receiver last, as `arity 1` binds them.
const STRING: &str = "flick:String";

/// Natives of the model itself: what flick-core leaves to the runtime
/// without a foreign binding.
const MODEL: &str = "$model";

/// A native of the model's own, as a constant.
fn model(export: &str) -> Term {
    let n = find(MODEL, export).expect("a native of the model");
    Term::konst(Const::Native(n))
}

/// The record of the compiler-owned instance of `class` for a record or a
/// tuple whose fields have the dictionaries `fields`.
pub fn structural_dict(class: &str, fields: &[Term]) -> Term {
    let list = Term::List(fields.into());
    let var = |x: &str| Term::Var(x.into());
    let (name, body) = match class {
        "Show" => (
            "show",
            Term::lam("$v", Term::apps(model("structShow"), [list, var("$v")])),
        ),
        "Eq" => (
            "eq",
            Term::lam(
                "$a",
                Term::lam(
                    "$b",
                    Term::apps(model("structEq"), [list, var("$a"), var("$b")]),
                ),
            ),
        ),
        other => (
            "compare",
            Term::Stuck(format!("the instance `{other}` of a record or tuple").into()),
        ),
    };
    Term::Record(Rc::from([(Rc::from(name), body)]))
}

/// The native of a foreign binding, if the model has one.
pub fn find(module: &str, export: &str) -> Option<&'static Native> {
    NATIVES
        .iter()
        .find(|n| n.module == module && n.export == export)
}

/// The constructor `name` of a Prelude type.
pub fn prelude(ty: &str, name: &str, arity: usize) -> Rc<Con> {
    Rc::new(Con {
        ty: Path::new("Prelude", ty),
        name: name.into(),
        arity,
        dicts: 0,
    })
}

pub fn data(c: Rc<Con>, args: Vec<Term>) -> Term {
    Term::Apply(Head::Con(c), args.into())
}

/// `a₁ ++ (a₂ ++ …)`.
pub fn concat(parts: Vec<Term>) -> Term {
    parts
        .into_iter()
        .rev()
        .reduce(|acc, x| Term::apps(Term::prim(Prim::Concat), [x, acc]))
        .unwrap_or_else(|| Term::string(""))
}

/// The arguments of the constructor `name` of `v`, if it is one.
fn of<'a>(v: &'a Term, name: &str) -> Option<&'a [Term]> {
    match v {
        Term::Apply(Head::Con(c), args) if &*c.name == name => Some(args),
        _ => None,
    }
}

fn list(v: &Term) -> Result<&[Term], String> {
    match v {
        Term::List(xs) => Ok(xs),
        other => Err(format!("not a list: {other:?}")),
    }
}

/// `match c with True → then | False → else`.
fn cond(c: Term, then: Term, otherwise: Term) -> Term {
    Term::Match(
        Rc::new(c),
        Rc::from([
            Arm {
                pat: Pat::Lit(Lit::Bool(true)),
                guard: None,
                body: then,
            },
            Arm {
                pat: Pat::Wild,
                guard: None,
                body: otherwise,
            },
        ]),
    )
}

fn stdout_line(args: &[Term], out: &mut String) -> Result<Term, String> {
    match args {
        [Term::Lit(Lit::String(s))] => {
            out.push_str(s);
            out.push('\n');
            Ok(Term::unit())
        }
        _ => Err(format!("{args:?}")),
    }
}

/// `text.includes(part)`, called `includes part text`.
fn includes(args: &[Term], _: &mut String) -> Result<Term, String> {
    match args {
        [Term::Lit(Lit::String(part)), Term::Lit(Lit::String(text))] => {
            Ok(Term::bool(text.contains(&**part)))
        }
        _ => Err(format!("{args:?}")),
    }
}

/// `text.startsWith(prefix)`, called `startsWith prefix text`.
fn starts_with(args: &[Term], _: &mut String) -> Result<Term, String> {
    match args {
        [Term::Lit(Lit::String(prefix)), Term::Lit(Lit::String(text))] => {
            Ok(Term::bool(text.starts_with(&**prefix)))
        }
        _ => Err(format!("{args:?}")),
    }
}

fn floats(args: &[Term]) -> Result<(f64, f64), String> {
    match args {
        [Term::Lit(Lit::Float(a)), Term::Lit(Lit::Float(b))] => Ok((*a, *b)),
        _ => Err(format!("{args:?}")),
    }
}

fn list_length(args: &[Term], _: &mut String) -> Result<Term, String> {
    let values = list(&args[0])?;
    Ok(Term::Lit(Lit::Int(
        values.iter().fold(0_i32, |n, _| n.wrapping_add(1)),
    )))
}

/// JS `===`: `NaN` differs from itself, `0` equals `-0`.
fn number_strict_equal(args: &[Term], _: &mut String) -> Result<Term, String> {
    let (a, b) = floats(args)?;
    #[allow(clippy::float_cmp, reason = "`===` on numbers")]
    Ok(Term::bool(a == b))
}

/// JS `Object.is`: `NaN` equals itself, `0` differs from `-0`.
fn number_same_value(args: &[Term], _: &mut String) -> Result<Term, String> {
    let (a, b) = floats(args)?;
    Ok(Term::bool(
        a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()),
    ))
}

fn equals(args: &[Term], _: &mut String) -> Result<Term, String> {
    let same = equal(&args[0], &args[1]).ok_or("values without `==`")?;
    Ok(Term::bool(same))
}

/// `eq x₁ y₁ && (eq x₂ y₂ && …)`, when the lengths agree.
fn list_equals(args: &[Term], _: &mut String) -> Result<Term, String> {
    let (xs, ys) = (list(&args[1])?, list(&args[2])?);
    if xs.len() != ys.len() {
        return Ok(Term::bool(false));
    }
    let pairs = xs.iter().zip(ys).rev();
    Ok(pairs.fold(Term::bool(true), |rest, (x, y)| {
        cond(
            Term::apps(args[0].clone(), [x.clone(), y.clone()]),
            rest,
            Term::bool(false),
        )
    }))
}

fn list_map(args: &[Term], _: &mut String) -> Result<Term, String> {
    let calls = list(&args[1])?
        .iter()
        .map(|x| Term::app(args[0].clone(), x.clone()));
    Ok(Term::List(calls.collect()))
}

fn list_flat_map(args: &[Term], _: &mut String) -> Result<Term, String> {
    let calls = list(&args[1])?
        .iter()
        .map(|x| Term::app(args[0].clone(), x.clone()));
    Ok(calls
        .rev()
        .reduce(|acc, x| Term::apps(Term::prim(Prim::ListConcat), [x, acc]))
        .unwrap_or_else(|| Term::List(Rc::from([]))))
}

fn list_fold(args: &[Term], _: &mut String) -> Result<Term, String> {
    let xs = list(&args[2])?;
    Ok(xs.iter().fold(args[1].clone(), |acc, x| {
        Term::apps(args[0].clone(), [acc, x.clone()])
    }))
}

/// Every pair, the first list outermost.
fn list_map2(args: &[Term], _: &mut String) -> Result<Term, String> {
    let (xs, ys) = (list(&args[1])?, list(&args[2])?);
    let calls = xs.iter().flat_map(|x| {
        ys.iter()
            .map(|y| Term::apps(args[0].clone(), [x.clone(), y.clone()]))
    });
    Ok(Term::List(calls.collect()))
}

fn list_range(args: &[Term], _: &mut String) -> Result<Term, String> {
    match args {
        [Term::Lit(Lit::Int(from)), Term::Lit(Lit::Int(until))] => Ok(Term::List(
            (*from..*until).map(|x| Term::Lit(Lit::Int(x))).collect(),
        )),
        _ => Err(format!("{args:?}")),
    }
}

fn option_map2(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(match (of(&args[1], "Some"), of(&args[2], "Some")) {
        (Some([x]), Some([y])) => Term::app(
            Term::konst_con(prelude("Option", "Some", 1)),
            Term::apps(args[0].clone(), [x.clone(), y.clone()]),
        ),
        _ => data(prelude("Option", "None", 0), Vec::new()),
    })
}

fn result_map2(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(match (of(&args[1], "Ok"), of(&args[2], "Ok")) {
        (Some([x]), Some([y])) => Term::app(
            Term::konst_con(prelude("Result", "Ok", 1)),
            Term::apps(args[0].clone(), [x.clone(), y.clone()]),
        ),
        (None, _) => args[1].clone(),
        _ => args[2].clone(),
    })
}

fn succeed(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(Term::Task(Rc::new(args[0].clone())))
}

fn await_(t: &Term) -> Term {
    Term::Await(Rc::new(t.clone()))
}

fn map_eff(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(Term::Task(Rc::new(Term::app(
        args[0].clone(),
        await_(&args[1]),
    ))))
}

fn flat_map_eff(args: &[Term], _: &mut String) -> Result<Term, String> {
    let next = Term::app(args[0].clone(), await_(&args[1]));
    Ok(Term::Task(Rc::new(Term::Await(Rc::new(next)))))
}

/// The first, then the second: `map2` runs its arguments in order.
fn map2_eff(args: &[Term], _: &mut String) -> Result<Term, String> {
    let both = Term::Let(
        "$a".into(),
        None,
        Rc::new(await_(&args[1])),
        Rc::new(Term::Let(
            "$b".into(),
            None,
            Rc::new(await_(&args[2])),
            Rc::new(Term::apps(
                args[0].clone(),
                [Term::Var("$a".into()), Term::Var("$b".into())],
            )),
        )),
    );
    Ok(Term::Task(Rc::new(both)))
}

/// `from_result r` of §1.4: `raise v` for `Err v`, `v` for `Ok v`.
fn from_result(args: &[Term], _: &mut String) -> Result<Term, String> {
    match (of(&args[0], "Ok"), of(&args[0], "Err")) {
        (Some([v]), _) => Ok(v.clone()),
        (_, Some([e])) => Ok(Term::Raise(Rc::new(e.clone()))),
        _ => Err(format!("{args:?}")),
    }
}

/// `match f (Ref.get c) with (b, a) → let _ = Ref.set c a in b`.
fn ref_modify(args: &[Term], _: &mut String) -> Result<Term, String> {
    let cell = args[0].clone();
    let get = Term::app(Term::konst(Const::RefGet), cell.clone());
    let set = Term::apps(
        Term::konst(Const::RefSet),
        [cell, Term::Var("$next".into())],
    );
    let arm = Arm {
        pat: Pat::Tuple(Rc::from([
            Pat::Var("$out".into()),
            Pat::Var("$next".into()),
        ])),
        guard: None,
        body: Term::Let(
            "_".into(),
            None,
            Rc::new(set),
            Rc::new(Term::Var("$out".into())),
        ),
    };
    Ok(Term::Match(
        Rc::new(Term::app(args[1].clone(), get)),
        Rc::from([arm]),
    ))
}

fn tag(t: &Term) -> Result<Path, String> {
    match t {
        Term::Tag(s) => Ok(s.clone()),
        other => Err(format!("not a service: {other:?}")),
    }
}

/// `Layer.succeed S impl` is `Layer.succeed_S impl` (§1.4).
fn layer_succeed(args: &[Term], _: &mut String) -> Result<Term, String> {
    let s = tag(&args[0])?;
    Ok(Term::Apply(
        Head::Const(Const::LayerSucceed(s)),
        Rc::from([args[1].clone()]),
    ))
}

fn layer_effect(args: &[Term], _: &mut String) -> Result<Term, String> {
    let s = tag(&args[0])?;
    Ok(Term::Apply(
        Head::Const(Const::LayerEffect(s)),
        Rc::from([args[1].clone()]),
    ))
}

fn layer_merge(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(Term::Apply(Head::Const(Const::LayerMerge), args.into()))
}

/// The values of the fields, and their labels if a record.
type Fields = (Vec<Term>, Option<Vec<Rc<str>>>);

/// The fields of a record in label order, or of a tuple by position.
fn fields(v: &Term) -> Result<Fields, String> {
    match v {
        Term::Tuple(xs) => Ok((xs.to_vec(), None)),
        Term::Record(fs) => {
            let mut fs = fs.to_vec();
            fs.sort_by(|a, b| a.0.cmp(&b.0));
            let labels = fs.iter().map(|(l, _)| l.clone()).collect();
            Ok((fs.into_iter().map(|(_, x)| x).collect(), Some(labels)))
        }
        other => Err(format!("not a record or tuple: {other:?}")),
    }
}

fn field_call(dict: &Term, method: &str, args: impl IntoIterator<Item = Term>) -> Term {
    Term::apps(Term::Field(Rc::new(dict.clone()), method.into()), args)
}

/// `(a, b)` or `{ x = a, y = b }`.
fn struct_show(args: &[Term], _: &mut String) -> Result<Term, String> {
    let dicts = list(&args[0])?;
    let (xs, labels) = fields(&args[1])?;
    let shown = xs
        .iter()
        .zip(dicts)
        .map(|(x, d)| field_call(d, "show", [x.clone()]));
    let mut parts = Vec::new();
    match labels {
        None => {
            parts.push(Term::string("("));
            for (i, s) in shown.enumerate() {
                if i > 0 {
                    parts.push(Term::string(", "));
                }
                parts.push(s);
            }
            parts.push(Term::string(")"));
        }
        Some(labels) => {
            parts.push(Term::string("{ "));
            for (i, (l, s)) in labels.iter().zip(shown).enumerate() {
                if i > 0 {
                    parts.push(Term::string(", "));
                }
                parts.push(Term::string(&format!("{l} = ")));
                parts.push(s);
            }
            parts.push(Term::string(" }"));
        }
    }
    Ok(concat(parts))
}

fn struct_eq(args: &[Term], _: &mut String) -> Result<Term, String> {
    let dicts = list(&args[0])?;
    let ((xs, _), (ys, _)) = (fields(&args[1])?, fields(&args[2])?);
    let pairs: Vec<_> = xs.into_iter().zip(ys).zip(dicts).collect();
    Ok(pairs
        .into_iter()
        .rev()
        .fold(Term::bool(true), |rest, ((x, y), d)| {
            cond(field_call(d, "eq", [x, y]), rest, Term::bool(false))
        }))
}

fn todo(args: &[Term], _: &mut String) -> Result<Term, String> {
    match args {
        [Term::Lit(Lit::String(s))] => Ok(Term::Die(s.clone())),
        _ => Err(format!("{args:?}")),
    }
}

impl Term {
    /// A constructor as a function of its fields.
    pub fn konst_con(c: Rc<Con>) -> Term {
        Term::Apply(Head::Con(c), Rc::from([]))
    }
}
