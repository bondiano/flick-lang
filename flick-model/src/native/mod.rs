//! Natives: the foreign bindings of the standard library that the model
//! runs, by JS module and export, as the Go backend binds them
//! (`packages/*/src/go/`). Each is a step under
//! hypothesis G1 of `docs/formal/core.md` §4. A native returns a term, not
//! only a value, so one that calls a Flick function hands the call back to
//! the machine.

#![allow(
    clippy::unnecessary_wraps,
    reason = "every native has the signature of `Native::run`, which may fail"
)]

mod exit;
pub mod host;
mod lazy;
mod numbers;
pub(crate) mod seq;
mod show;
mod struct_ops;

pub(crate) use lazy::settled;

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
    RT, "showInt64", 1 => numbers::show_int64;
    RT, "showUInt64", 1 => numbers::show_uint64;
    RT, "toInt8", 1 => numbers::to_int8;
    RT, "toInt16", 1 => numbers::to_int16;
    RT, "toInt32", 1 => numbers::to_int32;
    RT, "toInt64", 1 => numbers::to_int64;
    RT, "toUInt8", 1 => numbers::to_uint8;
    RT, "toUInt16", 1 => numbers::to_uint16;
    RT, "toUInt32", 1 => numbers::to_uint32;
    RT, "toUInt64", 1 => numbers::to_uint64;
    RT, "toFloat32", 1 => numbers::to_float32;
    RT, "toFloat64", 1 => numbers::number;
    RT, "toBigInt", 1 => numbers::bigint;
    RT, "int64BitAnd", 2 => numbers::int64_bit_and;
    RT, "int64BitOr", 2 => numbers::int64_bit_or;
    RT, "int64BitXor", 2 => numbers::int64_bit_xor;
    RT, "int64BitNot", 1 => numbers::int64_bit_not;
    RT, "int64ShiftLeft", 2 => numbers::int64_shift_left;
    RT, "int64ShiftRight", 2 => numbers::int64_shift_right;
    RT, "int64ShiftRightUnsigned", 2 => numbers::int64_shift_right_unsigned;
    RT, "stdoutLine", 1 => stdout_line;
    RT, "stderrLine", 1 => host::stderr_line;
    RT, "stdoutWrite", 1 => host::stdout_write;
    RT, "platformEnv", 1 => host::env;
    RT, "platformVars", 1 => host::vars;
    RT, "platformArgs", 1 => host::args;
    "flick:Clock", "flickCurrentTimeMillis", 1 => host::current_time_millis;
    "flick:Clock", "flickCurrentTimeNanos", 1 => host::current_time_nanos;
    "flick:Clock", "flickMonotonicTimeNanos", 1 => host::monotonic_time_nanos;
    "flick:Random", "flickNext", 1 => host::next;
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
    RT, "seqOfList", 1 => seq::of_list;
    RT, "seqFrom", 1 => seq::from;
    RT, "seqRange", 2 => seq::range;
    RT, "seqRangeTo", 2 => seq::range_to;
    RT, "seqRangeBy", 3 => seq::range_by;
    RT, "seqIterate", 2 => seq::iterate;
    RT, "seqRepeat", 1 => seq::repeat;
    RT, "seqCycle", 1 => seq::cycle;
    RT, "seqUnfold", 2 => seq::unfold;
    RT, "seqAppend", 2 => seq::append;
    RT, "seqMap", 2 => seq::map;
    RT, "seqMap2", 3 => seq::map2;
    RT, "seqFilter", 2 => seq::filter;
    RT, "seqFilterMap", 2 => seq::filter_map;
    RT, "seqFlatMap", 2 => seq::flat_map;
    RT, "seqTake", 2 => seq::take;
    RT, "seqTakeWhile", 2 => seq::take_while;
    RT, "seqDrop", 2 => seq::drop;
    RT, "seqDropWhile", 2 => seq::drop_while;
    RT, "seqZip", 2 => seq::zip;
    RT, "seqZipWith", 3 => seq::zip_with;
    RT, "seqEnumerate", 1 => seq::enumerate;
    RT, "seqScan", 3 => seq::scan;
    RT, "seqChunks", 2 => seq::chunks;
    RT, "seqWindows", 2 => seq::windows;
    RT, "seqToList", 1 => seq::to_list;
    RT, "seqFold", 3 => seq::fold;
    RT, "seqAny", 2 => seq::any;
    RT, "seqAll", 2 => seq::all;
    RT, "seqFind", 2 => seq::find;
    RT, "seqFirst", 1 => seq::first;
    RT, "seqEachEffect", 2 => seq::each_effect;
    RT, "seqFoldEffect", 3 => seq::fold_effect;
    RT, "optionMap2", 3 => option_map2;
    RT, "resultMap2", 3 => result_map2;
    RT, "succeed", 1 => succeed;
    RT, "awaitEff", 1 => |args, _| Ok(await_(&args[0]));
    RT, "mapEff", 2 => map_eff;
    RT, "flatMapEff", 2 => flat_map_eff;
    RT, "map2Eff", 3 => map2_eff;
    RT, "fromResult", 1 => from_result;
    RT, "refModify", 2 => ref_modify;
    RT, "lazyMake", 1 => lazy::lazy_make;
    RT, "lazyMakeLocal", 1 => lazy::lazy_make_local;
    RT, "lazyForce", 1 => lazy::lazy_force;
    RT, "lazyDone", 1 => lazy::lazy_done;
    RT, "layerSucceed", 2 => layer_succeed;
    RT, "layerMerge", 2 => layer_merge;
    RT, "layerOverride", 2 => layer_merge;
    "flick:Layer", "effect", 2 => layer_effect;
    "flick:Exit", "flickIsFailure", 1 => exit::is_failure;
    "flick:Exit", "flickIsSuccess", 1 => exit::is_success;
    RT, "flickTodo", 1 => todo;
    STRING, "includes", 2 => includes;
    STRING, "startsWith", 2 => starts_with;
    MODEL, "structShow", 2 => struct_ops::struct_show;
    MODEL, "structEq", 3 => struct_ops::struct_eq;
    MODEL, "seqListAt", 3 => seq::list_at;
    MODEL, "seqFromStep", 2 => seq::from_step;
    MODEL, "seqRangeStep", 3 => seq::range_step;
    MODEL, "seqRangeToStep", 3 => seq::range_to_step;
    MODEL, "seqRangeByStep", 4 => seq::range_by_step;
    MODEL, "seqIterateStep", 3 => seq::iterate_step;
    MODEL, "seqRepeatStep", 2 => seq::repeat_step;
    MODEL, "seqCycleStep", 3 => seq::cycle_step;
    MODEL, "seqUnfoldStep", 3 => seq::unfold_step;
    MODEL, "seqAppendStep", 3 => seq::append_step;
    MODEL, "seqMapStep", 3 => seq::map_step;
    MODEL, "seqFilterStep", 3 => seq::filter_step;
    MODEL, "seqFilterMapStep", 3 => seq::filter_map_step;
    MODEL, "seqFlatMapStep", 4 => seq::flat_map_step;
    MODEL, "seqTakeStep", 3 => seq::take_step;
    MODEL, "seqTakeWhileStep", 3 => seq::take_while_step;
    MODEL, "seqDropStep", 3 => seq::drop_step;
    MODEL, "seqDropWhileStep", 3 => seq::drop_while_step;
    MODEL, "seqZipWithStep", 4 => seq::zip_with_step;
    MODEL, "seqEnumerateStep", 3 => seq::enumerate_step;
    MODEL, "seqScanStep", 4 => seq::scan_step;
    MODEL, "seqChunksStep", 4 => seq::chunks_step;
    MODEL, "seqWindowsStep", 4 => seq::windows_step;
    MODEL, "seqToListAcc", 2 => seq::to_list_acc;
    MODEL, "seqRevToList", 1 => seq::rev_to_list;
    MODEL, "seqFoldStep", 3 => seq::fold_step;
    MODEL, "seqAnyStep", 2 => seq::any_step;
    MODEL, "seqAllStep", 2 => seq::all_step;
    MODEL, "seqFindStep", 2 => seq::find_step;
    MODEL, "seqEachStep", 2 => seq::each_step;
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

pub(super) fn list(v: &Term) -> Result<&[Term], String> {
    match v {
        Term::List(xs) => Ok(xs),
        other => Err(format!("not a list: {other:?}")),
    }
}

/// `match c with True → then | False → else`.
pub(super) fn cond(c: Term, then: Term, otherwise: Term) -> Term {
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
