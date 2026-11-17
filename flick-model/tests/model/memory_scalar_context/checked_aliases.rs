#[test]
fn no_evidence_path_fails_closed_but_arbitrary_numeric_names_use_checked_schemes() {
    let modules = run::compile_source(BASIC).expect("source accepted");
    let entry = Name::new("App.ScalarContext", "convert");
    let failure = perceus::lower(
        modules
            .iter()
            .map(|module| &module.ir)
            .chain(build::builtin_modules()),
        &entry,
    )
    .expect_err("Number ABI alone proves no numeric domain");
    assert!(
        failure
            .to_string()
            .contains("requires a checked declaration context")
    );
    for release in [false, true] {
        let program = lower(&modules, &facts(&modules), &entry, release)
            .expect("arbitrary source name, genuine Int domain");
        returned_number(&program, &entry, &Lit::Int(7), 7.0_f64.to_bits());
    }
}

#[test]
fn opened_opaque_function_aliases_restore_prior_argument_environments() {
    let source = r#"module App.ScalarAliases exposing (arbitrary_int, arbitrary_big)
opaque type alias Small = Int
opaque type alias Large = BigInt
opaque type alias Output = Float
opaque type alias Id a = a
opaque type alias Nest a = Id (Id a)
opaque type alias Convert = Nest Small -> Output
arbitrary_int : Convert
foreign pure arbitrary_int from global "Number"
arbitrary_big : Nest Large -> Output
foreign pure arbitrary_big from global "Number"
"#;
    let modules = run::compile_source(source).expect("legitimate opaque aliases check");
    for (name, literal, bits) in [
        ("arbitrary_int", Lit::Int(-7), (-7.0_f64).to_bits()),
        (
            "arbitrary_big",
            Lit::BigInt("9007199254740995".into()),
            0x4340_0000_0000_0002,
        ),
    ] {
        let entry = Name::new("App.ScalarAliases", name);
        for release in [false, true] {
            let program = lower(&modules, &facts(&modules), &entry, release)
                .expect("parameter substitution establishes exact numeric representation");
            returned_number(&program, &entry, &literal, bits);
        }
    }
}

