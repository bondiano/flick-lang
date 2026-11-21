//! Rules that rejected programs flick-core accepts and both targets run:
//! each with what it now accepts and what it still rejects.

use std::rc::Rc;

use super::{T, a, decls, int, log, mono, poly, rejected_by, ty};
use crate::check::Checker;
use crate::decls::{Class, Decls};
use crate::eval::{Machine, Outcome, Program};
use crate::syntax::{Arm, Con, GETTER, Head, Lit, Name, Pat, Path, Term};
use crate::types::{Caps, Fields, Kind, PROVIDED, Row, Scheme, Stage, Type, Var, provided};

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

fn scope_value_bound_decls() -> Decls {
    let mut d = decls();
    let region_s = Var::Named("s".into());
    let scope_s = || {
        let mut r = Row::none();
        r.elems.insert(
            Path::new("Prelude", "Scope"),
            Rc::from([Type::Var(region_s.clone())]),
        );
        Stage::Runtime(r, Row::none())
    };
    let option_path = Path::new("Prelude", "Option");
    d.variants.insert(
        option_path.clone(),
        vec![("None".into(), 0), ("Some".into(), 1)],
    );
    d.constructors.insert(
        Path::new("Prelude", "None"),
        poly(Type::app(Type::Con(option_path.clone()), a())),
    );
    d.constructors.insert(
        Path::new("Prelude", "Some"),
        poly(Type::fun(
            a(),
            Stage::Pure,
            Type::app(Type::Con(option_path.clone()), a()),
        )),
    );
    let bounded_of = |inner: Type| {
        Scheme::new(
            vec![("s".into(), Kind::Region)],
            Type::fun(
                ty("Unit"),
                scope_s(),
                Type::Bounded(Rc::new(inner), Caps::atom(region_s.clone())),
            ),
        )
    };
    d.values
        .insert(Path::new(T, "get_str"), bounded_of(ty("String")));
    d.values.insert(
        Path::new(T, "get_opt_str"),
        bounded_of(Type::app(Type::Con(option_path), ty("String"))),
    );
    d.values
        .insert(Path::new(T, "get_raw"), bounded_of(Type::con(T, "Raw")));
    d.values.insert(
        Path::new(T, "open_handle"),
        Scheme::new(
            vec![("s".into(), Kind::Region)],
            Type::fun(
                ty("Unit"),
                scope_s(),
                Type::app(Type::con(T, "Handle"), Type::Var(region_s.clone())),
            ),
        ),
    );
    d.values.insert(
        Path::new(T, "get_generic"),
        Scheme::new(
            vec![("s".into(), Kind::Region), ("a".into(), Kind::Type)],
            Type::fun(
                a(),
                scope_s(),
                Type::Bounded(Rc::new(a()), Caps::atom(region_s.clone())),
            ),
        ),
    );
    d.values.insert(
        Path::new(T, "use_handle"),
        Scheme::new(
            vec![("s".into(), Kind::Region)],
            Type::fun(
                Type::app(Type::con(T, "Handle"), Type::Var(region_s.clone())),
                Stage::Runtime(Row::none(), Row::none()),
                ty("Unit"),
            ),
        ),
    );
    let closed_rec = Type::Record(Fields {
        fields: [("x".into(), ty("Int"))].into(),
        tail: None,
    });
    let open_rec = Type::Record(Fields {
        fields: [("x".into(), ty("Int"))].into(),
        tail: Some(Var::Named("r".into())),
    });
    d.values
        .insert(Path::new(T, "get_closed_rec"), bounded_of(closed_rec));
    d.values.insert(
        Path::new(T, "get_open_rec"),
        Scheme::new(
            vec![("s".into(), Kind::Region), ("r".into(), Kind::Fields)],
            Type::fun(
                ty("Unit"),
                scope_s(),
                Type::Bounded(Rc::new(open_rec), Caps::atom(region_s)),
            ),
        ),
    );
    d
}

