use super::*;
use crate::diagnostics::{Diagnostic, codes};
use crate::source::SourceMap;
use crate::types::{Thresholds, dialect};

fn parse_text(text: &str) -> Result<Config, Diagnostic> {
    let mut sources = SourceMap::default();
    let file = sources.add("flick.toml", text);
    parse(text, file)
}

#[test]
fn spec_example_parses_with_defaults() {
    let config = parse_text(
        "schema-version = 1\n[project]\nkind = \"app\"\nsource-dir = \"src\"\ntest-dir = \"test\"\n\
         entry = \"App.Main\"\n[build]\nout-dir = \"dist\"\nprofile = \"debug\"\nemit = \"js\"\n\
         schema-aot = false\nlabels = \"all\"\n",
    )
    .expect("valid");
    assert_eq!(config.kind, Kind::App);
    let named: Vec<_> = config
        .entries
        .iter()
        .map(|e| (e.name.as_str(), e.module.as_str()))
        .collect();
    assert_eq!(named, [("main", "App.Main")]);
    assert_eq!(config.build, Build::default());
    let lib = parse_text("schema-version = 1\n[project]\nkind = \"lib\"\n").expect("valid");
    assert_eq!(
        (lib.source_dir.as_str(), lib.test_dir.as_str()),
        ("src", "test")
    );
    assert_eq!(lib.sql_dialect.name, "postgresql");
    assert!(!lib.no_let_assert);
    let strict =
        parse_text("schema-version = 1\n[project]\nkind = \"lib\"\n[lint]\nno-let-assert = true\n");
    assert!(strict.expect("valid").no_let_assert);
    let components = parse_text(
        "schema-version = 1\n[project]\nkind = \"lib\"\n[build]\nlabels = \"components\"\n",
    );
    assert_eq!(components.expect("valid").build.labels, Labels::Components);
}

#[test]
fn typescript_settings_require_the_capability() {
    for setting in ["emit = \"ts\"", "emit-dts = true", "promise-facade = true"] {
        let text = format!("schema-version = 1\n[project]\nkind = \"lib\"\n[build]\n{setting}\n");
        #[cfg(feature = "typescript")]
        assert!(parse_text(&text).is_ok(), "{setting}");
        #[cfg(not(feature = "typescript"))]
        assert_eq!(parse_text(&text).expect_err(setting).code, codes::P005);
    }
}

#[test]
fn several_entries_by_name() {
    let config = parse_text(
        "schema-version = 1\n[project]\nkind = \"app\"\nentries = { server = \"App.Server\", cli-2 = \"App.Cli\" }\n",
    )
    .expect("valid");
    let named: Vec<_> = config
        .entries
        .iter()
        .map(|e| (e.name.as_str(), e.module.as_str()))
        .collect();
    assert_eq!(named, [("cli-2", "App.Cli"), ("server", "App.Server")]);
}

#[test]
fn an_entry_takes_its_runner_or_the_project_s() {
    let config = parse_text(
        "schema-version = 1\n[project]\nkind = \"app\"\nrunner = \"Runner.Node\"\nentries = { web = { module = \"App.Web\", runner = \"Runner.Cloudflare\" }, cli = \"App.Cli\" }\n",
    )
    .expect("valid");
    let runners: Vec<_> = config
        .entries
        .iter()
        .map(|e| (e.module.as_str(), e.runner.as_ref().map(|r| r.0.as_str())))
        .collect();
    assert_eq!(
        runners,
        [
            ("App.Cli", Some("Runner.Node")),
            ("App.Web", Some("Runner.Cloudflare"))
        ]
    );
    let err = parse_text(
        "schema-version = 1\n[project]\nkind = \"app\"\nentries = { web = { module = \"App.Web\", host = \"x\" } }\n",
    )
    .expect_err("unknown key");
    assert!(
        err.message.starts_with("entry `web` is a module name"),
        "{}",
        err.message
    );
}

