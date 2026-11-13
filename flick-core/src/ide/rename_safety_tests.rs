//! Renames must not change bindings or leave unchecked/structural uses behind.

use super::tests::{Fixture, fixture_of};
use crate::diagnostics::{Severity, codes};
#[cfg(feature = "native-project")]
use crate::project::Analysis;
#[cfg(feature = "native-project")]
use crate::source::SourceMap;
use crate::types::Ref;

const NAMED: &str = "module App.Named exposing (Named(..), describe)
class Named a =
  describe : a -> String
instance Named Int =
  let describe _ = \"value\"
";

const CONTRACT: &str = "module App.Contracts exposing (Sig)\nimport App.Named as N\ntype module Sig =\n  class Named = N.Named\n  describe : Named a => a -> String\n";

fn assert_clean(f: &Fixture) {
    let errors: Vec<_> = f
        .ide()
        .analysis
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
}

fn assert_refused(f: &Fixture, new_name: &str) {
    assert!(
        f.ide()
            .rename(
                f.file("App.Named"),
                f.at("App.Named", "describe :", 0) + 1,
                new_name
            )
            .is_err(),
        "rename to {new_name} must not change a binding or leave a method use behind"
    );
}

#[test]
fn class_rename_refuses_capture_of_a_bare_reference_under_same_range_wrappers() {
    let user = "module App.Use exposing (..)\nimport App.Named exposing (Named, describe)\nalias : Named a => Int -> a -> String\nlet alias render = describe\n";
    let f = fixture_of(&[("src/App/Named.fl", NAMED), ("src/App/Use.fl", user)]);
    assert_clean(&f);
    assert_refused(&f, "render");
}

#[test]
fn class_rename_refuses_capture_by_a_named_scope_owner() {
    let user = "module App.Use exposing (..)\nimport App.Named exposing (describe)\nrun : Unit ->{} String\nlet run () = scope method_owner { describe 1 }\n";
    let f = fixture_of(&[("src/App/Named.fl", NAMED), ("src/App/Use.fl", user)]);
    assert_clean(&f);
    assert_refused(&f, "method_owner");
}

#[test]
fn class_rename_refuses_type_recovery_that_skips_a_method_use() {
    let user = "module App.Use exposing (..)\nimport App.Named exposing (describe)\nlet broken = { {} with missing = describe 1 }\n";
    let f = fixture_of(&[("src/App/Named.fl", NAMED), ("src/App/Use.fl", user)]);
    assert!(
        f.ide()
            .analysis
            .diagnostics
            .iter()
            .any(|d| d.code == codes::T003)
    );
    assert!(!f.ide().analysis.module("App.Use").expect("use module").typing.names.values()
        .any(|r| matches!(r, Ref::Method { module, name } if module == "App.Named" && name == "describe")),
        "the checker has not resolved the skipped field-value use");
    assert_refused(&f, "render");
}

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

#[cfg(feature = "native-project")]
fn opening_fixture(user: &str) -> (Analysis, SourceMap) {
    let named = NAMED.replace("module App.Named", "module Pairs");
    let (analysis, sources) = pairs_fixture(user, &named);
    let errors: Vec<_> = analysis
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
    (analysis, sources)
}

#[cfg(feature = "native-project")]
fn pairs_fixture(user: &str, pairs: &str) -> (Analysis, SourceMap) {
    use std::collections::BTreeMap;
    use std::io::{self, ErrorKind};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use crate::plugin::{Expander, Native};
    use crate::project::{self, Host, Project, config};
    use flick_test_pairs::Pairs;

    struct Memory(BTreeMap<PathBuf, String>);
    impl Host for Memory {
        fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
            self.0
                .get(path)
                .map(|text| text.as_bytes().to_vec())
                .ok_or_else(|| ErrorKind::NotFound.into())
        }
        fn display(&self, path: &Path) -> String {
            path.display().to_string()
        }
        fn plugin(&self, package: &str, _: Option<&Path>) -> Result<Arc<dyn Expander>, String> {
            if package == "pairs" {
                Ok(Arc::new(Native::new("pairs", Pairs)))
            } else {
                Err(format!("no plugin for {package}"))
            }
        }
    }
    let manifest = "schema-version = 1\n[project]\nkind = \"lib\"\n[dependencies]\npairs = { path = \"/pairs\" }\n";
    let host = Memory(
        [
            (PathBuf::from("/p/flick.toml"), manifest.to_owned()),
            (PathBuf::from("/p/src/App/Use.fl"), user.to_owned()),
            (
                PathBuf::from("/pairs/flick.toml"),
                "schema-version = 1\n[project]\nkind = \"lib\"\n".to_owned(),
            ),
            (PathBuf::from("/pairs/src/Pairs.fl"), pairs.to_owned()),
        ]
        .into(),
    );
    let mut sources = SourceMap::default();
    let config_file = sources.add("/p/flick.toml", manifest);
    let project = Project {
        root: "/p".into(),
        config: config::parse(manifest, config_file).expect("manifest"),
        config_file,
        workspace: None,
    };
    let analysis = project::analyze(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/App/Use.fl")],
    );
    (analysis, sources)
}

