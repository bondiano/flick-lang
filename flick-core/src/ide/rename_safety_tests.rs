//! Renames must not change bindings or leave unchecked/structural uses behind.

use super::tests::{Fixture, fixture_of};
use crate::diagnostics::{Severity, codes};
#[cfg(feature = "native-project")]
use crate::project::Analysis;
#[cfg(feature = "native-project")]
use crate::source::SourceMap;
use crate::types::Ref;

mod contracts;

const NAMED: &str = "module App.Named exposing (Named(..), describe)
class Named a =
  describe : a -> String
instance Named Int =
  let describe _ = \"value\"
";

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
fn class_rename_refuses_unpack_payload_capture() {
    let contract =
        "module App.BoxContracts exposing (Box)\ntype module Box =\n  type Key\n  value : Key\n";
    let payload = "module App.Payload exposing (Key, value)\ntype alias Key = Int\nvalue : Key\nlet value = 1\n";
    let packed = "module App.Packed exposing (value)\nimport App.BoxContracts exposing (Box)\nimport App.Payload as P\nvalue : exists K : Box. K.Key\nlet value = unpack (pack P as Box) as K in K.value\n";
    let user = "module App.Use exposing (answer)\nimport App.Packed as P\nimport App.Named exposing (describe)\nanswer : String\nlet answer = unpack P.value as K, render in describe 1\n";
    let f = fixture_of(&[
        ("src/App/Named.fl", NAMED),
        ("src/App/BoxContracts.fl", contract),
        ("src/App/Payload.fl", payload),
        ("src/App/Packed.fl", packed),
        ("src/App/Use.fl", user),
    ]);
    assert_clean(&f);
    assert_refused(&f, "render");
}

#[test]
fn class_rename_refuses_qualified_dictionary_dispatch() {
    let user = "module App.Use exposing (value)\nimport App.Named as Named exposing (Named)\nvalue : String\nlet value = Named.describe 1\n";
    let f = fixture_of(&[("src/App/Named.fl", NAMED), ("src/App/Use.fl", user)]);
    assert_clean(&f);
    let named = NAMED.replace("describe", "dictionary");
    let user = user.replace("Named.describe", "Named.dictionary");
    let updated = fixture_of(&[("src/App/Named.fl", &named), ("src/App/Use.fl", &user)]);
    assert!(
        !updated
            .ide()
            .analysis
            .module("App.Use")
            .expect("use module")
            .typing
            .dictionary_constructors
            .is_empty(),
        "the renamed qualifier starts class-dictionary dispatch rather than the method"
    );
    assert_refused(&f, "dictionary");
}

#[test]
fn class_rename_refuses_qualified_service_operation_dispatch() {
    let user = "module App.Use exposing (value)\nimport App.Named as Clock\nservice Clock =\n  render : Int ->{} String\nvalue : String\nlet value = Clock.describe 1\n";
    let f = fixture_of(&[("src/App/Named.fl", NAMED), ("src/App/Use.fl", user)]);
    assert_clean(&f);
    let named = NAMED.replace("describe", "render");
    let user = user.replace("Clock.describe", "Clock.render");
    let updated = fixture_of(&[("src/App/Named.fl", &named), ("src/App/Use.fl", &user)]);
    assert!(
        updated
            .ide()
            .analysis
            .module("App.Use")
            .expect("use module")
            .typing
            .names
            .values()
            .any(|r| matches!(r, Ref::Operation { name, .. } if name == "render")),
        "the service operation takes precedence over the module's renamed method"
    );
    assert_refused(&f, "render");
}

#[test]
fn class_rename_allows_qualified_dictionary_method_through_nonclass_alias() {
    let user = "module App.Use exposing (value)\nimport App.Named as Other\ntype alias Other = Int\nvalue : String\nlet value = Other.describe 1\n";
    let f = fixture_of(&[("src/App/Named.fl", NAMED), ("src/App/Use.fl", user)]);
    assert_clean(&f);
    assert!(
        f.ide()
            .rename(
                f.file("App.Named"),
                f.at("App.Named", "describe :", 0) + 1,
                "dictionary"
            )
            .is_ok(),
        "a non-class type with the module alias's spelling cannot trigger dictionary construction"
    );
    let named = NAMED.replace("describe", "dictionary");
    let user = user.replace("Other.describe", "Other.dictionary");
    let updated = fixture_of(&[("src/App/Named.fl", &named), ("src/App/Use.fl", &user)]);
    assert_clean(&updated);
    assert!(
        updated
            .ide()
            .analysis
            .module("App.Use")
            .expect("use module")
            .typing
            .dictionary_constructors
            .is_empty()
    );
}

#[test]
fn class_rename_refuses_control_handler_resume_capture() {
    let user = "module App.Use exposing (run)\nimport App.Named exposing (describe)\neffect Ask =\n  control ask : Unit ->{} String\nrun : Unit ->{} String\nlet run () =\n  try Ask.ask () with\n  | Ask.ask () -> describe 1\n";
    let f = fixture_of(&[("src/App/Named.fl", NAMED), ("src/App/Use.fl", user)]);
    assert_clean(&f);
    assert_refused(&f, "resume");
}

#[test]
fn class_rename_allows_resume_name_in_a_control_handler_guard() {
    let user = "module App.Use exposing (run)\nimport App.Named exposing (describe)\neffect Ask =\n  control ask : Unit ->{} String\nrun : Unit ->{} String\nlet run () =\n  try Ask.ask () with\n  | Ask.ask () when describe 1 == \"value\" -> resume \"value\"\n  | Ask.ask () -> resume \"fallback\"\n";
    let f = fixture_of(&[("src/App/Named.fl", NAMED), ("src/App/Use.fl", user)]);
    assert_clean(&f);
    assert!(
        f.ide()
            .rename(
                f.file("App.Named"),
                f.at("App.Named", "describe :", 0) + 1,
                "resume"
            )
            .is_ok(),
        "resume is introduced only for the arm body, not its guard"
    );
    let named = NAMED.replace("describe", "resume");
    let user = user.replace("describe", "resume");
    assert_clean(&fixture_of(&[
        ("src/App/Named.fl", &named),
        ("src/App/Use.fl", &user),
    ]));
}

#[test]
fn class_rename_refuses_private_component_collision() {
    use crate::iface::Namespace;

    let user = "module App.Use exposing (result)\nimport App.Named exposing (describe)\ncomponent render_widget (label : String) = ui { text label }\nresult : String\nlet result = describe 1\n";
    let f = fixture_of(&[("src/App/Named.fl", NAMED), ("src/App/Use.fl", user)]);
    assert_clean(&f);
    let module = f.ide().analysis.module("App.Use").expect("use module");
    assert!(
        module
            .interface
            .find(Namespace::Value, "render_widget")
            .is_none(),
        "the component must be private in this regression"
    );
    assert!(
        module
            .expansion
            .as_ref()
            .expect("component expansion")
            .declarations()
            .iter()
            .any(|d| d.name == "render_widget"),
        "the private value is a real expansion declaration"
    );
    assert_refused(&f, "render_widget");
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
