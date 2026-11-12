//! §1.4 under a `∀` of a signature: a result holds what the parameters
//! before it reach of the atoms outside the `∀`, never one of its binders
//! (`Ty.elide` of `.all`, `Caps.under` in `formal/Formal/Live/Elide.lean`).

use std::rc::Rc;

use super::Checker;
use crate::decls::Decls;
use crate::syntax::Path;
use crate::types::{Caps, Kind, Row, Scheme, Stage, Type, Var};

fn var(n: &str) -> Type {
    Type::Var(Var::Named(n.into()))
}

fn file(region: &str) -> Type {
    Type::app(Type::con("File", "File"), var(region))
}

/// `type G = G (Unit ->{…} Int)`: data with hidden storage.
fn g() -> Type {
    Type::con("T", "G")
}

fn decls() -> Decls {
    let mut d = Decls::default();
    let field = Type::fun(
        Type::con("Prelude", "Unit"),
        Stage::Runtime(Row::none(), Row::none()),
        Type::con("Prelude", "Int"),
    );
    let ctor = Scheme::new(Vec::new(), Type::fun(field, Stage::Pure, g()));
    d.constructors.insert(Path::new("T", "G"), ctor);
    d.variants
        .insert(Path::new("T", "G"), vec![("G".into(), 1)]);
    d
}

fn forall(vars: &[(&str, Kind)], body: Type) -> Type {
    let vars = vars.iter().map(|(n, k)| ((*n).into(), k.clone())).collect();
    Type::Forall(Rc::new(Scheme::new(vars, body)))
}

fn read(body: Type) -> Type {
    let d = decls();
    let s = Scheme::new(vec![("s".into(), Kind::Region)], body);
    Checker::new(&d).live_scheme(&s).body
}

fn held(t: Type, region: &str) -> Type {
    Type::Captured(Rc::new(t), Caps::atom(Var::Named(region.into())))
}

#[test]
fn under_a_forall_only_outer_atoms_are_held() {
    let int = || Type::con("Prelude", "Int");
    // `(forall t. File t -> G) -> File s -> Int`: `G` holds nothing of `t`
    let own = forall(
        &[("t", Kind::Region)],
        Type::fun(file("t"), Stage::Pure, g()),
    );
    let t = Type::fun(
        own.clone(),
        Stage::Pure,
        Type::fun(file("s"), Stage::Pure, int()),
    );
    assert_eq!(read(t.clone()), t);
    // `(forall a. File s -> a -> G) -> Int`: `G` holds `s`
    let outer = |g| {
        let inner = Type::fun(file("s"), Stage::Pure, Type::fun(var("a"), Stage::Pure, g));
        Type::fun(forall(&[("a", Kind::Type)], inner), Stage::Pure, int())
    };
    assert_eq!(read(outer(g())), outer(held(g(), "s")));
    // `File s -> (forall s. s -> G)`: the inner `s` is renamed, `G` holds the outer one
    let shadowed = Type::fun(
        file("s"),
        Stage::Pure,
        forall(&[("s", Kind::Type)], Type::fun(var("s"), Stage::Pure, g())),
    );
    let renamed = Type::fun(
        file("s"),
        Stage::Pure,
        forall(
            &[("s$0", Kind::Type)],
            Type::fun(var("s$0"), Stage::Pure, held(g(), "s")),
        ),
    );
    assert_eq!(read(shadowed), renamed);
}
