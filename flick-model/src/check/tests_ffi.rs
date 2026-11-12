//! G4 (`FLICK-F009`): a pure foreign arrow over a region its result does not
//! carry uses the region with no stage that keeps it live.

use std::rc::Rc;

use super::pure_over_capability;
use crate::types::{Caps, Kind, Row, Scheme, Stage, Type, Var};

fn ty(name: &str) -> Type {
    Type::con("Prelude", name)
}

#[test]
fn a_pure_foreign_over_a_capability_uses_its_region() {
    let s = || Type::Var(Var::Named("s".into()));
    let file = || Type::app(Type::con("App", "File"), s());
    let region = |body| Scheme::new(vec![("s".into(), Kind::Region)], body);
    assert!(pure_over_capability(&region(Type::fun(
        file(),
        Stage::Pure,
        ty("Int")
    ))));
    // a stage keeps the region live where it is called
    let staged = Stage::Runtime(Row::none(), Row::none());
    assert!(!pure_over_capability(&region(Type::fun(
        file(),
        staged,
        ty("Int")
    ))));
    // a result that carries the region is no use of it
    assert!(!pure_over_capability(&region(Type::fun(
        file(),
        Stage::Pure,
        file()
    ))));
    // nor is a result that holds it in its qualifier
    let held = Type::Captured(Rc::new(ty("Int")), Caps::atom(Var::Named("s".into())));
    assert!(!pure_over_capability(&region(Type::fun(
        file(),
        Stage::Pure,
        held
    ))));
    // a capture row of a pure callback uses nothing
    let callback = Type::Captured(
        Rc::new(Type::fun(ty("Int"), Stage::Pure, ty("Int"))),
        Caps::var(Var::Named("c".into())),
    );
    let map = Scheme::new(
        vec![("c".into(), Kind::Captures)],
        Type::fun(callback, Stage::Pure, ty("Int")),
    );
    assert!(!pure_over_capability(&map));
}
