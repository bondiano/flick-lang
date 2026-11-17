//! What a pattern tests of its place, and how two tests of one place
//! relate: the same test, or two no value passes both.

use super::{Step, Test};
use crate::ir::{EnumLit, Lit, Pat, Rep};

/// What `p` tests of its place, or `None` when it accepts anything.
pub(super) fn test(p: &Pat) -> Option<Test<'_>> {
    match p {
        Pat::Con(c, _) => Some(match &c.rep {
            Rep::Typeof(_) => Test::Class(c),
            Rep::Literal(_) => Test::Enum(c),
            Rep::Tagged | Rep::Unboxed => Test::Tag(c),
        }),
        Pat::Lit(lit) => Some(Test::Lit(lit)),
        Pat::List(items, rest) => Some(Test::Len {
            n: items.len(),
            rest: rest.is_some(),
        }),
        _ => None,
    }
}

/// The patterns of the parts of what `p` tests.
pub(super) fn parts(p: &Pat) -> Vec<&Pat> {
    let mut out = Vec::new();
    match p {
        Pat::Con(_, args) => out.extend(args),
        Pat::List(items, rest) => {
            out.extend(items);
            out.extend(rest.as_deref());
        }
        _ => {}
    }
    out
}

/// Whether two tests of one place are the same; the constructors of a
/// place are of one type, so their names tell them apart.
pub(super) fn same(a: Test<'_>, b: Test<'_>) -> bool {
    match (a, b) {
        (Test::Tag(x), Test::Tag(y))
        | (Test::Class(x), Test::Class(y))
        | (Test::Enum(x), Test::Enum(y)) => x.name == y.name,
        (Test::Lit(x), Test::Lit(y)) => x == y,
        (Test::Len { n: x, rest: rx }, Test::Len { n: y, rest: ry }) => x == y && rx == ry,
        _ => false,
    }
}

pub(super) fn same_step(one: Step<'_>, other: Step<'_>) -> bool {
    match (one, other) {
        (Step::Field(c, at), Step::Field(d, to)) => c.name == d.name && at == to,
        (Step::Item(at), Step::Item(to)) | (Step::Rest(at), Step::Rest(to)) => at == to,
        (Step::Label(l), Step::Label(m)) => l == m,
        _ => false,
    }
}

/// Whether no value passes both tests, two different ones.
pub(super) fn exclusive(one: Test<'_>, other: Test<'_>) -> bool {
    match (one, other) {
        (Test::Tag(c), Test::Tag(d)) => c.name != d.name,
        (Test::Enum(c), Test::Enum(d)) => match (&c.rep, &d.rep) {
            (Rep::Literal(EnumLit::Int(m)), Rep::Literal(EnumLit::Int(n))) => m != n,
            (Rep::Literal(EnumLit::String(m)), Rep::Literal(EnumLit::String(n))) => m != n,
            _ => false,
        },
        (Test::Class(c), Test::Class(d)) => match (&c.rep, &d.rep) {
            (Rep::Typeof(m), Rep::Typeof(n)) => m != n,
            _ => false,
        },
        // a `Float` `0.0` is `-0.0` to a test, and two `BigInt` digits may be one number
        (Test::Lit(l), Test::Lit(m)) => match (l, m) {
            (Lit::Int(i), Lit::Int(j)) => i != j,
            (Lit::String(i), Lit::String(j)) => i != j,
            (Lit::Bool(i), Lit::Bool(j)) => i != j,
            _ => false,
        },
        (Test::Len { n, rest: at_least }, Test::Len { n: m, rest }) => match (at_least, rest) {
            (false, false) => n != m,
            (true, false) => m < n,
            (false, true) => n < m,
            (true, true) => false,
        },
        _ => false,
    }
}
