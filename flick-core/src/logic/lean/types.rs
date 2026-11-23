//! Types, named terms, and structural constructors for the `λPT` Lean replay.

use std::rc::Rc;

use crate::logic::term::Def;

/// A type of `λPT`, with unknowns while it is inferred.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum T {
    Meta(usize),
    /// A type variable: of the statement, or of a global's scheme.
    Var(usize),
    Unit,
    Prod(Box<T>, Box<T>),
    Sum(Box<T>, Box<T>),
    Arrow(Box<T>, Box<T>),
}

pub(super) fn arrow(a: T, b: T) -> T {
    T::Arrow(Box::new(a), Box::new(b))
}

/// The product of `fields`: `unit`, the field, or pairs nested to the right.
pub(super) fn product(fields: &[T]) -> T {
    match fields {
        [] => T::Unit,
        [f] => f.clone(),
        [f, rest @ ..] => T::Prod(Box::new(f.clone()), Box::new(product(rest))),
    }
}

/// The sum of the products of `ctors`, nested to the right.
pub(super) fn sum(ctors: &[Vec<T>]) -> T {
    match ctors {
        [] => T::Unit,
        [c] => product(c),
        [c, rest @ ..] => T::Sum(Box::new(product(c)), Box::new(sum(rest))),
    }
}

/// A term of `λPT` with names for its binders.
#[derive(Debug, Clone)]
pub(super) enum N {
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
pub(super) fn inject(i: usize, fields: Vec<N>, ctors: &[Vec<T>]) -> N {
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
pub(super) struct Global {
    pub(super) def: Rc<Def>,
    pub(super) body: N,
    pub(super) scheme: T,
    pub(super) vars: usize,
}

/// The constructors of a data type at its arguments: names and fields.
pub(super) type Ctors = Vec<(String, Vec<T>)>;

/// A constructor's branch of a `match`: its fields' names and types, and
/// the tree under them.
pub(super) type Branch = (Vec<(Rc<str>, T)>, N);

pub(super) fn fields_of(ctors: &Ctors) -> Vec<Vec<T>> {
    ctors.iter().map(|(_, f)| f.clone()).collect()
}

pub(super) fn collect_metas(t: &T, out: &mut Vec<usize>) {
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

pub(super) fn instantiate(t: &T, args: &[T]) -> T {
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
