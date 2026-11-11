//! Types, and terms by their type within a budget `φ`: every step a term
//! takes fits `φ`, so it has its type at that stage by the rules of §2.3.
//! A `let` only comes where the source has a block, so `print` can write
//! it. Parameters, patterns, cells and a `let` without a signature hold
//! data: a function there would be typed by the guess for a callee of unknown type, which the
//! rules do not make.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;

use flick_core::numeric::{Arith, Num};

use super::bind::fits;
use super::holes::{bases, js, js_value, shape, shape_value};
use super::regions::{handles, holders, visible};
use super::{
    ERRORS, Gen, SERVICES, boolean, error_con, int, mono, nothing, operation, option, payload, row,
    service, some, string, unit,
};
use crate::syntax::{Arm, Const, Lit, Name, Pat, Path, Prim, Term};
use crate::types::{Fields, Row, Scheme, Stage, Type};

/// `Γ` of the generator: names with their schemes, innermost last.
pub(super) type Cx = [(Name, Scheme)];

/// What a term of a type can be.
#[derive(Clone, Copy)]
enum Form {
    Leaf,
    Intro,
    Let,
    Statement,
    Cell,
    Match,
    Call,
    Await,
    Try,
    Raise,
    Provide,
    Operation,
    Get,
    Set,
    Prim,
    Field,
    Scope,
    Hold,
    Consume,
    Annotate,
    Update,
    Destructure,
    Lookup,
    Predicate,
    Member,
    FunctionCell,
    Method,
}