#[cfg(feature = "native-project")]
#[test]
fn class_rename_refuses_an_unmapped_plugin_generated_instance_binder() {
    use super::{Ide, tree};
    use crate::syntax::kind::SyntaxKind as K;

    let pairs = "module Pairs exposing (..)\n@derive pairs\nclass Arbitrary a =\n  arbitrary : a\n";
    let user = "module App.Use exposing (..)\nimport Pairs\ntype Token = Token\n  deriving Pairs.Arbitrary\nvalue : Token\nlet value = Pairs.arbitrary\n";
    let (analysis, sources) = pairs_fixture(user, pairs);
    let errors: Vec<_> = analysis
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
    let use_module = analysis.module("App.Use").expect("derived instance module");
    let instance = use_module
        .typing
        .instances
        .iter()
        .find(|i| i.class == ("Pairs".to_owned(), "Arbitrary".to_owned()))
        .expect("generated instance");
    assert!(
        tree::find(&use_module.tree, instance.span).is_none_or(|n| n.kind != K::InstanceDecl),
        "the instance binder is generated, not a written source declaration"
    );

    // Even updating every written method spelling leaves the plugin's
    // hard-coded `let arbitrary` field inconsistent with class `sample`.
    let renamed_pairs = pairs.replace("arbitrary", "sample");
    let renamed_user = user.replace("Pairs.arbitrary", "Pairs.sample");
    let (updated, _) = pairs_fixture(&renamed_user, &renamed_pairs);
    assert!(
        updated.diagnostics.iter().any(|d| d.code == codes::C011),
        "{:?}",
        updated.diagnostics
    );

    let declared = analysis.module("Pairs").expect("class module");
    let offset = u32::try_from(pairs.find("arbitrary :").expect("method")).expect("small") + 1;
    let ide = Ide {
        analysis: &analysis,
        sources: &sources,
    };
    assert!(
        ide.rename(declared.file, offset, "sample").is_err(),
        "generated instance fields have no exact source mapping and must not be omitted from rename"
    );
}

#[cfg(feature = "native-project")]
#[test]
fn class_rename_refuses_reverse_capture_in_a_plugin_opening_without_old_calls() {
    use super::Ide;

    let header = "module App.Use exposing (..)\nimport Pairs exposing (plugin opens)\n";
    let user = format!(
        "{header}render : Int -> String\nlet render _ = \"file\"\nlet result = opens {{ render 1 }}\n"
    );
    let (analysis, sources) = opening_fixture(&user);
    let declared = analysis.module("Pairs").expect("class module");
    let offset = u32::try_from(
        sources
            .get(declared.file)
            .text
            .find("describe :")
            .expect("method"),
    )
    .expect("small")
        + 1;
    let ide = Ide {
        analysis: &analysis,
        sources: &sources,
    };
    assert!(
        ide.rename(declared.file, offset, "display_item").is_ok(),
        "a fresh opened name cannot capture render"
    );
    assert!(
        ide.rename(declared.file, offset, "render").is_err(),
        "the opening would capture the file's existing render even without old-method calls"
    );
}

#[cfg(feature = "native-project")]
#[test]
fn class_rename_allows_reverse_opening_locals_qualified_calls_and_unused_names() {
    use super::Ide;

    let header = "module App.Use exposing (..)\nimport Pairs exposing (plugin opens)\n";
    for body in [
        "result : (Int -> String) -> String\nlet result render = opens { render 1 }\n",
        "render : Int -> String\nlet render _ = \"file\"\nlet result = opens { Pairs.describe 1 }\n",
        "let result = opens { \"constant\" }\n",
    ] {
        let (analysis, sources) = opening_fixture(&format!("{header}{body}"));
        let declared = analysis.module("Pairs").expect("class module");
        let offset = u32::try_from(
            sources
                .get(declared.file)
                .text
                .find("describe :")
                .expect("method"),
        )
        .expect("small")
            + 1;
        let ide = Ide {
            analysis: &analysis,
            sources: &sources,
        };
        assert!(
            ide.rename(declared.file, offset, "render").is_ok(),
            "locals, qualified calls and unused names are safe: {body}"
        );
    }
}
