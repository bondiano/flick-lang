#[test]
fn contract_substitution_relabels_service_error_and_capture_rows_canonically() {
    use super::super::ty::{Effects, Elided, Row};

    let row_before = Type::Row(Row {
        entries: vec![
            (
                Elem::Service(Atom::Var("ZStore".into())),
                Type::Var("ZStore".into()),
            ),
            (
                Elem::Service(Atom::Var("AStore".into())),
                Type::Var("AStore".into()),
            ),
        ],
        tail: None,
        ..Row::default()
    });
    let err_row = Type::Row(Row::closed(vec![(
        Elem::Error(Atom::Var("Miss".into())),
        Type::Var("Miss".into()),
    )]));
    let cap_row = Type::Row(Row::closed(vec![(
        Elem::Capture(Atom::Var("Cap".into())),
        Type::Var("Cap".into()),
    )]));
    let fn_ty = Type::Fun {
        param: Box::new(Type::con("Prelude", "Unit")),
        effects: Some(Box::new(Effects::new(row_before, err_row))),
        result: Box::new(Type::Captured {
            ty: Box::new(Type::con("Prelude", "Int")),
            captures: Box::new(cap_row),
            value: false,
            elided: Elided::default(),
        }),
    };
    let substituted = substitute_contract_type(
        &fn_ty,
        &[
            ("ZStore".into(), Type::con("App.Env.Alpha", "Store")),
            ("AStore".into(), Type::con("App.Env.Zeta", "Store")),
            ("Miss".into(), Type::con("App.Env.Alpha", "Miss")),
            ("Cap".into(), Type::con("App.Env.Alpha", "Cap")),
        ],
    );
    let Type::Fun {
        effects: Some(effects),
        result,
        ..
    } = &substituted
    else {
        panic!("expected effectful function: {substituted:?}");
    };
    let Type::Row(services) = &effects.services else {
        panic!("services row");
    };
    // Canonically sorted: Alpha before Zeta despite original ZStore/AStore order.
    assert_eq!(
        services.entries,
        vec![
            (
                Elem::Named("App.Env.Alpha.Store".into()),
                Type::con("App.Env.Alpha", "Store"),
            ),
            (
                Elem::Named("App.Env.Zeta.Store".into()),
                Type::con("App.Env.Zeta", "Store"),
            ),
        ]
    );
    assert!(services.get("App.Env.Alpha.Store").is_some());
    assert!(services.get("App.Env.Zeta.Store").is_some());
    let Type::Row(errors) = &effects.errors else {
        panic!("errors row");
    };
    assert_eq!(
        errors.entries,
        vec![(
            Elem::Named("App.Env.Alpha.Miss".into()),
            Type::con("App.Env.Alpha", "Miss"),
        )]
    );
    let Type::Captured { captures, .. } = result.as_ref() else {
        panic!("captured result");
    };
    let Type::Row(caps) = captures.as_ref() else {
        panic!("capture row");
    };
    assert_eq!(
        caps.entries,
        vec![(
            Elem::Named("App.Env.Alpha.Cap".into()),
            Type::con("App.Env.Alpha", "Cap"),
        )]
    );
    let shown = super::super::show_type(&substituted);
    assert!(
        !shown.contains('@') && !shown.contains('~') && !shown.contains(" : "),
        "{shown}"
    );
}