/// (Scope) ignores vacuous value bounds on types that hold nothing (`String^<={s}`,
/// `(Option String)^<={s}`, closed scalar records), including when unifying an
/// outer inference variable.
#[test]
fn scope_ignores_vacuous_value_bounds() {
    let d = scope_value_bound_decls();
    let scoped = |body: Term| Term::Scope("s".into(), Rc::new(body));
    let call = |name: &str, arg: Term| Term::app(Term::Global(Path::new(T, name)), arg);

    assert!(
        Checker::new(&d)
            .infer_top(&scoped(call("get_str", Term::unit())))
            .is_ok()
    );
    assert!(
        Checker::new(&d)
            .infer_top(&scoped(call("get_opt_str", Term::unit())))
            .is_ok()
    );
    assert!(
        Checker::new(&d)
            .infer_top(&scoped(call("get_closed_rec", Term::unit())))
            .is_ok()
    );
    assert!(
        Checker::new(&d)
            .infer_top(&Term::Tuple(Rc::from([
                scoped(call("get_str", Term::unit())),
                Term::string("ok"),
            ])))
            .is_ok()
    );
    let outer_meta = Term::Match(
        Rc::new(Term::bool(true)),
        Rc::from([
            Arm {
                pat: Pat::Lit(Lit::Bool(true)),
                guard: None,
                body: scoped(call("get_str", Term::unit())),
            },
            Arm {
                pat: Pat::Lit(Lit::Bool(false)),
                guard: None,
                body: Term::string("fallback"),
            },
        ]),
    );
    assert!(Checker::new(&d).infer_top(&outer_meta).is_ok());
    let wrapped_some = scoped(Term::Apply(
        Head::Con(Rc::new(Con {
            ty: Path::new("Prelude", "Option"),
            name: "Some".into(),
            arity: 1,
            dicts: 0,
        })),
        Rc::from([call("get_str", Term::unit())]),
    ));
    assert!(Checker::new(&d).infer_top(&wrapped_some).is_ok());
}

/// (Scope) still rejects capabilities (`Handle s`), opaque acquired resources
/// (`Raw^<={s}`), closures using `s`, generic bounded variables (`a^<={s}`),
/// and open records (`{ x : Int | r }^<={s}`).
#[test]
fn scope_still_rejects_capabilities_and_opaque_bounds() {
    let d = scope_value_bound_decls();
    let scoped = |body: Term| Term::Scope("s".into(), Rc::new(body));
    let call = |name: &str, arg: Term| Term::app(Term::Global(Path::new(T, name)), arg);

    rejected_by(
        Checker::new(&d).infer_top(&scoped(call("open_handle", Term::unit()))),
        "Scope",
    );
    rejected_by(
        Checker::new(&d).infer_top(&scoped(call("get_raw", Term::unit()))),
        "Scope",
    );
    let escaping_closure = scoped(Term::Let(
        "h".into(),
        None,
        Rc::new(call("open_handle", Term::unit())),
        Rc::new(Term::lam("_", call("use_handle", Term::Var("h".into())))),
    ));
    rejected_by(Checker::new(&d).infer_top(&escaping_closure), "Scope");
    let generic_escape = Term::lam(
        "x",
        Term::Let(
            "_".into(),
            None,
            Rc::new(scoped(call("get_generic", Term::Var("x".into())))),
            Rc::new(Term::unit()),
        ),
    );
    rejected_by(
        Checker::new(&d).check_top(
            &generic_escape,
            &poly(Type::fun(
                Type::Bounded(Rc::new(a()), Caps::default()),
                Stage::Runtime(Row::none(), Row::none()),
                ty("Unit"),
            )),
        ),
        "Scope",
    );
    rejected_by(
        Checker::new(&d).infer_top(&scoped(call("get_open_rec", Term::unit()))),
        "Scope",
    );
}

fn stream_type(r: Row, e: Row, elem: Type) -> Type {
    Type::app(
        Type::app(
            Type::app(Type::Con(Path::new(T, "Stream")), Type::Errors(r)),
            Type::Errors(e),
        ),
        elem,
    )
}

fn stream_decls() -> Decls {
    let mut d = decls();
    d.params.insert(
        Path::new(T, "Stream"),
        vec![Kind::Services, Kind::Errors, Kind::Type],
    );
    let s = Var::Named("s".into());
    let e = Var::Named("e".into());
    let c = Var::Named("c".into());
    let mut scope_row = Row::none();
    scope_row.elems.insert(
        Path::new("Prelude", "Scope"),
        Rc::from([Type::Var(s.clone())]),
    );
    d.values.insert(
        Path::new(T, "make_stream"),
        Scheme::new(
            vec![("s".into(), Kind::Region), ("e".into(), Kind::Errors)],
            Type::fun(
                Type::fun(
                    ty("Int"),
                    Stage::Runtime(Row::none(), Row::var(e.clone())),
                    ty("Int"),
                ),
                Stage::Runtime(scope_row, Row::none()),
                Type::Captured(
                    Rc::new(stream_type(Row::none(), Row::var(e.clone()), ty("Int"))),
                    Caps::atom(s),
                ),
            ),
        ),
    );
    d.values.insert(
        Path::new(T, "run_stream"),
        Scheme::new(
            vec![("e".into(), Kind::Errors), ("c".into(), Kind::Captures)],
            Type::fun(
                Type::Captured(
                    Rc::new(stream_type(Row::none(), Row::var(e.clone()), ty("Int"))),
                    Caps::var(c),
                ),
                Stage::Runtime(Row::none(), Row::var(e)),
                ty("Int"),
            ),
        ),
    );
    d
}