impl Gen<'_> {
    /// A type: data, or where `depth` allows an arrow or a computation.
    pub(super) fn ty(&mut self, depth: u32) -> Type {
        if depth == 0 {
            return self.base();
        }
        match self.pick(4) {
            0 | 1 => self.data(depth),
            2 => {
                let p = self.data(depth - 1);
                let s = self.stage();
                Type::fun(p, s, self.ty(depth - 1))
            }
            _ => {
                let (r, e) = (self.services(), self.errors());
                Type::Eff(r, e, Rc::new(self.ty(depth - 1)))
            }
        }
    }

    /// A type without arrows and computations.
    pub(super) fn data(&mut self, depth: u32) -> Type {
        if depth == 0 {
            return self.base();
        }
        match self.pick(5) {
            0 | 1 => self.base(),
            2 => option(self.data(depth - 1)),
            3 => Type::Tuple(Rc::from([self.data(depth - 1), self.data(depth - 1)])),
            _ => Type::Record(Fields {
                fields: BTreeMap::from([
                    (Name::from("a"), self.data(depth - 1)),
                    (Name::from("b"), self.data(depth - 1)),
                ]),
                tail: None,
            }),
        }
    }

    fn base(&mut self) -> Type {
        let bases = bases();
        bases[self.pick(bases.len())]()
    }

    pub(super) fn stage(&mut self) -> Stage {
        if self.pick(2) == 0 {
            Stage::Pure
        } else {
            Stage::Runtime(self.services(), self.errors())
        }
    }

    /// A row of the services of `Gen.fl`.
    pub(super) fn services(&mut self) -> Row {
        let bits = self.pick(4);
        let names: Vec<&str> = (0..2)
            .filter(|i| bits >> i & 1 == 1)
            .map(|i| SERVICES[i])
            .collect();
        row(&names)
    }

    /// A row of the errors of `Gen.fl`.
    pub(super) fn errors(&mut self) -> Row {
        let bits = self.pick(4);
        let names: Vec<&str> = (0..2)
            .filter(|i| bits >> i & 1 == 1)
            .map(|i| ERRORS[i])
            .collect();
        row(&names)
    }

    /// Some of the elements of `r`.
    pub(super) fn some_of(&mut self, r: &Row) -> Row {
        let mut out = Row::none();
        for (name, args) in &r.elems {
            if self.pick(2) == 1 {
                out.elems.insert(name.clone(), args.clone());
            }
        }
        out
    }

    /// A term of type `t` whose steps fit `budget`; `block`: the source
    /// has a block here.
    pub(super) fn term(
        &mut self,
        cx: &Cx,
        t: &Type,
        budget: &Stage,
        size: u32,
        block: bool,
    ) -> Term {
        if size == 0 {
            return self.leaf(cx, t);
        }
        let size = size - 1;
        let (services, errors) = match budget {
            Stage::Pure => (Row::none(), Row::none()),
            Stage::Runtime(r, e) => (r.clone(), e.clone()),
        };
        let has = |s: &str| services.elems.contains_key(&service(s));
        let forms = forms(cx, t, budget, block);
        let s = size;
        match forms[self.pick(forms.len())] {
            Form::Leaf => self.leaf(cx, t),
            Form::Intro => self.intro(cx, t, budget, s),
            Form::Let => self.let_(cx, t, budget, s),
            Form::Statement => {
                let st = self.term(cx, &unit(), budget, s, false);
                let rest = self.term(cx, t, budget, s, true);
                Term::Let(
                    "_".into(),
                    Some(Rc::new(mono(unit()))),
                    Rc::new(st),
                    Rc::new(rest),
                )
            }
            Form::Cell => {
                let c = self.data(1);
                let v = self.term(cx, &c, budget, s, false);
                let r = self.fresh("cell");
                let inner = extend(cx, &r, mono(Type::Ref(Rc::new(c))));
                let rest = self.term(&inner, t, budget, s, true);
                let make = Term::app(Term::konst(Const::RefMake), v);
                Term::Let(r, None, Rc::new(make), Rc::new(rest))
            }
            Form::Match => self.match_(cx, t, budget, s),
            Form::Call => self.call(cx, t, budget, s),
            Form::Await => {
                let (r, e) = (self.some_of(&services), self.some_of(&errors));
                let task = self.term(cx, &Type::Eff(r, e, Rc::new(t.clone())), budget, s, false);
                Term::Await(Rc::new(task))
            }
            Form::Try => self.try_(cx, t, &services, &errors, s),
            Form::Raise => {
                let names: Vec<&Path> = errors.elems.keys().collect();
                let e = names[self.pick(names.len())].name.clone();
                let v = self.term(cx, &payload(&e), budget, s, false);
                let error = Term::app(Term::konst_con(error_con(&e)), v);
                Term::app(Term::konst(Const::Raise), error)
            }
            Form::Provide => self.provide(cx, t, &services, &errors, s),
            Form::Operation if *t == unit() && has("Log") => {
                let line = self.term(cx, &string(), budget, s, false);
                Term::app(operation("Log", "log"), line)
            }
            Form::Operation => Term::app(operation("Counter", "next"), Term::unit()),
            Form::Get => {
                let found: Vec<Name> = cells(cx).filter(|(_, c)| *c == t).map(|(x, _)| x).collect();
                let x = found[self.pick(found.len())].clone();
                Term::app(Term::konst(Const::RefGet), Term::Var(x))
            }
            Form::Set => {
                let found: Vec<(Name, Type)> = cells(cx).map(|(x, c)| (x, c.clone())).collect();
                let (x, c) = found[self.pick(found.len())].clone();
                let v = self.term(cx, &c, budget, s, false);
                Term::apps(Term::konst(Const::RefSet), [Term::Var(x), v])
            }
            Form::Prim => self.prim(cx, t, budget, s),
            Form::Scope => self.scope(cx, t, budget, s),
            Form::Hold => self.hold(cx, t, budget, s),
            Form::Consume => self.consume(cx),
            Form::Annotate => self.annotate(cx, t, budget, s),
            Form::Update => self.update(cx, t, budget, s),
            Form::Destructure => self.destructure(cx, t, budget, s),
            Form::Lookup => match t {
                Type::App(_, a) => self.lookup(cx, a, budget, s),
                _ => self.leaf(cx, t),
            },
            Form::Predicate => self.predicate(cx, budget, s),
            Form::Member => self.member(cx, budget, s),
            Form::FunctionCell => self.function_cell(cx, t, budget, s),
            Form::Method => self.method(cx, t, budget, s),
            Form::Field => {
                let other = self.data(0);
                let fields =
                    BTreeMap::from([(Name::from("a"), t.clone()), (Name::from("b"), other)]);
                let rec = Type::Record(Fields { fields, tail: None });
                let r = self.term(cx, &rec, budget, s, false);
                Term::Field(Rc::new(r), "a".into())
            }
        }
    }

    /// A variable of type `t`, or a value without steps.
    pub(super) fn leaf(&mut self, cx: &Cx, t: &Type) -> Term {
        let vars: Vec<&Name> = cx
            .iter()
            .filter(|(_, s)| fits(&s.body, t, &mut HashMap::new()))
            .map(|(x, _)| x)
            .collect();
        if !vars.is_empty() && self.pick(2) == 1 {
            return Term::Var(vars[self.pick(vars.len())].clone());
        }
        self.value(t)
    }

    /// A value of `t` built of literals.
    fn value(&mut self, t: &Type) -> Term {
        match t {
            Type::Tuple(ts) => Term::Tuple(ts.iter().map(|t| self.value(t)).collect()),
            Type::Record(f) => Term::Record(
                f.fields
                    .iter()
                    .map(|(l, t)| (l.clone(), self.value(t)))
                    .collect(),
            ),
            Type::Fun(p, _, r) => {
                let x = self.fresh("x");
                let _ = p;
                Term::Lam(x, Rc::new(self.value(r)))
            }
            Type::Eff(_, _, a) => Term::Task(Rc::new(self.value(a))),
            Type::App(..) => Term::konst_con(nothing()),
            _ if *t == int() => Term::Lit(Lit::Int(i32::try_from(self.pick(3)).unwrap_or(0))),
            _ if *t == boolean() => Term::bool(self.pick(2) == 1),
            _ if *t == string() => Term::string(["", "a", "ok"][self.pick(3)]),
            _ if *t == js() => js_value(),
            _ if *t == shape() => shape_value(),
            _ => Term::unit(),
        }
    }

    /// The form that builds a value of `t`, with its parts generated.
    fn intro(&mut self, cx: &Cx, t: &Type, budget: &Stage, s: u32) -> Term {
        match t {
            Type::App(_, a) if self.pick(2) == 1 => {
                let v = self.term(cx, a, budget, s, false);
                Term::app(Term::konst_con(some()), v)
            }
            Type::Tuple(ts) => Term::Tuple(
                ts.iter()
                    .map(|t| self.term(cx, t, budget, s, false))
                    .collect(),
            ),
            Type::Record(f) => Term::Record(
                f.fields
                    .iter()
                    .map(|(l, t)| (l.clone(), self.term(cx, t, budget, s, false)))
                    .collect(),
            ),
            // a closure of a bare type holds no region (`regions.md` §2.3)
            Type::Fun(p, stage, r) => {
                let x = self.fresh("x");
                let inner = extend(&visible(cx, &BTreeSet::new()), &x, mono((**p).clone()));
                Term::Lam(x, Rc::new(self.term(&inner, r, stage, s, true)))
            }
            Type::Eff(r, e, a) => {
                let stage = Stage::Runtime(r.clone(), e.clone());
                let cx = visible(cx, &BTreeSet::new());
                Term::Task(Rc::new(self.term(&cx, a, &stage, s, true)))
            }
            _ => self.value(t),
        }
    }

    /// (Match) on data, its arms covering it; sometimes with a guard.
    fn match_(&mut self, cx: &Cx, t: &Type, budget: &Stage, s: u32) -> Term {
        let d = self.data(1);
        let scrutinee = self.term(cx, &d, budget, s, false);
        let guarded = self.one_in(3);
        let mut arms = Vec::new();
        let mut arm = |g: &mut Self, pat: Pat, bound: Vec<(Name, Type)>, guard: bool| {
            let mut inner = cx.to_vec();
            inner.extend(bound.into_iter().map(|(x, t)| (x, mono(t))));
            let guard = guard.then(|| g.term(&inner, &boolean(), budget, s, false));
            let body = g.term(&inner, t, budget, s, true);
            arms.push(Arm { pat, guard, body });
        };
        match &d {
            // with a guard, the arms after it cover the value but not the
            // type: the rest of the `match` is typed by (Match-Val)
            Type::App(_, a) if guarded => {
                let x = self.fresh("x");
                arm(self, Pat::Con(nothing(), Rc::from([])), Vec::new(), false);
                arm(
                    self,
                    Pat::Con(some(), Rc::from([Pat::Var(x.clone())])),
                    vec![(x, (**a).clone())],
                    true,
                );
                arm(
                    self,
                    Pat::Con(some(), Rc::from([Pat::Wild])),
                    Vec::new(),
                    false,
                );
            }
            Type::App(_, a) => {
                let x = self.fresh("x");
                arm(
                    self,
                    Pat::Con(some(), Rc::from([Pat::Var(x.clone())])),
                    vec![(x, (**a).clone())],
                    false,
                );
                arm(self, Pat::Con(nothing(), Rc::from([])), Vec::new(), false);
            }
            Type::Tuple(ts) => {
                let xs: Vec<Name> = ts.iter().map(|_| self.fresh("x")).collect();
                let pat = Pat::Tuple(xs.iter().map(|x| Pat::Var(x.clone())).collect());
                arm(
                    self,
                    pat,
                    xs.into_iter().zip(ts.iter().cloned()).collect(),
                    false,
                );
            }
            Type::Record(f) => {
                let x = self.fresh("x");
                let pat = Pat::Record(Rc::from([(Name::from("a"), Pat::Var(x.clone()))]));
                arm(self, pat, vec![(x, f.fields["a"].clone())], false);
            }
            _ if d == boolean() => {
                arm(self, Pat::Lit(Lit::Bool(true)), Vec::new(), guarded);
                arm(
                    self,
                    if guarded {
                        Pat::Wild
                    } else {
                        Pat::Lit(Lit::Bool(false))
                    },
                    Vec::new(),
                    false,
                );
            }
            _ if d == int() => {
                arm(self, Pat::Lit(Lit::Int(0)), Vec::new(), guarded);
                let n = self.fresh("n");
                arm(self, Pat::Var(n.clone()), vec![(n, int())], false);
            }
            _ if d == string() => {
                arm(
                    self,
                    Pat::Lit(Lit::String("ok".into())),
                    Vec::new(),
                    guarded,
                );
                arm(self, Pat::Wild, Vec::new(), false);
            }
            _ if [js(), shape()].contains(&d) => arm(self, Pat::Wild, Vec::new(), false),
            _ => arm(self, Pat::Lit(Lit::Unit), Vec::new(), false),
        }
        Term::Match(Rc::new(scrutinee), arms.into())
    }

    /// `+`, `-`, `<` on `Int` and `++` on `String`.
    fn prim(&mut self, cx: &Cx, t: &Type, budget: &Stage, s: u32) -> Term {
        let (op, operand) = if *t == int() {
            let a = if self.pick(2) == 0 {
                Arith::Add
            } else {
                Arith::Sub
            };
            (Prim::Arith(a, Num::Int), int())
        } else if *t == boolean() {
            (Prim::Lt, int())
        } else {
            (Prim::Concat, string())
        };
        let a = self.term(cx, &operand, budget, s, false);
        let b = self.term(cx, &operand, budget, s, false);
        Term::apps(Term::prim(op), [a, b])
    }
}

