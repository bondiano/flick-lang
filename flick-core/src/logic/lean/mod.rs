//! The proofs of a module as a program and statements of `λPT`, the calculus
//! `formal/Formal/Proofs` mechanizes, written as Lean source for Lean to
//! check again (`just formal-proofs`). Only that test builds it: the
//! compiler ships none of it.
//!
//! A data type is the sum of its constructors, each the product of its
//! fields: `Option a` is `unit + a`, and `Bool` is `unit + unit`. The
//! annotations `λPT`'s checker reads (a binder's type, the other side of an
//! injection, the instance of a global) are inferred here by unification;
//! Lean checks every one again, so a wrong one only fails the replay. A
//! `match` becomes a tree of `case` and `split`, first arm first.

mod emit;

pub use emit::lean_replay;

use std::rc::Rc;

use flick_ir::Lit;

use super::term::{Arm, Ctor, Def, Pat, Term};
use super::{Data, TUPLES, Ty};

/// A type of `λPT`, with unknowns while it is inferred.
#[derive(Debug, Clone, PartialEq)]
enum T {
    Meta(usize),
    /// A type variable: of the statement, or of a global's scheme.
    Var(usize),
    Unit,
    Prod(Box<T>, Box<T>),
    Sum(Box<T>, Box<T>),
    Arrow(Box<T>, Box<T>),
}

fn arrow(a: T, b: T) -> T {
    T::Arrow(Box::new(a), Box::new(b))
}

/// The product of `fields`: `unit`, the field, or pairs nested to the right.
fn product(fields: &[T]) -> T {
    match fields {
        [] => T::Unit,
        [f] => f.clone(),
        [f, rest @ ..] => T::Prod(Box::new(f.clone()), Box::new(product(rest))),
    }
}

/// The sum of the products of `ctors`, nested to the right.
fn sum(ctors: &[Vec<T>]) -> T {
    match ctors {
        [] => T::Unit,
        [c] => product(c),
        [c, rest @ ..] => T::Sum(Box::new(product(c)), Box::new(sum(rest))),
    }
}

/// A term of `λPT` with names for its binders.
#[derive(Debug, Clone)]
enum N {
    Var(Rc<str>),
    Glob(usize, Vec<T>),
    Lam(Rc<str>, T, Box<N>),
    App(Box<N>, Box<N>),
    Unit,
    Pair(Box<N>, Box<N>),
    Split(Box<N>, Rc<str>, Rc<str>, Box<N>),
    Inl(Box<N>, T),
    Inr(T, Box<N>),
    Case(Box<N>, Rc<str>, Box<N>, Rc<str>, Box<N>),
    Let(Rc<str>, Box<N>, Box<N>),
}

/// The value of constructor `i` of `ctors` with the payload `fields`.
fn inject(i: usize, fields: Vec<N>, ctors: &[Vec<T>]) -> N {
    fn payload(mut fields: Vec<N>) -> N {
        match fields.len() {
            0 => N::Unit,
            1 => fields.remove(0),
            _ => {
                let first = fields.remove(0);
                N::Pair(Box::new(first), Box::new(payload(fields)))
            }
        }
    }
    if ctors.len() <= 1 {
        return payload(fields);
    }
    if i == 0 {
        N::Inl(Box::new(payload(fields)), sum(&ctors[1..]))
    } else {
        N::Inr(
            product(&ctors[0]),
            Box::new(inject(i - 1, fields, &ctors[1..])),
        )
    }
}

/// A global: a definition with its body and scheme over `vars` variables.
struct Global {
    def: Rc<Def>,
    body: N,
    scheme: T,
    vars: usize,
}

/// The constructors of a data type at its arguments: names and fields.
type Ctors = Vec<(String, Vec<T>)>;

/// A constructor's branch of a `match`: its fields' names and types, and
/// the tree under them.
type Branch = (Vec<(Rc<str>, T)>, N);

struct Tx<'a> {
    data: &'a Data,
    metas: Vec<Option<T>>,
    names: usize,
    globals: Vec<Global>,
    /// The type variables of the statement being read, by name.
    rigid: Vec<String>,
}

impl<'a> Tx<'a> {
    fn new(data: &'a Data) -> Self {
        Tx {
            data,
            metas: Vec::new(),
            names: 0,
            globals: Vec::new(),
            rigid: Vec::new(),
        }
    }

