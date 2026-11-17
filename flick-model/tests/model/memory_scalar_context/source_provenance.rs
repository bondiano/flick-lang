#[test]
fn raw_foreign_names_require_explicit_source_provenance_and_never_prefix_decoding() {
    let original = run::compile_source(BASIC).expect("source checks");
    let declarations = facts(&original);
    for provenance in [None, Some("missing"), Some("text_only"), Some("convert")] {
        let mut modules = run::compile_source(BASIC).expect("same checked declarations");
        let declaration = modules[0]
            .ir
            .items
            .iter_mut()
            .find_map(|item| match &mut item.kind {
                ItemKind::Foreign(value) if value.name == "convert" => Some(value),
                _ => None,
            })
            .expect("source foreign");
        declaration.name = "$raw_convert".into();
        declaration.source_name = provenance.map(str::to_owned);
        let entry = Name::new("App.ScalarContext", "$raw_convert");
        let result = lower(&modules, &declarations, &entry, false);
        if provenance == Some("convert") {
            returned_number(
                &result.expect("explicit genuine source scheme"),
                &entry,
                &Lit::Int(7),
                7.0_f64.to_bits(),
            );
        } else {
            result.expect_err("no guessed prefix/domain fallback");
        }
    }
}

#[test]
fn static_alias_views_do_not_compete_with_checked_own_declarations() {
    let source = r#"module App.AliasContext exposing (plain, convert)
module Alias = Prelude
plain : Unit -> Unit
let plain () = ()
convert : Alias.Int -> Alias.Float
foreign pure convert from global "Number"
"#;
    let modules = run::compile_source(source).expect("accepted static module alias");
    for release in [false, true] {
        lower(
            &modules,
            &facts(&modules),
            &Name::new("App.AliasContext", "plain"),
            release,
        )
        .expect("a plain entry must not fail on legitimate duplicate views");
        let entry = Name::new("App.AliasContext", "convert");
        let program = lower(&modules, &facts(&modules), &entry, release)
            .expect("numeric types reached through a genuine static alias");
        returned_number(&program, &entry, &Lit::Int(7), 7.0_f64.to_bits());
    }
}

#[test]
fn owning_open_representation_precedes_public_views_but_nested_only_facts_remain() {
    let source = r#"module App.OpenScalar exposing (Small, convert)
opaque type alias Small = Int
convert : Small -> Float
foreign pure convert from global "Number"
"#;
    let modules = run::compile_source(source).expect("checked opaque representation");
    let own = modules[0].declarations.as_ref();
    let public = &modules[0].interface;
    let definition = |interface: &Interface| {
        interface
            .exports
            .iter()
            .find(|value| value.entity.name == "Small" && value.entity.namespace == Namespace::Type)
            .and_then(|value| value.definition.as_ref())
            .map(|value| &value.form)
            .cloned()
    };
    assert!(matches!(definition(own), Some(Form::Alias { .. })));
    assert!(matches!(definition(public), Some(Form::Nominal { .. })));
    let mut container = own.clone();
    container.module = "App.Container".into();
    container.exports.clear();
    container.modules.clear();
    container
        .modules
        .insert("Projection".into(), public.clone());
    let entry = Name::new("App.OpenScalar", "convert");
    for release in [false, true] {
        for declarations in [
            vec![container.clone(), own.clone()],
            vec![own.clone(), container.clone()],
        ] {
            let program = lower(&modules, &declarations, &entry, release)
                .expect("own opened representation, not traversal order, is authority");
            returned_number(&program, &entry, &Lit::Int(7), 7.0_f64.to_bits());
        }
        lower(&modules, &[container.clone()], &entry, release)
            .expect_err("a public opaque view alone proves no numeric representation");
        let mut nested = container.clone();
        nested.modules.insert("Projection".into(), own.clone());
        let program = lower(&modules, &[nested], &entry, release)
            .expect("private checked descendant remains available without an explicit own root");
        returned_number(&program, &entry, &Lit::Int(7), 7.0_f64.to_bits());
    }
}