#[test]
fn an_entry_runs_on_a_server_unless_its_target_is_the_client() {
    let config = parse_text(
        "schema-version = 1\n[project]\nkind = \"app\"\nentries = { web = { module = \"App.Web\", target = \"client\" }, api = { module = \"App.Api\", target = \"server\" }, cli = \"App.Cli\" }\n",
    )
    .expect("valid");
    let clients: Vec<_> = config
        .entries
        .iter()
        .map(|e| (e.name.as_str(), e.client))
        .collect();
    assert_eq!(clients, [("api", false), ("cli", false), ("web", true)]);
    let err = parse_text(
        "schema-version = 1\n[project]\nkind = \"app\"\nentries = { web = { module = \"App.Web\", target = \"browser\" } }\n",
    )
    .expect_err("unknown target");
    assert_eq!(err.code, codes::P005);
}

#[test]
fn sql_dialect() {
    let pg = parse_text(
        "schema-version = 1\n[project]\nkind = \"lib\"\n[sql]\ndialect = \"postgresql\"\n",
    )
    .expect("valid");
    assert_eq!(pg.sql_dialect, &dialect::POSTGRESQL);
    let err =
        parse_text("schema-version = 1\n[project]\nkind = \"lib\"\n[sql]\ndialect = \"oracle\"\n")
            .expect_err("unsupported");
    assert_eq!(
        err.message,
        "the SQL dialect `oracle` is not supported yet; supported: `postgresql`"
    );
    let err = parse_text(
        "schema-version = 1\n[project]\nkind = \"lib\"\n[sql]\ndialect = \"postgres\"\n",
    )
    .expect_err("unknown");
    assert_eq!(
        err.message,
        "unknown SQL dialect `postgres`; known: `postgresql`, `mysql`, `sqlite`, `clickhouse`, `oracle`"
    );
}

#[test]
fn rejects_with_spans() {
    let cases = [
        (
            "schema-version = 1\n[project]\nkind = \"lib\"\nsrc = \"x\"\n",
            codes::P005,
            "src",
        ),
        (
            "schema-version = 1\n[project]\nkind = \"tool\"\n",
            codes::P005,
            "\"tool\"",
        ),
        (
            "schema-version = 1\n[project]\nkind = \"app\"\n",
            codes::P007,
            "",
        ),
        (
            "schema-version = 2\n[project]\nkind = \"lib\"\n",
            codes::P006,
            "2",
        ),
        (
            "schema-version = 1\n[project]\nkind = \"app\"\nentry = \"A\"\nentries = { b = \"B\" }\n",
            codes::P005,
            "{ b = \"B\" }",
        ),
        (
            "schema-version = 1\n[project]\nkind = \"app\"\nentries = { Server = \"B\" }\n",
            codes::P005,
            "{ Server = \"B\" }",
        ),
        (
            "schema-version = 1\n[project]\nkind = \"app\"\nentries = {}\n",
            codes::P007,
            "{}",
        ),
        ("[project]\nkind = \"lib\"\n", codes::P006, ""),
        (
            "schema-version = 1\n[build]\nprofile = \"fast\"\n[project]\nkind = \"lib\"\n",
            codes::P005,
            "\"fast\"",
        ),
        ("schema-version = 1\n[project\n", codes::P005, ""),
        (
            "schema-version = 1\n[project]\nkind = \"lib\"\n[lint]\nallow = [\"FLICK-T999\"]\n",
            codes::P005,
            "\"FLICK-T999\"",
        ),
        (
            "schema-version = 1\n[project]\nkind = \"lib\"\n[sql]\ndialect = \"mysql\"\n",
            codes::P022,
            "\"mysql\"",
        ),
        (
            "schema-version = 1\n[project]\nkind = \"lib\"\n[sql]\ndialect = \"pg\"\n",
            codes::P023,
            "\"pg\"",
        ),
        (
            "schema-version = 1\n[project]\nkind = \"lib\"\n[sql]\nname = \"x\"\n",
            codes::P005,
            "name",
        ),
    ];
    for (text, code, spanned) in cases {
        let err = parse_text(text).expect_err(text);
        assert_eq!(err.code, code, "{text}: {}", err.message);
        if !spanned.is_empty() {
            let span = err.span.expect("has a span");
            assert_eq!(&text[span.range()], spanned, "{text}");
        }
    }
}