    fn meta(&mut self) -> T {
        self.metas.push(None);
        T::Meta(self.metas.len() - 1)
    }

    fn fresh(&mut self) -> Rc<str> {
        self.names += 1;
        format!("%{}", self.names).into()
    }

    fn zonk(&self, t: &T) -> T {
        match t {
            T::Meta(m) => match &self.metas[*m] {
                Some(t) => self.zonk(t),
                None => t.clone(),
            },
            T::Var(_) | T::Unit => t.clone(),
            T::Prod(a, b) => T::Prod(Box::new(self.zonk(a)), Box::new(self.zonk(b))),
            T::Sum(a, b) => T::Sum(Box::new(self.zonk(a)), Box::new(self.zonk(b))),
            T::Arrow(a, b) => arrow(self.zonk(a), self.zonk(b)),
        }
    }

    fn occurs(&self, m: usize, t: &T) -> bool {
        match self.zonk(t) {
            T::Meta(n) => n == m,
            T::Var(_) | T::Unit => false,
            T::Prod(a, b) | T::Sum(a, b) | T::Arrow(a, b) => {
                self.occurs(m, &a) || self.occurs(m, &b)
            }
        }
    }

    fn unify(&mut self, a: &T, b: &T) -> Result<(), String> {
        match (self.zonk(a), self.zonk(b)) {
            (T::Meta(m), T::Meta(n)) if m == n => Ok(()),
            (T::Meta(m), t) | (t, T::Meta(m)) => {
                if self.occurs(m, &t) {
                    return Err("a type would contain itself".into());
                }
                self.metas[m] = Some(t);
                Ok(())
            }
            (T::Var(x), T::Var(y)) if x == y => Ok(()),
            (T::Unit, T::Unit) => Ok(()),
            (T::Prod(a, b), T::Prod(c, d))
            | (T::Sum(a, b), T::Sum(c, d))
            | (T::Arrow(a, b), T::Arrow(c, d)) => {
                self.unify(&a, &c)?;
                self.unify(&b, &d)
            }
            (a, b) => Err(format!("{a:?} is not {b:?}")),
        }
    }

    /// A type of the logic, its variables `params` read as `args`.
    fn ty_in(&mut self, t: &Ty, params: &[String], args: &[T]) -> Result<T, String> {
        Ok(match t {
            Ty::Rigid(x) => match params.iter().position(|p| p == x) {
                Some(i) => args[i].clone(),
                None => T::Var(self.rigid_index(x)),
            },
            Ty::Bool => T::Sum(Box::new(T::Unit), Box::new(T::Unit)),
            Ty::Fun(a, b) => arrow(self.ty_in(a, params, args)?, self.ty_in(b, params, args)?),
            Ty::Data {
                module,
                name,
                args: own,
            } => {
                let own: Vec<T> = (own.iter())
                    .map(|a| self.ty_in(a, params, args))
                    .collect::<Result<_, _>>()?;
                let ctors = self.ctors(module, name, &own)?;
                sum(&fields_of(&ctors))
            }
            Ty::Refused(why) => return Err(why.clone()),
        })
    }

    fn ty(&mut self, t: &Ty) -> Result<T, String> {
        self.ty_in(t, &[], &[])
    }

    fn rigid_index(&mut self, x: &str) -> usize {
        if let Some(i) = self.rigid.iter().position(|r| r == x) {
            return i;
        }
        self.rigid.push(x.to_owned());
        self.rigid.len() - 1
    }

    /// The constructors of `module.name` at `args`, in declaration order.
    fn ctors(&mut self, module: &str, name: &str, args: &[T]) -> Result<Ctors, String> {
        if module == TUPLES {
            return Ok(vec![("(,)".into(), args.to_vec())]);
        }
        let key = (module.to_owned(), name.to_owned());
        let decl = (self.data.get(&key))
            .ok_or_else(|| format!("the data type `{name}` is not among the proof's"))?
            .clone();
        let mut out = Vec::new();
        for (ctor, fields) in &decl.constructors {
            let fields = (fields.iter())
                .map(|f| self.ty_in(f, &decl.params, args))
                .collect::<Result<_, _>>()?;
            out.push((ctor.clone(), fields));
        }
        Ok(out)
    }