/// Reduced Repro 2: inside `scope { ... }`, `?e` is an open error-row meta
/// while `?c` is an open capture-row meta. `run_stream` must grow `?c` to
/// `{$owner}` inside the scope rather than binding `?e` to `$owner`.
#[test]
fn open_region_solving_does_not_bind_error_row_meta() {
    let d = stream_decls();
    let body = Term::Let(
        "st".into(),
        None,
        Rc::new(Term::app(
            Term::Global(Path::new(T, "make_stream")),
            Term::lam(
                "n",
                Term::Let(
                    "_".into(),
                    None,
                    Rc::new(Term::Await(Rc::new(Term::Task(Rc::new(Term::unit()))))),
                    Rc::new(Term::Var("n".into())),
                ),
            ),
        )),
        Rc::new(Term::app(
            Term::Global(Path::new(T, "run_stream")),
            Term::Var("st".into()),
        )),
    );
    let program = Term::Let(
        "checked".into(),
        None,
        Rc::new(Term::Scope("$owner".into(), Rc::new(body))),
        Rc::new(Term::Var("checked".into())),
    );
    let (inferred, _) = Checker::new(&d)
        .infer_top(&program)
        .expect("error-row meta must not be bound to the scope region");
    assert_eq!(inferred, ty("Int"));
}

/// `take_open_region` solves a genuine open `Kind::Region` meta in a written
/// qualifier (`{?r}`), while never binding a type meta or a region meta inside
/// a type argument.
#[test]
fn open_region_solving_only_binds_written_qualifier_region_metas() {
    let mut d = stream_decls();
    let unit_fn = || Type::fun(ty("Unit"), Stage::Pure, ty("Unit"));
    d.values.insert(
        Path::new(T, "apply_braced"),
        Scheme::new(
            vec![("r".into(), Kind::Region)],
            Type::fun(
                Type::Captured(Rc::new(unit_fn()), Caps::atom(Var::Named("r".into()))),
                Stage::Pure,
                ty("Unit"),
            ),
        ),
    );
    let caller_sig = Scheme::new(
        vec![("x".into(), Kind::Region)],
        Type::fun(
            Type::Captured(Rc::new(unit_fn()), Caps::atom(Var::Named("x".into()))),
            Stage::Pure,
            ty("Unit"),
        ),
    );
    let caller = Term::lam(
        "g",
        Term::app(
            Term::Global(Path::new(T, "apply_braced")),
            Term::Var("g".into()),
        ),
    );
    assert_eq!(Checker::new(&d).check_top(&caller, &caller_sig), Ok(()));

    let inner_region = Var::Named("s".into());
    for kind_is_region in [false, true] {
        let mut checker = Checker::new(&d);
        checker.regions.kinds.insert(inner_region.clone());
        let meta = checker.fresh();
        let elem = if kind_is_region {
            if let Type::Var(v) = &meta {
                checker.regions.kinds.insert(v.clone());
            }
            Type::app(Type::con(T, "Handle"), meta.clone())
        } else {
            meta.clone()
        };
        let cap_meta = checker.fresh_caps();
        let held = Type::Captured(
            Rc::new(stream_type(Row::none(), Row::none(), elem.clone())),
            Caps::atom(inner_region.clone()),
        );
        let within = Type::Captured(
            Rc::new(stream_type(Row::none(), Row::none(), elem)),
            cap_meta.clone(),
        );
        checker.hold(&[held], &[within], &Caps::default(), "≤-Cap");
        checker.grow_holds().expect("grow_holds succeeds");
        assert_eq!(
            checker.zonk(&meta),
            meta,
            "argument meta must stay unsolved"
        );
        assert!(
            checker.zonk_caps(&cap_meta).atoms.contains(&inner_region),
            "capture-row meta must grow to include `s`"
        );
    }
}
