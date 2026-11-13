//! §1.4: a nominal type with hidden storage, an opaque ADT whose fields hide
//! captures or a foreign type marked `@stores`, reads a hidden row when a
//! signature writes it bare, as a declared ADT with such fields does.

use std::rc::Rc;

use super::Checker;
use crate::decls::Decls;
use crate::syntax::Path;
use crate::types::{Caps, Kind, Scheme, Stage, Type, Var};

#[test]
fn a_nominal_type_with_hidden_storage_reads_its_hidden_rows() {
    let codec = || Type::con("Prelude", "FieldCodec");
    let s = Scheme::new(Vec::new(), Type::fun(codec(), Stage::Pure, codec()));
    let mut d = Decls::default();
    // without constructors and unmarked, it reads no row
    assert_eq!(Checker::new(&d).live_scheme(&s).body, s.body);
    d.hidden.insert(Path::new("Prelude", "FieldCodec"));
    let read = Checker::new(&d).live_scheme(&s);
    let c = Var::Named("captures$0".into());
    assert_eq!(read.vars, [("captures$0".into(), Kind::Captures)]);
    let expected = Type::fun(
        Type::Captured(Rc::new(codec()), Caps::var(c.clone())),
        Stage::Pure,
        Type::Captured(Rc::new(codec()), Caps::var(c)),
    );
    assert_eq!(read.body, expected);
}