    /// Constructor `c`: its index, every constructor of its type with its
    /// fields at new unknowns, and the type.
    fn ctor(&mut self, c: &Ctor) -> Result<(usize, Ctors, T), String> {
        let params = if c.module == TUPLES {
            c.arity
        } else {
            let key = (c.module.clone(), c.ty.clone());
            (self.data.get(&key))
                .ok_or_else(|| format!("the data type `{}` is not among the proof's", c.ty))?
                .params
                .len()
        };
        let args: Vec<T> = (0..params).map(|_| self.meta()).collect();
        let ctors = self.ctors(&c.module, &c.ty, &args)?;
        let index = (ctors.iter())
            .position(|(n, _)| *n == c.name)
            .ok_or_else(|| format!("`{}` has no constructor `{}`", c.ty, c.name))?;
        let ty = sum(&fields_of(&ctors));
        Ok((index, ctors, ty))
    }

    /// The term `t` with its type, the names of `env` in scope.
    fn term(&mut self, t: &Term, env: &mut Vec<(Rc<str>, T)>) -> Result<(N, T), String> {
        match t {
            Term::Var(x) => {
                let (_, ty) = (env.iter().rev())
                    .find(|(n, _)| n == x)
                    .ok_or_else(|| format!("`{x}` is not bound"))?;
                Ok((N::Var(x.clone()), ty.clone()))
            }
            Term::Lit(l) => match &l.0 {
                Lit::Bool(b) => Ok((
                    inject(usize::from(*b), Vec::new(), &[Vec::new(), Vec::new()]),
                    sum(&[Vec::new(), Vec::new()]),
                )),
                Lit::Unit => Ok((N::Unit, T::Unit)),
                _ => Err("a literal other than a `Bool` is outside the domain".into()),
            },
            Term::Lam(x, b) => {
                let a = self.meta();
                env.push((x.clone(), a.clone()));
                let body = self.term(b, env);
                env.pop();
                let (nb, tb) = body?;
                Ok((N::Lam(x.clone(), a.clone(), Box::new(nb)), arrow(a, tb)))
            }
            Term::App(..) | Term::Con(_) => self.application(t, env),
            Term::Match(s, arms) => self.matching(s, arms, env),
            Term::Let(x, e, b) => {
                let (ne, te) = self.term(e, env)?;
                env.push((x.clone(), te));
                let body = self.term(b, env);
                env.pop();
                let (nb, tb) = body?;
                Ok((N::Let(x.clone(), Box::new(ne), Box::new(nb)), tb))
            }
            Term::Def(d) => {
                let g = (self.globals.iter())
                    .position(|g| *g.def == **d)
                    .ok_or_else(|| format!("`{}` is not a global of the program", d.shown()))?;
                let args: Vec<T> = (0..self.globals[g].vars).map(|_| self.meta()).collect();
                let ty = instantiate(&self.globals[g].scheme, &args);
                Ok((N::Glob(g, args), ty))
            }
        }
    }

    /// An application, or a constructor: saturated, or each missing field a
    /// parameter of a function.
    fn application(
        &mut self,
        applied: &Term,
        env: &mut Vec<(Rc<str>, T)>,
    ) -> Result<(N, T), String> {
        let mut args = Vec::new();
        let mut head = applied;
        while let Term::App(f, x) = head {
            args.push(x.as_ref());
            head = f;
        }
        args.reverse();
        let Term::Con(c) = head else {
            let Term::App(f, x) = applied else {
                unreachable!("a constructor is the other head")
            };
            let (nf, tf) = self.term(f, env)?;
            let (nx, tx) = self.term(x, env)?;
            let r = self.meta();
            self.unify(&tf, &arrow(tx, r.clone()))?;
            return Ok((N::App(Box::new(nf), Box::new(nx)), r));
        };
        if args.len() > c.arity {
            return Err(format!("`{}` is given more than its fields", c.name));
        }
        let (index, ctors, ty) = self.ctor(c)?;
        let ctors = fields_of(&ctors);
        let mut fields = Vec::new();
        for (x, ft) in args.iter().zip(&ctors[index]) {
            let (nx, tx) = self.term(x, env)?;
            self.unify(&tx, ft)?;
            fields.push(nx);
        }
        // the fields not given are parameters
        let missing: Vec<(Rc<str>, T)> = (ctors[index][args.len()..].iter())
            .map(|ft| (self.fresh(), ft.clone()))
            .collect();
        fields.extend(missing.iter().map(|(n, _)| N::Var(n.clone())));
        let mut out = inject(index, fields, &ctors);
        let mut out_ty = ty;
        for (n, ft) in missing.into_iter().rev() {
            out = N::Lam(n, ft.clone(), Box::new(out));
            out_ty = arrow(ft, out_ty);
        }
        Ok((out, out_ty))
    }

