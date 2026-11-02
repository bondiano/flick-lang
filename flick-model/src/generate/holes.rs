//! The Flick forms of the TypeScript holes (`docs/formal/ts-holes.md` §2):
//! a JS value of unknown type and an opaque JS object (2.1, 2.9, 2.10), a
//! foreign predicate (2.5), an annotation (2.2, 2.3), `{ r with ℓ = e }`
//! (2.9), a destructuring `let` (2.4), a cell of a function (2.7), a lookup
//! that gives an `Option` (2.13); and for `mutate`, the changes that would
//! open each hole: a cast, a field of an opaque value, a record with a
//! field more or less (2.8, 2.12), a stage widened in a parameter or a cell
//! (2.6, 2.7). Each is rejected by both checkers, or by neither.

use std::rc::Rc;

use super::term::{Cx, extend};
use super::{Gen, MODULE, int, mono, string};
use crate::syntax::{Arm, Const, Lit, Name, Pat, Path, Term};
use crate::types::{Row, Scheme, Stage, Type};

/// `JsValue`: a JS value of unknown type.
pub(super) fn js() -> Type {
    Type::con("Prelude", "JsValue")
}

/// `Shape`: a JS object as the importer gives it, opaque.
pub(super) fn shape() -> Type {
    Type::con(MODULE, "Shape")
}

fn unknown() -> Type {
    Type::con("Prelude", "Unknown")
}

/// The values of `Gen.fl` and the standard library these forms use.
pub(super) const GLOBALS: [&str; 5] = ["js_of", "shape_of", "is_point", "shape_name", "get"];

fn global(name: &str) -> Term {
    Term::Global(Path::new(MODULE, name))
}

/// `js_of "a"`: a pure foreign call.
pub(super) fn js_value() -> Term {
    Term::app(global("js_of"), Term::string("a"))
}

/// `shape_of (js_of "a")`.
pub(super) fn shape_value() -> Term {
    Term::app(global("shape_of"), js_value())
}

impl Gen<'_> {
    /// `(e : t)`, which is `let x : t = e in x` (`core.md` §1.4).
    pub(super) fn annotate(&mut self, cx: &Cx, t: &Type, budget: &Stage, size: u32) -> Term {
        let x = self.fresh("as");
        let e = self.term(cx, t, budget, size, false);
        Term::Let(
            x.clone(),
            Some(Rc::new(mono(t.clone()))),
            Rc::new(e),
            Rc::new(Term::Var(x)),
        )
    }

    /// `{ r with a = e }` of the record type `t`.
    pub(super) fn update(&mut self, cx: &Cx, t: &Type, budget: &Stage, size: u32) -> Term {
        let Type::Record(f) = t else {
            return self.leaf(cx, t);
        };
        // a leaf: `print` writes the record on the line of `with`
        let r = self.leaf(cx, t);
        let e = self.term(cx, &f.fields["a"], budget, size, false);
        Term::Update(Rc::new(r), "a".into(), Rc::new(e))
    }

    /// `let (x, y) = e` before `e' : t`: an irrefutable pattern.
    pub(super) fn destructure(&mut self, cx: &Cx, t: &Type, budget: &Stage, size: u32) -> Term {
        let (a, b) = (self.data(1), self.data(1));
        let pair = Type::Tuple(Rc::from([a.clone(), b.clone()]));
        let e = self.term(cx, &pair, budget, size, false);
        let (x, y) = (self.fresh("x"), self.fresh("x"));
        let inner = extend(&extend(cx, &x, mono(a)), &y, mono(b));
        let arm = Arm {
            pat: Pat::Tuple(Rc::from([Pat::Var(x), Pat::Var(y)])),
            guard: None,
            body: self.term(&inner, t, budget, size, true),
        };
        Term::Match(Rc::new(e), Rc::from([arm]))
    }

    /// `List.get 0 [e, e']` of `Option a`: a lookup has no default.
    pub(super) fn lookup(&mut self, cx: &Cx, a: &Type, budget: &Stage, size: u32) -> Term {
        let items = Term::List(Rc::from([
            self.term(cx, a, budget, size, false),
            self.term(cx, a, budget, size, false),
        ]));
        let index = Term::Lit(Lit::Int(i32::try_from(self.pick(3)).unwrap_or(0)));
        // `List` exposes the `get` of `List.Core`
        Term::apps(Term::Global(Path::new("List.Core", "get")), [index, items])
    }

    /// `is_point v`: what a predicate says changes no type.
    pub(super) fn predicate(&mut self, cx: &Cx, budget: &Stage, size: u32) -> Term {
        Term::app(
            global("is_point"),
            self.term(cx, &js(), budget, size, false),
        )
    }

    /// `shape_name o`: a member of a JS object is a call, not a field.
    pub(super) fn member(&mut self, cx: &Cx, budget: &Stage, size: u32) -> Term {
        Term::app(
            global("shape_name"),
            self.term(cx, &shape(), budget, size, false),
        )
    }

    /// `let c : Ref (a -> b) = Ref.make (f : a -> b)` before `e : t`.
    /// `Ref.make` takes the function at a variable's type, where a λ takes
    /// its least stage (`docs/formal/core.md#a-callee-of-unknown-type-is-a-pure-arrow`): the annotation types it while it is checked,
    /// but a state of the machine has the λ without it, so the stage is
    /// the least, `P`.
    pub(super) fn function_cell(&mut self, cx: &Cx, t: &Type, budget: &Stage, size: u32) -> Term {
        let f = Type::fun(self.data(1), Stage::Pure, self.data(1));
        let lam = self.annotate(cx, &f, budget, size);
        let c = self.fresh("cell");
        let cell = Type::Ref(Rc::new(f));
        let rest = self.term(&extend(cx, &c, mono(cell.clone())), t, budget, size, true);
        let make = Term::app(Term::konst(Const::RefMake), lam);
        Term::Let(c, Some(Rc::new(mono(cell))), Rc::new(make), Rc::new(rest))
    }
}

