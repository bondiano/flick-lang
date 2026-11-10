#[test]
fn class_dictionary_type_uses_manifest_class_identity() {
    let src = "module App.Contracts exposing (S)\ntype module S =\n  type Key\n  class Equality = Prelude.Eq\n  equality : Equality.Dict Key\n  instance Equality Key from equality\n";
    let mut map = SourceMap::default();
    let file = map.add("App/Contracts.fl", src);
    let diagnostics = crate::check(&map, file);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn module_dictionary_requirement_checks_its_declared_value_type() {
    let prefix = "module App.Contracts exposing (S)\ntype module S =\n  type Key\n  class Equality = Prelude.Eq\n";
    for suffix in [
        "  instance Equality Key from equality\n",
        "  equality : String\n  instance Equality Key from equality\n",
    ] {
        let src = format!("{prefix}{suffix}");
        let mut map = SourceMap::default();
        let file = map.add("App/Contracts.fl", &src);
        let diagnostics = crate::check(&map, file);
        assert!(
            diagnostics.iter().any(|d| d.code == codes::T065),
            "{suffix}: {diagnostics:?}"
        );
    }
}

#[test]
fn pack_checks_named_module_against_its_contract() {
    let good = "module App.Contracts exposing (S, packed)\ntype module S =\n  type String\n  class Eq = Prelude.Eq\npacked : Module S\nlet packed = pack Prelude as S\n";
    let bad = "module App.Contracts exposing (S, packed)\ntype module S =\n  type Missing\npacked : Module S\nlet packed = pack Prelude as S\n";
    for (src, valid) in [(good, true), (bad, false)] {
        let mut map = SourceMap::default();
        let file = map.add("App/Contracts.fl", src);
        let diagnostics = crate::check(&map, file);
        if valid {
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
        } else {
            assert!(
                diagnostics.iter().any(|d| d.message.contains("Missing")),
                "{diagnostics:?}"
            );
        }
    }
}

#[test]
fn pack_lowers_to_a_record_of_public_values() {
    let src = "module App.Contracts exposing (S, packed)\ntype module S =\n  type Int\n  identity : a -> a\npacked : Module S\nlet packed = pack Prelude as S\n";
    let mut map = SourceMap::default();
    let file = map.add("App/Contracts.fl", src);
    let parsed = parse_ok(src);
    let syntax = resolve::module_syntax(&parsed.tree, src, file);
    let module = resolve::Module {
        name: "App.Contracts",
        file,
        text: src,
        tree: &parsed.tree,
        syntax: &syntax,
        generated: resolve::NO_GENERATED,
    };
    let (mut interface, resolved) = resolve::resolve(&module, &|_| None);
    assert!(resolved.is_empty(), "{resolved:?}");
    let local = |_: &str| true;
    let watched = types::Watched::none();
    let options = types::Options {
        local: &local,
        limits: types::Limits::default(),
        no_let_assert: false,
        thresholds: types::Thresholds::default(),
        effects: true,
        watched,
        stages_by_class: false,
        regions: types::RegionMode::Captures,
    };
    let (checked, typing) = types::check_typed(
        &module,
        &|_| None,
        types::builtins(),
        options,
        &mut interface,
    );
    assert!(checked.is_empty(), "{checked:?}");
    let ir = ir::lower(&module, &typing).expect("packed module lowers");
    assert!(ir.items.iter().any(|item| matches!(&item.kind, ir::ItemKind::Value { name, body, .. } if name == "packed" && matches!(&body.kind, ir::ExprKind::Record(fields) if matches!(&fields[..], [(field, value)] if field == "identity" && matches!(&value.kind, ir::ExprKind::Global { module, name } if module == "Prelude" && name == "identity"))))), "{ir:?}");
}

#[test]
fn unpack_opens_public_values_in_a_lexical_scope() {
    let src = "module App.Contracts exposing (S, packed, answer)\ntype module S =\n  identity : a -> a\npacked : Module S\nlet packed = pack Prelude as S\nanswer : Int\nlet answer = unpack packed as K in K.identity 42\n";
    let mut map = SourceMap::default();
    let file = map.add("App/Contracts.fl", src);
    let diagnostics = crate::check(&map, file);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn unpack_reports_an_escaping_abstract_type_witness() {
    let src = "module App.Contracts exposing (S, packed, leaked)\ntype module S =\n  type Key\n  value : Key\npacked : Module S\nforeign pure packed from \"fixture\" \"packed\"\nlet leaked = unpack packed as K in K.value\n";
    let mut map = SourceMap::default();
    let file = map.add("App/Contracts.fl", src);
    let diagnostics = crate::check(&map, file);
    assert!(
        diagnostics.iter().any(|d| d.code == codes::T064),
        "{diagnostics:?}"
    );
}

#[test]
fn pack_reports_distinct_contract_failures_at_the_module_path() {
    let cases = [
        ("type Missing", codes::T060),
        ("type Int = String", codes::T061),
        ("class Eq = Prelude.Ord", codes::T062),
        ("class Eq a =\n    eq : a -> a -> Bool", codes::T063),
    ];
    for (requirement, expected) in cases {
        let src = format!(
            "module App.Contracts exposing (S, packed)\ntype module S =\n  {requirement}\npacked : Module S\nlet packed = pack Prelude as S\n"
        );
        let mut map = SourceMap::default();
        let file = map.add("App/Contracts.fl", &src);
        let diagnostics = crate::check(&map, file);
        assert!(
            diagnostics.iter().any(|d| d.code == expected),
            "{requirement}: {diagnostics:?}"
        );
        if expected != codes::T063 {
            let module_at = u32::try_from(src.rfind("Prelude as S").expect("pack source"))
                .expect("source fits in u32");
            assert!(
                diagnostics
                    .iter()
                    .any(|d| d.code == expected
                        && d.span.is_some_and(|span| span.start == module_at)),
                "{requirement}: {diagnostics:?}"
            );
        }
    }
}

#[test]
fn unpack_keeps_manifest_equalities_and_separates_abstract_witnesses() {
    let manifest = "module App.Contracts exposing (S, packed, answer)\ntype module S =\n  type Key = String\n  value : Key\npacked : Module S\nforeign pure packed from \"fixture\" \"packed\"\nanswer : String\nlet answer = unpack packed as K in (K.value : K.Key)\n";
    let abstract_mixed = "module App.Contracts exposing (S, packed, bad)\ntype module S =\n  type Key\n  value : Key\n  equal : Key -> Key -> Bool\npacked : Module S\nforeign pure packed from \"fixture\" \"packed\"\nlet bad = unpack packed as A in unpack packed as B in A.equal A.value B.value\n";
    for (src, wanted) in [(manifest, None), (abstract_mixed, Some(codes::T003))] {
        let mut map = SourceMap::default();
        let file = map.add("App/Contracts.fl", src);
        let diagnostics = crate::check(&map, file);
        if let Some(code) = wanted {
            assert!(
                diagnostics.iter().any(|d| d.code == code),
                "{diagnostics:?}"
            );
        } else {
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
        }
    }
}

#[test]
fn module_contract_manifest_class_keeps_nominal_identity() {
    let src = "module App.Contracts exposing (S)\ntype module S =\n  class Eq = Prelude.Eq\n";
    let mut map = SourceMap::default();
    let file = map.add("App/Contracts.fl", src);
    let diagnostics = crate::check(&map, file);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let bad = "module App.Contracts exposing (S)\ntype module S =\n  class Eq = Prelude.String\n";
    let mut map = SourceMap::default();
    let file = map.add("App/Contracts.fl", bad);
    let diagnostics = crate::check(&map, file);
    assert!(
        diagnostics.iter().any(|d| d.code == codes::T002),
        "{diagnostics:?}"
    );
}

#[test]
fn static_module_contract_preserves_abstract_class_and_service_shapes() {
    let src = "module App.Contracts exposing (S)\ntype module S =\n  class Equality a =\n    equal : a -> a -> Bool\n  service Clock =\n    now : Unit ->{} Int\n";
    let mut map = SourceMap::default();
    let file = map.add("App/Contracts.fl", src);
    let parsed = parse_ok(src);
    let syntax = resolve::module_syntax(&parsed.tree, src, file);
    let module = resolve::Module {
        name: "App.Contracts",
        file,
        text: src,
        tree: &parsed.tree,
        syntax: &syntax,
        generated: resolve::NO_GENERATED,
    };
    let (mut interface, resolved) = resolve::resolve(&module, &|_| None);
    assert!(resolved.is_empty(), "{resolved:?}");
    let local = |_: &str| true;
    let watched = Watched::none();
    let options = Options {
        local: &local,
        limits: Limits::default(),
        no_let_assert: false,
        thresholds: Thresholds::default(),
        effects: true,
        watched,
        stages_by_class: false,
        regions: types::RegionMode::Captures,
    };
    let diagnostics = types::check(
        &module,
        &|_| None,
        types::builtins(),
        options,
        &mut interface,
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let requirements = match &interface
        .exports
        .iter()
        .find(|e| e.entity.name == "S")
        .and_then(|e| e.definition.as_ref())
        .expect("contract")
        .form
    {
        types::Form::ModuleContract { requirements } => requirements,
        other => panic!("expected module contract: {other:?}"),
    };
    assert_eq!(requirements.len(), 2);
    assert!(requirements.iter().all(|r| matches!(
        r,
        types::ModuleRequirement::Nominal {
            identity: None,
            definition: Some(_),
            ..
        }
    )));
    let written = iface::write(&interface);
    let read = iface::read(written.as_bytes(), "App.Contracts").expect("interface round trip");
    assert_eq!(read, interface);
    let page = doc::page(&interface, src);
    assert!(page.contains("class Equality a =\n    equal :"), "{page}");
    assert!(page.contains("service Clock =\n    now :"), "{page}");
}