#[test]
fn both_selection_arrows_need_homogeneous_numeric_evidence_and_source_provenance() {
    for operation in ["min", "max"] {
        for domain in [
            "Float -> Float -> Int",
            "Float -> Int -> Float",
            "Int -> Float -> Float",
            "String -> String -> String",
            "Cycle Int -> Cycle Int -> Cycle Int",
        ] {
            let source = format!(
                r#"module App.SelectionEvidence exposing (choose)
opaque type alias Cycle a = Cycle (List a)
choose : {domain}
foreign pure choose from "flick:Number" "{operation}"
  arity 1
"#
            );
            let modules =
                run::compile_source(&source).expect("unsupported domains remain source accepted");
            let entry = Name::new("App.SelectionEvidence", "choose");
            for release in [false, true] {
                let error = lower(&modules, &facts(&modules), &entry, release)
                    .expect_err("nonhomogeneous, Other and Unknown domains fail before execution");
                assert!(matches!(error, perceus::LowerError::Unsupported { .. }));
                if !domain.starts_with("Cycle") {
                    assert!(
                        error
                            .to_string()
                            .contains("unsupported checked Number selection domain"),
                        "{domain}: {error}"
                    );
                }
                lower(&modules, &[], &entry, release)
                    .expect_err("missing checked selection scheme");
            }
        }
        let source = format!(
            r#"module App.SelectionEvidence exposing (choose)
choose : Float -> Float -> Float
foreign pure choose from "flick:Number" "{operation}"
  arity 1
"#
        );
        for provenance in [None, Some("missing"), Some("choose")] {
            let mut modules = run::compile_source(&source).expect("valid selection source");
            let declarations = facts(&modules);
            for item in &mut modules[0].ir.items {
                if let ItemKind::Foreign(value) = &mut item.kind {
                    value.name = "$opaque_generated_selection".into();
                    value.source_name = provenance.map(str::to_owned);
                }
            }
            let entry = Name::new("App.SelectionEvidence", "$opaque_generated_selection");
            let result = lower(&modules, &declarations, &entry, false);
            if provenance == Some("choose") {
                result.expect("explicit checked source provenance");
            } else {
                result.expect_err("no generated-name decoding or missing source fact");
            }
        }
    }
}

#[test]
fn exact_selection_abi_still_cannot_discharge_unknown_checked_domain_facts() {
    // Isolate domain evidence from the separately audited ABI guard. These
    // signatures are genuinely checked; only the test pairs their facts with
    // a canonical candidate IR to prove that metadata cannot replace evidence.
    for operation in ["min", "max"] {
        let source = |domain: &str| {
            format!(
                r#"module App.SelectionUnknown exposing (choose)
opaque type alias Cycle a = Cycle (List a)
choose : {domain}
foreign pure choose from "flick:Number" "{operation}"
  arity 1
"#
            )
        };
        let modules = run::compile_source(&source("Float -> Float -> Float"))
            .expect("exact Float selection candidate");
        let entry = Name::new("App.SelectionUnknown", "choose");
        for (domain, expected) in [
            (
                "Cycle Int -> Float -> Float",
                "unknown scalar domain through recursive alias",
            ),
            (
                "Float -> Cycle Int -> Float",
                "unknown scalar domain through recursive alias",
            ),
            (
                "Float -> Float -> Cycle Int",
                "unknown scalar domain through recursive alias",
            ),
            (
                "a -> a -> a",
                "unsupported checked Number selection domain Unknown",
            ),
        ] {
            let checked = run::compile_source(&source(domain)).expect("checked unknown domain");
            for release in [false, true] {
                let error = lower(&modules, &facts(&checked), &entry, release)
                    .expect_err("exact ABI cannot prove an unknown scalar domain");
                assert!(error.to_string().contains(expected), "{domain}: {error}");
            }
        }
    }
}