#[test]
fn deep_acyclic_and_nested_identity_aliases_have_no_head_depth_cutoff() {
    let mut source =
        "module App.DeepScalar exposing (convert)\nopaque type alias A0 = Int\n".to_owned();
    for index in 1..200 {
        writeln!(source, "opaque type alias A{index} = A{}", index - 1).expect("source buffer");
    }
    source.push_str("opaque type alias Id a = a\nconvert : ");
    writeln!(
        source,
        "{}A199{} -> Float",
        "Id (".repeat(32),
        ")".repeat(32)
    )
    .expect("source buffer");
    source.push_str("foreign pure convert from global \"Number\"\n");
    let modules = run::compile_source(&source).expect("deep acyclic checked declaration");
    let entry = Name::new("App.DeepScalar", "convert");
    for release in [false, true] {
        let program = lower(&modules, &facts(&modules), &entry, release).unwrap_or_else(|error| {
            panic!(
                "no64/depth budget: {error}; foreign {:?}",
                modules[0]
                    .ir
                    .items
                    .iter()
                    .filter_map(|item| match &item.kind {
                        ItemKind::Foreign(value) => Some(value),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            )
        });
        returned_number(&program, &entry, &Lit::Int(7), 7.0_f64.to_bits());
    }
}

#[test]
fn a_recursive_domain_is_explicit_unknown_and_unused_recursive_arguments_stay_lazy() {
    let source = r#"module App.RecursiveScalar exposing (unknown, known, plain)
opaque type alias Cycle a = Cycle (List a)
opaque type alias Ignore a = Int
unknown : Cycle Int -> Float
foreign pure unknown from global "Number"
known : Ignore (Cycle Int) -> Float
foreign pure known from global "Number"
plain : Unit -> Unit
let plain () = ()
"#;
    let modules =
        run::compile_source(source).expect("source checker still accepts recursive opaque types");
    for release in [false, true] {
        let context = facts(&modules);
        let unknown = lower(
            &modules,
            &context,
            &Name::new("App.RecursiveScalar", "unknown"),
            release,
        )
        .expect_err("no finite cyclic scalar witness is claimed");
        assert!(
            unknown
                .to_string()
                .contains("unknown scalar domain through recursive alias"),
            "{unknown}; foreign {:?}",
            modules[0]
                .ir
                .items
                .iter()
                .filter_map(|item| match &item.kind {
                    ItemKind::Foreign(value) => Some(value),
                    _ => None,
                })
                .collect::<Vec<_>>()
        );
        let entry = Name::new("App.RecursiveScalar", "known");
        let program =
            lower(&modules, &context, &entry, release).expect("unused argument is not normalized");
        returned_number(&program, &entry, &Lit::Int(7), 7.0_f64.to_bits());
        lower(
            &modules,
            &context,
            &Name::new("App.RecursiveScalar", "plain"),
            release,
        )
        .expect("unreached recursive declarations do not poison context");
    }
}

#[test]
fn duplicate_incomplete_missing_and_error_facts_fail_closed_atomically() {
    let modules = run::compile_source(BASIC).expect("source checks");
    let original = facts(&modules);
    let plain = Name::new("App.ScalarContext", "plain");
    let mut duplicate = original.clone();
    duplicate.push(original[0].clone());
    assert!(
        lower(&modules, &duplicate, &plain, false)
            .expect_err("duplicate context even identical")
            .to_string()
            .contains("duplicate declaration context")
    );
    let mut invalid = original.clone();
    invalid[0].incomplete = true;
    assert!(
        lower(&modules, &invalid, &plain, false)
            .expect_err("incomplete before any code lowered")
            .to_string()
            .contains("incomplete declaration context")
    );
    for bad in [None, Some(Type::Error), Some(Type::meta(0))] {
        let mut invalid = original.clone();
        invalid[0]
            .exports
            .iter_mut()
            .find(|value| {
                value.entity.namespace == Namespace::Value && value.entity.name == "convert"
            })
            .expect("conversion scheme")
            .scheme = bad;
        assert!(matches!(
            lower(&modules, &invalid, &plain, false),
            Err(perceus::LowerError::Declarations(_))
        ));
    }
    let missing = lower(
        &modules,
        &[],
        &Name::new("App.ScalarContext", "convert"),
        false,
    )
    .expect_err("no source fact");
    assert!(
        missing
            .to_string()
            .contains("missing checked declaration scheme")
    );
    let mut conflict = original.clone();
    let mut view = original[0].clone();
    view.module = "App.NonOwningView".into();
    conflict[0]
        .exports
        .retain(|value| value.entity.name != "convert");
    let mut competing = view.clone();
    competing.module = "App.CompetingView".into();
    competing
        .exports
        .iter_mut()
        .find(|value| value.entity.namespace == Namespace::Value && value.entity.name == "convert")
        .expect("conversion scheme")
        .scheme = Some(Type::lang(Lang::Float));
    conflict.extend([view, competing]);
    assert!(
        lower(&modules, &conflict, &plain, false)
            .expect_err("conflicting fallback facts without an owning declaration")
            .to_string()
            .contains("conflicting declaration schemes")
    );
    let mut conflict = original.clone();
    let mut duplicate = conflict[0]
        .exports
        .iter()
        .find(|value| value.entity.namespace == Namespace::Value && value.entity.name == "convert")
        .expect("owned conversion")
        .clone();
    duplicate.scheme = Some(Type::lang(Lang::Float));
    conflict[0].exports.push(duplicate);
    assert!(
        lower(&modules, &conflict, &plain, false).is_err(),
        "conflicting own facts cannot be masked by projection precedence"
    );
}