    fn matching(
        &mut self,
        s: &Term,
        arms: &[Arm],
        env: &mut Vec<(Rc<str>, T)>,
    ) -> Result<(N, T), String> {
        let (ns, ts) = self.term(s, env)?;
        let x = self.fresh();
        let result = self.meta();
        let rows: Vec<Row> = (arms.iter())
            .map(|a| Row {
                pats: vec![a.pat.clone()],
                binds: Vec::new(),
                body: &a.body,
            })
            .collect();
        env.push((x.clone(), ts.clone()));
        let body = self.compile(&[(x.clone(), ts)], &rows, &result, env);
        env.pop();
        Ok((N::Let(x, Box::new(ns), Box::new(body?)), result))
    }

    /// A pattern matrix as a tree of `case` and `split`, first row first.
    fn compile(
        &mut self,
        cols: &[(Rc<str>, T)],
        rows: &[Row],
        result: &T,
        env: &mut Vec<(Rc<str>, T)>,
    ) -> Result<N, String> {
        let Some(first) = rows.first() else {
            return Err("a `match` misses a case".into());
        };
        let Some(j) = first
            .pats
            .iter()
            .position(|p| matches!(p, Pat::Con(..) | Pat::Lit(_)))
        else {
            // every pattern of the first row matches: its names, then its body
            let mut binds = first.binds.clone();
            for (p, (col, _)) in first.pats.iter().zip(cols) {
                if let Pat::Bind(y) = p {
                    binds.push((y.clone(), col.clone()));
                }
            }
            let mark = env.len();
            for (y, col) in &binds {
                let ty = (cols.iter())
                    .find(|(c, _)| c == col)
                    .map(|(_, t)| t.clone());
                let ty = ty.or_else(|| {
                    env.iter()
                        .rev()
                        .find(|(n, _)| n == col)
                        .map(|(_, t)| t.clone())
                });
                env.push((y.clone(), ty.ok_or("a pattern binds an unknown column")?));
            }
            let body = self.term(first.body, env);
            env.truncate(mark);
            let (mut nb, tb) = body?;
            self.unify(&tb, result)?;
            for (y, col) in binds.into_iter().rev() {
                nb = N::Let(y, Box::new(N::Var(col)), Box::new(nb));
            }
            return Ok(nb);
        };
        // a `Bool` is matched by its literals, `false` first
        let ctors: Ctors = if let Pat::Con(c, _) = &first.pats[j] {
            let (_, ctors, ty) = self.ctor(c)?;
            self.unify(&ty, &cols[j].1)?;
            ctors
        } else {
            self.unify(&sum(&[Vec::new(), Vec::new()]), &cols[j].1)?;
            vec![("false".into(), Vec::new()), ("true".into(), Vec::new())]
        };
        let mut branches = Vec::new();
        for (i, (name, fields)) in ctors.iter().enumerate() {
            let names: Vec<(Rc<str>, T)> =
                fields.iter().map(|f| (self.fresh(), f.clone())).collect();
            let mut sub = Vec::new();
            for row in rows {
                if let Some(row) = specialize(row, j, (i, name), fields.len(), &cols[j].0) {
                    sub.push(row);
                }
            }
            let mut sub_cols = cols[..j].to_vec();
            sub_cols.extend(names.iter().cloned());
            sub_cols.extend(cols[j + 1..].iter().cloned());
            let mark = env.len();
            env.extend(names.iter().cloned());
            let tree = self.compile(&sub_cols, &sub, result, env);
            env.truncate(mark);
            branches.push((names, tree?));
        }
        Ok(self.destruct(N::Var(cols[j].0.clone()), branches))
    }

