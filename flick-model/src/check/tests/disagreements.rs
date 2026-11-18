//! Rules that rejected programs flick-core accepts and both targets run:
//! each with what it now accepts and what it still rejects.

use std::rc::Rc;

use super::{T, a, decls, int, log, mono, poly, rejected_by, ty};
use crate::check::Checker;
use crate::decls::Class;
use crate::eval::{Machine, Outcome, Program};
use crate::syntax::{GETTER, Lit, Name, Path, Term};
use crate::types::{Caps, PROVIDED, Row, Stage, Type, Var, provided};

/// `{E₁, …, Eₙ | ?m}` of the errors of `decls`.
fn errors(c: &mut Checker, names: &[&str]) -> Row {
    let mut row = c.fresh_row();
    for n in names {
        row.elems.insert(Path::new(T, n), Rc::from([]));
    }
    row
}

/// `Errors {A | ?m} ≡ A` closes `?m`: no other row projects to `A`.
#[test]
fn an_error_row_with_an_unknown_tail_projects_to_its_error() {
    let d = decls();
    let mut c = Checker::new(&d);
    let row = errors(&mut c, &["A"]);
    c.unify(&Type::Errors(row.clone()), &Type::con(T, "A"))
        .expect("`Errors {A | ?m}` is `A`");
    let closed = c.zonk_row(&row);
    assert!(closed.vars.is_empty(), "{closed}");
    assert_eq!(
        closed.elems.keys().collect::<Vec<_>>(),
        [&Path::new(T, "A")]
    );
    let mut c = Checker::new(&d);
    let row = errors(&mut c, &["A"]);
    rejected_by(c.unify(&Type::Errors(row), &Type::con(T, "B")), "≡");
    let mut c = Checker::new(&d);
    let row = errors(&mut c, &["A", "B"]);
    rejected_by(c.unify(&Type::Errors(row), &Type::con(T, "A")), "≡");
}

/// The instance of a record or tuple is read under the bounds of its type.
#[test]
fn a_compiler_owned_instance_reads_through_bounds() {
    let d = decls();
    let show: Name = "Show".into();
    let dict = |t: Type| Type::Dict(Path::new("Prelude", "Show"), Rc::from([t]));
    let bounded = |t: Type| Type::Bounded(Rc::new(t), Caps::default());
    let unit = Type::Tuple(Rc::from([]));
    Checker::new(&d)
        .structural_fields(&show, &[], &dict(bounded(bounded(unit))))
        .expect("a tuple under two bounds");
    rejected_by(
        Checker::new(&d).structural_fields(&show, &[], &dict(bounded(ty("Int")))),
        "Structural",
    );
}

/// Two superclasses of one class are told apart by position.
#[test]
fn superclasses_of_one_class_have_numbered_slots() {
    let sup = |c: &str| Type::Dict(Path::new(T, c), Rc::from([]));
    let class = Class {
        params: Vec::new(),
        supers: vec![sup("Convert"), sup("Convert"), sup("Eq")],
        methods: Vec::new(),
    };
    let slots: Vec<_> = (0..4).map(|i| class.super_slot(i)).collect();
    assert_eq!(
        slots,
        [
            Some("Convert0".to_owned()),
            Some("Convert1".to_owned()),
            Some("Eq".to_owned()),
            None
        ]
    );
}

/// Work an owner keeps needs a provider that outlives it for each service
/// but `Async`, which only the root handles.
#[test]
fn kept_work_owes_no_provider_of_async() {
    let d = decls();
    let wanted = Row {
        elems: [
            (Path::new("Prelude", "Async"), Rc::from([])),
            (log(), Rc::from([])),
        ]
        .into(),
        ..Row::default()
    };
    let owner = Type::Var(Var::Named("s".into()));
    let row = Row {
        elems: [(provided(), Rc::from([Type::Tag(wanted), owner]))].into(),
        ..Row::default()
    };
    let split = Checker::new(&d).split_provided(&row).expect("split");
    let owed: Vec<_> = split.elems.keys().cloned().collect();
    assert_eq!(owed, [Path::new(PROVIDED, &log().to_string())]);
}

/// A default's field is a getter: checked by its body at the method's
/// type, and run where the field is read.
#[test]
fn a_default_is_a_getter_of_the_method_type() {
    let mut d = decls();
    let class = Path::new(T, "Counter");
    d.classes.insert(
        class.clone(),
        Class {
            params: vec!["a".into()],
            supers: Vec::new(),
            methods: vec!["start".into()],
        },
    );
    let dict = |t: Type| Type::Dict(class.clone(), Rc::from([t]));
    d.values.insert(
        Path::new(T, "start"),
        poly(Type::fun(dict(a()), Stage::Pure, a())),
    );
    let getter = |body| Term::Record(Rc::from([("start".into(), Term::lam(GETTER, body))]));
    Checker::new(&d)
        .check_top(&getter(int(1)), &mono(dict(ty("Int"))))
        .expect("a value default is a value");
    rejected_by(
        Checker::new(&d).check_top(&getter(Term::string("x")), &mono(dict(ty("Int")))),
        "≡",
    );
    let program = Program::default();
    let read = Term::Field(Rc::new(getter(int(1))), "start".into());
    let outcome = Machine::new(&program, 100).run(read);
    assert!(
        matches!(outcome, Outcome::Value(Term::Lit(Lit::Int(1)))),
        "{outcome:?}"
    );
}
