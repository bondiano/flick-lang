//! Classes in the programs (`docs/formal/core.md` §1.6): a method of
//! `Describe` or `Rank` of `Gen.fl` at a type with an instance, through the
//! dictionary of that instance, of the instance of `Option` at the inner
//! one, or of the superclass `$Describe` of a `Rank`. flick-core resolves
//! the dictionaries from the source, where the argument of a method is
//! annotated with its type, so no variable is ambiguous.

use std::rc::Rc;

use super::term::Cx;
use super::{Gen, MODULE, boolean, error_con, int, mono, option, string};
use crate::syntax::{Path, Term};
use crate::translate::instance;
use crate::types::{Kind, Scheme, Stage, Type, Var};

/// The instances of `Gen.fl`, by class and head.
pub(super) const INSTANCES: [(&str, &str); 6] = [
    ("Describe", "Int"),
    ("Describe", "Bool"),
    ("Describe", "Oops"),
    ("Describe", "(Option a)"),
    ("Rank", "Int"),
    ("Rank", "Bool"),
];

fn global(class: &str, head: &str) -> Term {
    Term::Global(instance(MODULE, &format!("{class} {head}")))
}

/// The globals of the instances the programs name.
pub fn instances() -> impl Iterator<Item = Path> {
    INSTANCES
        .iter()
        .map(|(class, head)| instance(MODULE, &format!("{class} {head}")))
}

fn oops() -> Type {
    Type::Con(Path::new(MODULE, "Oops"))
}

/// The dictionary of `class` at `t` the instances of `Gen.fl` give.
pub(super) fn dictionary(class: &str, t: &Type) -> Option<Term> {
    match (class, t) {
        (_, t) if *t == int() => Some(global(class, "Int")),
        (_, t) if *t == boolean() => Some(global(class, "Bool")),
        ("Describe", t) if *t == oops() => Some(global(class, "Oops")),
        ("Describe", Type::App(h, a)) if **h == Type::con("Prelude", "Option") => Some(Term::app(
            global(class, "(Option a)"),
            dictionary(class, a)?,
        )),
        _ => None,
    }
}

/// Whether `e` is a dictionary: of an instance, applied to those of its
/// context, a given `?d`, or the superclass field of one.
pub(super) fn is_dict(e: &Term) -> bool {
    match e {
        Term::Global(p) => p.name.starts_with("instance "),
        Term::Var(x) => x.starts_with('?'),
        Term::App(f, a) => is_dict(f) && is_dict(a),
        Term::Field(d, l) => l.starts_with('$') && is_dict(d),
        _ => false,
    }
}

/// `(e : t)`: an annotated term, as the source writes it.
pub(super) fn annotated(x: &str, e: Term, t: &Type) -> Term {
    let x: Rc<str> = x.into();
    Term::Let(
        x.clone(),
        Some(Rc::new(mono(t.clone()))),
        Rc::new(e),
        Rc::new(Term::Var(x)),
    )
}

impl Gen<'_> {
    /// A type `class` has an instance at that `term` builds values of: not
    /// `Oops`, whose value only `method` makes.
    pub(super) fn instance_type(&mut self, class: &str) -> Type {
        let ts = [int(), boolean(), option(int())];
        let n = if class == "Rank" { 2 } else { ts.len() };
        ts[self.pick(n)].clone()
    }

    /// `describe (e : T)` of type `String`, or `rank (e : T)` of type `Int`.
    pub(super) fn method(&mut self, cx: &Cx, t: &Type, budget: &Stage, s: u32) -> Term {
        let (class, method) = if *t == string() {
            ("Describe", "describe")
        } else {
            ("Rank", "rank")
        };
        // `describe` through the superclass of a `Rank`
        let via_rank = class == "Describe" && self.one_in(3);
        let at = if class == "Describe" && !via_rank && self.one_in(4) {
            oops()
        } else {
            self.instance_type(if via_rank { "Rank" } else { class })
        };
        let dict = if via_rank {
            let rank = dictionary("Rank", &at).expect("an instance of `Rank`");
            Term::Field(Rc::new(rank), "$Describe".into())
        } else {
            dictionary(class, &at).expect("an instance")
        };
        let arg = if at == oops() {
            let n = self.term(cx, &int(), budget, s, false);
            Term::app(Term::konst_con(error_con("Oops")), n)
        } else {
            self.term(cx, &at, budget, s, false)
        };
        let x = self.fresh("m");
        Term::app(
            Term::Field(Rc::new(dict), method.into()),
            annotated(&x, arg, &at),
        )
    }
}

/// Templates 9 and 10 of `bind.rs`: `∀a. Describe a => a -> String`, its
/// method called on the given dictionary, or `∀a. Rank a => a -> String`
/// through its superclass; bound with the signature, as flick-core needs.
pub(super) fn template(via_rank: bool) -> (Scheme, Term, bool, bool) {
    let a = Type::Var(Var::Named("a".into()));
    let class = if via_rank { "Rank" } else { "Describe" };
    let given = Term::Var("?d".into());
    let dict = if via_rank {
        Term::Field(Rc::new(given), "$Describe".into())
    } else {
        given
    };
    let dict_ty = Type::Dict(Path::new(MODULE, class), Rc::from([a.clone()]));
    let body = Type::fun(dict_ty, Stage::Pure, Type::fun(a, Stage::Pure, string()));
    let call = Term::app(
        Term::Field(Rc::new(dict), "describe".into()),
        Term::Var("x".into()),
    );
    (
        Scheme::new(vec![("a".into(), Kind::Type)], body),
        Term::lam("?d", Term::lam("x", call)),
        false,
        true,
    )
}

/// The class a dictionary parameter asks of `v`, if one does.
pub(super) fn constrained<'p>(params: &'p [(Type, Stage)], v: &str) -> Option<&'p str> {
    params.iter().find_map(|(p, _)| match p {
        Type::Dict(c, args) if matches!(&args[..], [Type::Var(Var::Named(w))] if &**w == v) => {
            Some(&*c.name)
        }
        _ => None,
    })
}