/// `(e : t)` with `t` of `g`, or `((e : Unknown) : t)` (2.2, 2.3).
pub(super) fn cast(g: &mut Gen, e: &Term) -> Term {
    let t = g.ty(1);
    let annotate = |e: Term, t: Type, x: Name| {
        Term::Let(
            x.clone(),
            Some(Rc::new(mono(t))),
            Rc::new(e),
            Rc::new(Term::Var(x)),
        )
    };
    let e = if g.pick(2) == 0 {
        e.clone()
    } else {
        annotate(e.clone(), unknown(), g.fresh("as"))
    };
    annotate(e, t, g.fresh("as"))
}

/// A field of what has none: of `Shape` or `JsValue`, read or replaced
/// (2.5, 2.9, 2.10).
pub(super) fn opaque_field(g: &mut Gen) -> Term {
    let value = if g.pick(2) == 0 {
        shape_value()
    } else {
        js_value()
    };
    if g.pick(2) == 0 {
        Term::Field(Rc::new(value), "name".into())
    } else {
        Term::Update(Rc::new(value), "name".into(), Rc::new(Term::string("x")))
    }
}

/// A record literal with a field more, or one fewer (2.8, 2.12).
pub(super) fn reshape(g: &mut Gen, fields: &[(Name, Term)]) -> Term {
    let mut fields = fields.to_vec();
    if fields.len() > 1 && g.pick(2) == 0 {
        fields.remove(g.pick(fields.len()));
    } else {
        fields.push(("c".into(), Term::Lit(Lit::Int(0))));
    }
    Term::Record(fields.into())
}

/// `σ` with the first pure arrow in a parameter or a cell made a runtime
/// one (2.6, 2.7).
pub(super) fn widen(s: &Scheme) -> Option<Scheme> {
    Some(Scheme {
        body: widen_type(&s.body, false)?,
        ..s.clone()
    })
}

/// `under`: `t` is in a parameter or a cell, where a wider stage is
/// another type.
fn widen_type(t: &Type, under: bool) -> Option<Type> {
    match t {
        Type::Fun(p, Stage::Pure, r) if under => Some(Type::Fun(
            p.clone(),
            Stage::Runtime(Row::none(), Row::none()),
            r.clone(),
        )),
        Type::Fun(p, s, r) => match widen_type(p, true) {
            Some(p) => Some(Type::Fun(Rc::new(p), s.clone(), r.clone())),
            None => Some(Type::Fun(
                p.clone(),
                s.clone(),
                Rc::new(widen_type(r, under)?),
            )),
        },
        Type::Ref(x) => Some(Type::Ref(Rc::new(widen_type(x, true)?))),
        _ => None,
    }
}

/// The types of the holes in `base`.
pub(super) fn bases() -> [fn() -> Type; 6] {
    [int, super::boolean, string, super::unit, js, shape]
}