/// The forms a term of `t` may take within `budget`; `block`: the source
/// has a block here.
fn forms(cx: &Cx, t: &Type, budget: &Stage, block: bool) -> Vec<Form> {
    let (services, errors) = match budget {
        Stage::Pure => (Row::none(), Row::none()),
        Stage::Runtime(r, e) => (r.clone(), e.clone()),
    };
    let has = |s: &str| services.elems.contains_key(&service(s));
    let runtime = matches!(budget, Stage::Runtime(..));
    let mut forms = vec![Form::Leaf, Form::Intro, Form::Match, Form::Call];
    // data only where the type is inferred, not checked: under a
    // projection, and of the task `await` runs; a λ there takes its least
    // arrow (`docs/formal/core.md#a-callee-of-unknown-type-is-a-pure-arrow`), not the one the position wants
    if plain(t) {
        forms.push(Form::Field);
    }
    if plain(t) && runtime {
        forms.push(Form::Await);
    }
    if block {
        forms.extend([Form::Let, Form::Statement]);
    }
    if block && runtime {
        forms.push(Form::Cell);
    }
    if runtime {
        forms.extend([Form::Try, Form::Provide]);
    }
    if !errors.is_none() {
        forms.push(Form::Raise);
    }
    if (*t == unit() && has("Log")) || (*t == int() && has("Counter")) {
        forms.push(Form::Operation);
    }
    if runtime && cells(cx).any(|(_, c)| c == t) {
        forms.push(Form::Get);
    }
    if runtime && *t == unit() && cells(cx).next().is_some() {
        forms.push(Form::Set);
    }
    if runtime {
        forms.push(Form::Scope);
    }
    // a block in a `scope` often holds its capability, and uses what does
    if block && runtime && !handles(cx).is_empty() {
        forms.extend([Form::Hold; 3]);
    }
    if runtime && *t == unit() && !holders(cx).is_empty() {
        forms.extend([Form::Consume; 3]);
    }
    // the Flick forms of the holes of TypeScript (`holes.rs`)
    forms.push(Form::Annotate);
    if matches!(t, Type::Record(_)) {
        forms.push(Form::Update);
    }
    if matches!(t, Type::App(..)) && plain(t) {
        forms.push(Form::Lookup);
    }
    if block {
        forms.push(Form::Destructure);
    }
    if block && runtime {
        forms.push(Form::FunctionCell);
    }
    if *t == boolean() {
        forms.push(Form::Predicate);
    }
    if *t == string() && runtime {
        forms.push(Form::Member);
    }
    if [int(), boolean(), string()].contains(t) {
        forms.push(Form::Prim);
    }
    if [int(), string()].contains(t) {
        forms.push(Form::Method);
    }
    forms
}

