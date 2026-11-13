//! Static signatures keep independent field names until they have exact mappings.

use super::*;

const CONTRACT: &str = "module App.Contracts exposing (Sig)\nimport App.Named as N\ntype module Sig =\n  class Named = N.Named\n  describe : Named a => a -> String\n";

#[test]
fn class_rename_refuses_an_unmapped_static_result_signature_field() {
    let facade = "module App.Facade : App.Contracts.Sig exposing (Named, describe)\nimport App.Contracts\nimport App.Named exposing (Named, describe)\n";
    let f = fixture_of(&[
        ("src/App/Named.fl", NAMED),
        ("src/App/Contracts.fl", CONTRACT),
        ("src/App/Facade.fl", facade),
    ]);
    assert_clean(&f);
    assert_refused(&f, "render");
}

#[test]
fn class_rename_refuses_inline_static_result_signature_fields() {
    let user = "module App.Use exposing (..)\nimport App.Contracts\nmodule Facade : App.Contracts.Sig exposing (Named, describe) =\n  import App.Named exposing (Named, describe)\n";
    let f = fixture_of(&[
        ("src/App/Named.fl", NAMED),
        ("src/App/Contracts.fl", CONTRACT),
        ("src/App/Use.fl", user),
    ]);
    assert_clean(&f);
    assert_refused(&f, "render");
}

#[test]
fn class_rename_refuses_static_module_ascription_alias_fields() {
    for alias in [
        "module Facade : App.Contracts.Sig = App.Named\n",
        "module Facade = (App.Named : App.Contracts.Sig)\n",
    ] {
        let user = format!(
            "module App.Use exposing (Facade)\nimport App.Contracts\nimport App.Named\n{alias}"
        );
        let f = fixture_of(&[
            ("src/App/Named.fl", NAMED),
            ("src/App/Contracts.fl", CONTRACT),
            ("src/App/Use.fl", &user),
        ]);
        assert_clean(&f);
        assert_refused(&f, "render");
    }
}

#[test]
fn class_rename_refuses_static_argument_signature_fields() {
    let make = "private module App.Make(K : App.Contracts.Sig) exposing (answer)\nanswer : Int\nlet answer = 1\n";
    let user = "module App.Use exposing (answer)\nmodule Built = App.Make(App.Named)\nanswer : Int\nlet answer = Built.answer\n";
    let f = fixture_of(&[
        ("src/App/Named.fl", NAMED),
        ("src/App/Contracts.fl", CONTRACT),
        ("src/App/Make.fl", make),
        ("src/App/Use.fl", user),
    ]);
    assert_clean(&f);
    let renamed = NAMED.replace("describe", "render");
    let updated = fixture_of(&[
        ("src/App/Named.fl", &renamed),
        ("src/App/Contracts.fl", CONTRACT),
        ("src/App/Make.fl", make),
        ("src/App/Use.fl", user),
    ]);
    assert!(
        updated
            .ide()
            .analysis
            .diagnostics
            .iter()
            .any(|d| d.code == codes::T060 && d.message.contains("describe")),
        "the argument's unchanged contract still requires the old method field"
    );
    assert_refused(&f, "render");
}

#[test]
fn class_rename_allows_unused_contracts_and_unannotated_aliases() {
    for body in ["module Facade = App.Named\n", "let answer = 1\n"] {
        let user =
            format!("module App.Use exposing (..)\nimport App.Contracts\nimport App.Named\n{body}");
        let f = fixture_of(&[
            ("src/App/Named.fl", NAMED),
            ("src/App/Contracts.fl", CONTRACT),
            ("src/App/Use.fl", &user),
        ]);
        assert_clean(&f);
        assert!(
            f.ide()
                .rename(
                    f.file("App.Named"),
                    f.at("App.Named", "describe :", 0) + 1,
                    "render"
                )
                .is_ok(),
            "an independent, unused structural signature is not a method occurrence"
        );
        let renamed = NAMED.replace("describe", "render");
        let updated = fixture_of(&[
            ("src/App/Named.fl", &renamed),
            ("src/App/Contracts.fl", CONTRACT),
            ("src/App/Use.fl", &user),
        ]);
        assert_clean(&updated);
    }
}

#[test]
fn class_rename_refuses_expression_bodied_module_argument_contract() {
    let make = "private module App.Make(K : App.Contracts.Sig) exposing (answer)\nanswer : Int\nlet answer = 1\n";
    let user = "module App.Use = App.Make(App.Named)\n";
    let f = fixture_of(&[
        ("src/App/Named.fl", NAMED),
        ("src/App/Contracts.fl", CONTRACT),
        ("src/App/Make.fl", make),
        ("src/App/Use.fl", user),
    ]);
    assert_clean(&f);
    let renamed = NAMED.replace("describe", "render");
    let updated = fixture_of(&[
        ("src/App/Named.fl", &renamed),
        ("src/App/Contracts.fl", CONTRACT),
        ("src/App/Make.fl", make),
        ("src/App/Use.fl", user),
    ]);
    assert!(
        updated
            .ide()
            .analysis
            .diagnostics
            .iter()
            .any(|d| d.code == codes::T060 && d.message.contains("describe")),
        "the expression-bodied application still requires the old method field"
    );
    assert_refused(&f, "render");
}

#[test]
fn class_rename_refuses_unsupported_inline_applications_using_parent_aliases() {
    let make = "private module App.Make(K : App.Contracts.Sig) exposing (answer)\nanswer : Int\nlet answer = 1\n";
    for prefix in ["import App.Named as N\n", "module Alias = App.Named\n"] {
        let argument = if prefix.starts_with("import") {
            "N"
        } else {
            "Alias"
        };
        let user = format!(
            "module App.Use exposing (..)\n{prefix}module Child exposing (answer) =\n  module Built = App.Make({argument})\n  answer : Int\n  let answer = Built.answer\n"
        );
        let f = fixture_of(&[
            ("src/App/Named.fl", NAMED),
            ("src/App/Contracts.fl", CONTRACT),
            ("src/App/Make.fl", make),
            ("src/App/Use.fl", &user),
        ]);
        assert!(
            f.ide()
                .analysis
                .diagnostics
                .iter()
                .any(|d| d.code == codes::R019),
            "static applications inside inline children are not valid programs: {:?}",
            f.ide().analysis.diagnostics
        );
        assert_refused(&f, "render");
    }
}
