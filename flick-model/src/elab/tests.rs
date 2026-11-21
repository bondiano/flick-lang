use std::rc::Rc;

use super::*;
use crate::decls::Class;
use crate::types::{Kind, Row, Scheme, Stage, Type, Var};
use crate::variance::Sign;

const M: &str = "M";

pub(super) fn p(name: &str) -> Path {
    Path::new(M, name)
}

pub(super) fn ty(name: &str) -> Type {
    Type::con("Prelude", name)
}

pub(super) fn var(name: &str) -> Type {
    Type::Var(Var::Named(name.into()))
}

pub(super) fn data(name: &str, args: Vec<Type>) -> Type {
    args.into_iter().fold(Type::con(M, name), Type::app)
}

pub(super) fn dict(class: &str, args: Vec<Type>) -> Type {
    Type::Dict(p(class), Rc::from(args))
}

pub(super) fn fn_pure() -> Type {
    Type::fun(ty("Unit"), Stage::Pure, ty("Unit"))
}

pub(super) fn fn_rt(service: &str) -> Type {
    Type::fun(
        ty("Unit"),
        Stage::Runtime(Row::one(p(service)), Row::none()),
        ty("Unit"),
    )
}

pub(super) fn fn_rt_empty() -> Type {
    Type::fun(
        ty("Unit"),
        Stage::Runtime(Row::none(), Row::none()),
        ty("Unit"),
    )
}

pub(super) fn cap_fn(t: Type, cap: &str) -> Type {
    Type::Captured(Rc::new(t), Caps::atom(Var::Named(cap.into())))
}

fn add_ctor(d: &mut Decls, ty_name: &str, params: &[(&str, Kind)], k: &str, fields: Vec<Type>) {
    let res = data(ty_name, params.iter().map(|(v, _)| var(v)).collect());
    let body = fields
        .into_iter()
        .rev()
        .fold(res, |r, f| Type::fun(f, Stage::Pure, r));
    let vars = params
        .iter()
        .map(|(v, k)| (Name::from(*v), k.clone()))
        .collect();
    d.constructors.insert(p(k), Scheme::new(vars, body));
    d.params
        .insert(p(ty_name), params.iter().map(|(_, k)| k.clone()).collect());
    d.variants
        .entry(p(ty_name))
        .or_default()
        .push((k.into(), 0));
}