pub(super) fn extend(cx: &Cx, x: &Name, s: Scheme) -> Vec<(Name, Scheme)> {
    let mut out = cx.to_vec();
    out.push((x.clone(), s));
    out
}

/// Whether `t` has no arrow and no computation in it.
pub(super) fn plain(t: &Type) -> bool {
    match t {
        Type::Fun(..) | Type::Eff(..) | Type::Ref(_) => false,
        Type::App(f, x) => plain(f) && plain(x),
        Type::Tuple(ts) => ts.iter().all(plain),
        Type::Record(f) => f.fields.values().all(plain),
        _ => true,
    }
}

/// The cells of `Γ` with the type they hold.
fn cells(cx: &Cx) -> impl Iterator<Item = (Name, &Type)> {
    // a cell of a capability is used by `regions.rs` only
    cx.iter().filter_map(|(x, s)| match &s.body {
        Type::Ref(c) if s.vars.is_empty() && plain_data(c) => Some((x.clone(), &**c)),
        _ => None,
    })
}

/// Whether `t` is data without variables: what `data` makes.
fn plain_data(t: &Type) -> bool {
    match t {
        Type::Var(_) => false,
        Type::App(f, x) => plain_data(f) && plain_data(x),
        Type::Tuple(ts) => ts.iter().all(plain_data),
        Type::Record(f) => f.fields.values().all(plain_data),
        _ => true,
    }
}