    /// The branch of each constructor of a value: a `case` for each
    /// constructor but the last, then a `split` of the fields.
    fn destruct(&mut self, value: N, mut branches: Vec<Branch>) -> N {
        if branches.len() == 1 {
            let (names, body) = branches.remove(0);
            return self.fields(value, names, body);
        }
        let (names, body) = branches.remove(0);
        let left = self.fresh();
        let right = self.fresh();
        let left_body = self.fields(N::Var(left.clone()), names, body);
        let right_body = self.destruct(N::Var(right.clone()), branches);
        N::Case(
            Box::new(value),
            left,
            Box::new(left_body),
            right,
            Box::new(right_body),
        )
    }

    /// `body` with `names` bound to the fields of the product `value`.
    fn fields(&mut self, value: N, mut names: Vec<(Rc<str>, T)>, body: N) -> N {
        match names.len() {
            0 => body,
            1 => N::Let(names.remove(0).0, Box::new(value), Box::new(body)),
            _ => {
                let (first, _) = names.remove(0);
                let rest = self.fresh();
                let inner = self.fields(N::Var(rest.clone()), names, body);
                N::Split(Box::new(value), first, rest, Box::new(inner))
            }
        }
    }

    /// A definition as the next global, its scheme its type generalized.
    fn global(&mut self, def: Rc<Def>, body: &Term) -> Result<(), String> {
        self.rigid.clear();
        let (n, t) = self.term(body, &mut Vec::new())?;
        let t = self.zonk(&t);
        let mut vars = Vec::new();
        collect_metas(&t, &mut vars);
        for (i, m) in vars.iter().enumerate() {
            self.metas[*m] = Some(T::Var(i));
        }
        let scheme = self.zonk(&t);
        self.globals.push(Global {
            def,
            body: n,
            scheme,
            vars: vars.len(),
        });
        Ok(())
    }
}

/// A row of a pattern matrix: the patterns left, the names bound so far to
/// columns, and the body.
#[derive(Clone)]
struct Row<'t> {
    pats: Vec<Pat>,
    binds: Vec<(Rc<str>, Rc<str>)>,
    body: &'t Term,
}

/// `row` when column `j` is constructor `(i, name)` with `arity` fields, or
/// `None` when its pattern is another constructor.
fn specialize<'t>(
    row: &Row<'t>,
    j: usize,
    (i, name): (usize, &str),
    arity: usize,
    col: &Rc<str>,
) -> Option<Row<'t>> {
    let mut row = row.clone();
    let fields = match row.pats[j].clone() {
        Pat::Con(c, sub) if c.name == name => sub,
        Pat::Con(..) => return None,
        Pat::Lit(l) => match l.0 {
            Lit::Bool(b) if usize::from(b) == i => Vec::new(),
            _ => return None,
        },
        Pat::Bind(y) => {
            row.binds.push((y, col.clone()));
            vec![Pat::Wild; arity]
        }
        Pat::Wild => vec![Pat::Wild; arity],
    };
    row.pats.splice(j..=j, fields);
    Some(row)
}

fn fields_of(ctors: &Ctors) -> Vec<Vec<T>> {
    ctors.iter().map(|(_, f)| f.clone()).collect()
}

fn collect_metas(t: &T, out: &mut Vec<usize>) {
    match t {
        T::Meta(m) => {
            if !out.contains(m) {
                out.push(*m);
            }
        }
        T::Var(_) | T::Unit => {}
        T::Prod(a, b) | T::Sum(a, b) | T::Arrow(a, b) => {
            collect_metas(a, out);
            collect_metas(b, out);
        }
    }
}

fn instantiate(t: &T, args: &[T]) -> T {
    match t {
        T::Var(i) => args.get(*i).cloned().unwrap_or(T::Unit),
        T::Meta(_) | T::Unit => t.clone(),
        T::Prod(a, b) => T::Prod(
            Box::new(instantiate(a, args)),
            Box::new(instantiate(b, args)),
        ),
        T::Sum(a, b) => T::Sum(
            Box::new(instantiate(a, args)),
            Box::new(instantiate(b, args)),
        ),
        T::Arrow(a, b) => arrow(instantiate(a, args), instantiate(b, args)),
    }
}