type MethodSpec<'a> = (&'a str, Vec<(&'a str, Kind)>, Type);

fn add_class(
    d: &mut Decls,
    name: &str,
    params: &[(&str, Kind)],
    supers: Vec<Type>,
    methods: Vec<MethodSpec<'_>>,
) {
    let param_names: Vec<Name> = params.iter().map(|(v, _)| Name::from(*v)).collect();
    let class_dict = dict(name, params.iter().map(|(v, _)| var(v)).collect());
    let mut method_names = Vec::new();
    for (m, extra_vars, body) in methods {
        let mut vars: Vec<(Name, Kind)> = params
            .iter()
            .map(|(v, k)| (Name::from(*v), k.clone()))
            .collect();
        vars.extend(extra_vars.into_iter().map(|(v, k)| (Name::from(v), k)));
        let full = Type::fun(class_dict.clone(), Stage::Pure, body);
        d.values.insert(p(m), Scheme::new(vars, full));
        method_names.push(Name::from(m));
    }
    d.params
        .insert(p(name), params.iter().map(|(_, k)| k.clone()).collect());
    d.classes.insert(
        p(name),
        Class {
            params: param_names,
            supers,
            methods: method_names,
        },
    );
}

pub(super) fn add_instance(
    d: &mut Decls,
    name: &str,
    vars: &[(&str, Kind)],
    context: Vec<Constraint>,
    head: Constraint,
) {
    let head_ty = Type::Dict(head.class, Rc::from(head.args));
    let body = context
        .into_iter()
        .rev()
        .fold(head_ty, |r, c| Type::fun(c.to_dict_type(), Stage::Pure, r));
    let scheme = Scheme::new(
        vars.iter()
            .map(|(v, k)| (Name::from(*v), k.clone()))
            .collect(),
        body,
    );
    d.instances.insert(p(name), scheme.clone());
    // Mirror instance dictionary as a top-level value so `Checker` can check
    // resolved dictionary terms end-to-end.
    d.values.insert(p(name), scheme);
}

fn add_base_data(d: &mut Decls) {
    add_ctor(d, "Box", &[("a", Kind::Type)], "MkBox", vec![var("a")]);
    add_ctor(
        d,
        "Pair",
        &[("a", Kind::Type), ("b", Kind::Type)],
        "MkPair",
        vec![var("a"), var("b")],
    );
    add_ctor(
        d,
        "Sink",
        &[("a", Kind::Type)],
        "MkSink",
        vec![Type::fun(var("a"), Stage::Pure, ty("Unit"))],
    );
    add_ctor(
        d,
        "Cell",
        &[("a", Kind::Type)],
        "MkCell",
        vec![Type::Ref(Rc::new(var("a")))],
    );
    add_ctor(d, "Phantom", &[("a", Kind::Type)], "MkPhantom", vec![]);
}

fn add_extra_classes(d: &mut Decls) {
    let k_f = Kind::Arrow(Rc::new(Kind::Type), Rc::new(Kind::Type));
    add_class(
        d,
        "Functor",
        &[("f", k_f)],
        vec![],
        vec![(
            "fmap",
            vec![("a", Kind::Type), ("b", Kind::Type)],
            Type::fun(
                Type::fun(var("a"), Stage::Pure, var("b")),
                Stage::Pure,
                Type::fun(
                    Type::app(var("f"), var("a")),
                    Stage::Pure,
                    Type::app(var("f"), var("b")),
                ),
            ),
        )],
    );
    add_class(
        d,
        "Cont",
        &[("a", Kind::Type)],
        vec![],
        vec![(
            "run_cont",
            vec![],
            Type::fun(
                Type::fun(var("a"), Stage::Pure, ty("Int")),
                Stage::Pure,
                ty("Int"),
            ),
        )],
    );
    add_class(
        d,
        "Tag",
        &[("a", Kind::Type)],
        vec![dict("Monoid", vec![var("a")])],
        vec![("tag", vec![], Type::fun(var("a"), Stage::Pure, ty("Int")))],
    );
    add_class(d, "TypeName", &[("a", Kind::Type)], vec![], vec![]);
    add_class(
        d,
        "SubMarker",
        &[("a", Kind::Type)],
        vec![dict("TypeName", vec![var("a")])],
        vec![(
            "sub_mark",
            vec![],
            Type::fun(var("a"), Stage::Pure, ty("String")),
        )],
    );
    add_class(
        d,
        "OverPhantom",
        &[("a", Kind::Type)],
        vec![],
        vec![(
            "over_phantom",
            vec![],
            data("Phantom", vec![Type::fun(var("a"), Stage::Pure, ty("Int"))]),
        )],
    );
}

pub(super) fn base_decls() -> Decls {
    let mut d = Decls::default();
    add_base_data(&mut d);

    // Classes covering -, +, (-, +), inv, higher-kinded, double-flip, bi, superclass
    add_class(
        &mut d,
        "Show",
        &[("a", Kind::Type)],
        vec![],
        vec![(
            "show",
            vec![],
            Type::fun(var("a"), Stage::Pure, ty("String")),
        )],
    );
    add_class(
        &mut d,
        "Eq",
        &[("a", Kind::Type)],
        vec![],
        vec![(
            "eq",
            vec![],
            Type::fun(
                var("a"),
                Stage::Pure,
                Type::fun(var("a"), Stage::Pure, ty("Bool")),
            ),
        )],
    );
    add_class(
        &mut d,
        "Ord",
        &[("a", Kind::Type)],
        vec![dict("Eq", vec![var("a")])],
        vec![(
            "cmp",
            vec![],
            Type::fun(
                var("a"),
                Stage::Pure,
                Type::fun(var("a"), Stage::Pure, ty("Int")),
            ),
        )],
    );
    add_class(
        &mut d,
        "SubOrd",
        &[("a", Kind::Type)],
        vec![dict("Ord", vec![var("a")])],
        vec![],
    );
    add_class(
        &mut d,
        "Default",
        &[("a", Kind::Type)],
        vec![],
        vec![("default_val", vec![], var("a"))],
    );
    add_class(
        &mut d,
        "Decode",
        &[("a", Kind::Type)],
        vec![],
        vec![(
            "decode",
            vec![],
            Type::fun(ty("String"), Stage::Pure, data("Box", vec![var("a")])),
        )],
    );
    add_class(
        &mut d,
        "Into",
        &[("a", Kind::Type), ("b", Kind::Type)],
        vec![],
        vec![("into", vec![], Type::fun(var("a"), Stage::Pure, var("b")))],
    );
    add_class(
        &mut d,
        "Monoid",
        &[("a", Kind::Type)],
        vec![],
        vec![
            ("empty", vec![], var("a")),
            (
                "combine",
                vec![],
                Type::fun(
                    var("a"),
                    Stage::Pure,
                    Type::fun(var("a"), Stage::Pure, var("a")),
                ),
            ),
        ],
    );
    add_extra_classes(&mut d);
    d.finish();
    d
}

#[test]
fn infers_class_parameter_variances_matching_lean() {
    let d = base_decls();
    assert_eq!(d.class_signs[&p("Show")], vec![Sign::Neg]);
    assert_eq!(d.class_signs[&p("Eq")], vec![Sign::Neg]);
    assert_eq!(d.class_signs[&p("Ord")], vec![Sign::Neg]);
    assert_eq!(d.class_signs[&p("SubOrd")], vec![Sign::Neg]);
    assert_eq!(d.class_signs[&p("Default")], vec![Sign::Pos]);
    assert_eq!(d.class_signs[&p("Decode")], vec![Sign::Pos]);
    assert_eq!(d.class_signs[&p("Into")], vec![Sign::Neg, Sign::Pos]);
    assert_eq!(d.class_signs[&p("Monoid")], vec![Sign::Inv]);
    assert_eq!(d.class_signs[&p("Functor")], vec![Sign::Inv]);
    // Double flip `(a -> Int) -> Int` is invariant (reach preservation).
    assert_eq!(d.class_signs[&p("Cont")], vec![Sign::Inv]);
    // Invariant superclass `Monoid` forces `Tag` invariant even though its
    // own method is consumer-only.
    assert_eq!(d.class_signs[&p("Tag")], vec![Sign::Inv]);
    // Unused parameter is `Bi` (exact match in `Serves` / `LeLive.dict`).
    assert_eq!(d.class_signs[&p("TypeName")], vec![Sign::Bi]);
    // Superclass of a genuinely `Bi` class forces `Inv` (exact superclass slot).
    assert_eq!(d.class_signs[&p("SubMarker")], vec![Sign::Inv]);
    // Under a `bi` data parameter (`Phantom`), occurrence at covariant
    // polarity requires `Pos` (matching Lean `inferTyVarArgs` / `Ty.PolD.data`).
    assert_eq!(d.class_signs[&p("OverPhantom")], vec![Sign::Pos]);
}

#[test]
fn direction_aware_resolution_for_neg_pos_inv_and_bi() {
    let mut d = base_decls();
    add_instance(
        &mut d,
        "inst_show_rt",
        &[],
        vec![],
        Constraint::new(p("Show"), vec![fn_rt("Async")]),
    );
    add_instance(
        &mut d,
        "inst_default_box_pure",
        &[],
        vec![],
        Constraint::new(p("Default"), vec![data("Box", vec![fn_pure()])]),
    );
    add_instance(
        &mut d,
        "inst_monoid_pure",
        &[],
        vec![],
        Constraint::new(p("Monoid"), vec![fn_pure()]),
    );
    add_instance(
        &mut d,
        "inst_typename_pure",
        &[],
        vec![],
        Constraint::new(p("TypeName"), vec![fn_pure()]),
    );
    add_instance(
        &mut d,
        "inst_show_box",
        &[("a", Kind::Type)],
        vec![Constraint::new(p("Show"), vec![var("a")])],
        Constraint::new(p("Show"), vec![data("Box", vec![var("a")])]),
    );
    d.finish();
    assert!(check_coherence(&d).is_ok());

    // Consumer-only (`-`): runtime-stage instance serves pure query.
    let show_pure = Constraint::new(p("Show"), vec![fn_pure()]);
    let dict_show = entail(&d, &[], &show_pure).expect("Show fnR serves Show fnP");
    assert!(matches!(dict_show, Term::Global(ref g) if *g == p("inst_show_rt")));

    // Context recursion through parameterized instance `Show a => Show (Box a)`.
    let show_box_pure = Constraint::new(p("Show"), vec![data("Box", vec![fn_pure()])]);
    let dict_box = entail(&d, &[], &show_box_pure).expect("Show (Box fnP) resolves via Box + fnR");
    assert!(matches!(
        dict_box,
        Term::App(ref f, ref a)
            if matches!(**f, Term::Global(ref g) if *g == p("inst_show_box"))
                && matches!(**a, Term::Global(ref g) if *g == p("inst_show_rt"))
    ));

    // Wrong direction for `-`: `Show (Unit ->R⟨Async⟩ Unit)` does not serve a
    // broader service row `Show (Unit ->R⟨Async, Log⟩ Unit)`.
    let broader = Type::fun(
        ty("Unit"),
        Stage::Runtime(
            Row::one(p("Async"))
                .sum(&Row::one(p("Log")))
                .expect("row sum"),
            Row::none(),
        ),
        ty("Unit"),
    );
    assert!(entail(&d, &[], &Constraint::new(p("Show"), vec![broader])).is_err());

    // Producer-only (`+`): pure-stage instance serves runtime and captured queries.
    let def_rt = Constraint::new(p("Default"), vec![data("Box", vec![fn_rt("Async")])]);
    let dict_def = entail(&d, &[], &def_rt).expect("Default (Box fnP) serves Default (Box fnR)");
    assert!(matches!(dict_def, Term::Global(ref g) if *g == p("inst_default_box_pure")));

    let def_cap = Constraint::new(
        p("Default"),
        vec![data("Box", vec![cap_fn(fn_pure(), "c")])],
    );
    assert!(entail(&d, &[], &def_cap).is_ok());

    // Invariant (`inv`) and unused (`bi`): exact equality only.
    assert!(entail(&d, &[], &Constraint::new(p("Monoid"), vec![fn_pure()])).is_ok());
    assert!(entail(&d, &[], &Constraint::new(p("Monoid"), vec![fn_rt("Async")])).is_err());
    assert!(entail(&d, &[], &Constraint::new(p("TypeName"), vec![fn_pure()])).is_ok());
    assert!(
        entail(
            &d,
            &[],
            &Constraint::new(p("TypeName"), vec![fn_rt("Async")])
        )
        .is_err()
    );

    // Superclass projection from a given under `-`: `Ord fnR` serves `Eq fnP`.
    let givens = vec![(
        Name::from("d_ord"),
        Constraint::new(p("Ord"), vec![fn_rt("Async")]),
    )];
    let eq_pure = Constraint::new(p("Eq"), vec![fn_pure()]);
    let projected = entail(&d, &givens, &eq_pure).expect("Ord fnR gives Eq fnP");
    assert!(matches!(
        projected,
        Term::Field(ref inner, ref slot)
            if matches!(**inner, Term::Var(ref x) if &**x == "d_ord") && &**slot == "Eq"
    ));
}