#[test]
fn lint_thresholds_default_and_override() {
    let lib = |lint: &str| {
        parse_text(&format!(
            "schema-version = 1\n[project]\nkind = \"lib\"\n[lint]\n{lint}"
        ))
    };
    assert_eq!(lib("").expect("valid").thresholds, Thresholds::default());
    let set = lib("max-function-lines = 30\nmax-nesting = 3\nmax-complexity = 7\nmax-params = 4\n");
    assert_eq!(
        set.expect("valid").thresholds,
        Thresholds {
            function_lines: 30,
            nesting: 3,
            complexity: 7,
            params: 4,
        }
    );
    let zero = lib("max-nesting = 0\n").expect_err("not positive");
    assert_eq!(zero.code, codes::P005);
    assert!(
        zero.message
            .contains("`[lint] max-nesting` is a positive integer")
    );
}

fn manifest(text: &str) -> Result<Manifest, Diagnostic> {
    let mut sources = SourceMap::default();
    let file = sources.add("flick.toml", text);
    parse_manifest(text, file)
}

#[test]
fn a_workspace_root_names_members_and_scripts() {
    let root = manifest(
        "schema-version = 1\n[workspace]\nmembers = [\"contract\", \"server\"]\n\
         [scripts]\nlint = [\"flick fmt --check\", \"flick check\"]\nci = [\"@lint\", \"flick test\"]\ntest = \"flick test\"\n",
    )
    .expect("valid");
    assert_eq!(root.project, None);
    assert_eq!(
        root.members,
        Some(vec!["contract".to_owned(), "server".to_owned()])
    );
    let scripts: Vec<_> = root
        .scripts
        .iter()
        .map(|s| (s.name.as_str(), s.steps.len()))
        .collect();
    assert_eq!(scripts, [("ci", 2), ("lint", 2), ("test", 1)]);
    let err =
        parse_text("schema-version = 1\n[workspace]\nmembers = []\n").expect_err("no project");
    assert_eq!(err.code, codes::P031);
}

#[test]
fn a_project_depends_on_libraries_by_path() {
    let config = parse_text(
        "schema-version = 1\n[project]\nkind = \"app\"\nentry = \"Main\"\n[dependencies]\ncontract = { path = \"../contract\" }\n",
    )
    .expect("valid");
    let deps: Vec<_> = config
        .dependencies
        .iter()
        .map(|d| (d.name.as_str(), d.path().unwrap_or_default()))
        .collect();
    assert_eq!(deps, [("contract", "../contract")]);
}

#[test]
fn rejects_invalid_workspaces_and_scripts() {
    for (text, code) in [
        ("schema-version = 1\n", codes::P005),
        (
            "schema-version = 1\n[workspace]\nmembers = []\n[build]\nemit = \"go\"\n",
            codes::P005,
        ),
        (
            "schema-version = 1\n[workspace]\nmembers = []\n[scripts]\nci = [\"@lint\"]\n",
            codes::P005,
        ),
        (
            "schema-version = 1\n[workspace]\nmembers = []\nexclude = []\n",
            codes::P005,
        ),
        (
            "schema-version = 1\n[project]\nkind = \"lib\"\n[dependencies]\nx = { version = \"1\" }\n",
            codes::P005,
        ),
    ] {
        let err = manifest(text).expect_err(text);
        assert_eq!(err.code, code, "{text}: {}", err.message);
    }
}

#[test]
fn a_package_names_its_plugin_inside_itself() {
    let package = |plugin: &str| {
        parse_text(&format!(
            "schema-version = 1\n[package]\nname = \"pairs\"\nversion = \"1.0.0\"\nplugin = \"{plugin}\"\n[project]\nkind = \"lib\"\n"
        ))
    };
    let config = package("plugin/pairs.wasm").expect("valid");
    let plugin = config.package.and_then(|p| p.plugin);
    assert_eq!(plugin.as_deref(), Some("plugin/pairs.wasm"));
    for outside in ["../pairs.wasm", "/tmp/pairs.wasm", "a/../../b.wasm"] {
        let refused = package(outside).expect_err(outside);
        assert_eq!(refused.code, codes::P032, "{outside}");
    }
}
