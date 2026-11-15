use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::target_preflight::{self, gated_service, handles};
use super::{Extras, Settings};
use crate::codegen::EffectBridge;
use crate::diagnostics::{Diagnostic, codes};
use crate::ir::tests::check_text;
use crate::project::config::{Backend, BackendEnvironment};
use crate::project::{Compilation, Compiled};
use crate::source::SourceMap;

const DB_PROVIDED: &str = r#"module App.Main exposing (main)

service Db =
  query : String ->{} String

main : Unit ->{} Unit
let main () =
  provide Db ({ query = fun key -> key }) {
    ignore (Db.query "x")
  }
"#;

const DB_UNUSED: &str = r"module App.Main exposing (main)

service Db =
  query : String ->{} String

unused : String ->{Db} String
let unused key = Db.query key

main : Unit ->{} Unit
let main () = ()
";

const DB_EXPORT: &str = r"module App.Main exposing (run)

service Db =
  query : String ->{} String

run : String ->{Db} String
let run key = Db.query key
";

fn environment(pair: &str) -> BackendEnvironment {
    pair.parse().expect("environment")
}

fn diagnostics(text: &str, pair: &str, app: bool) -> Vec<Diagnostic> {
    let (ir, interface) = check_text("App.Main", text).expect("the source checks");
    let mut sources = SourceMap::default();
    let file = sources.add("App/Main.fl", text);
    let compilation = Compilation {
        modules: vec![Compiled {
            name: ir.name.clone(),
            path: PathBuf::from("/app/src/App/Main.fl"),
            file,
            test: false,
            package: None,
            imports: vec![],
            ir,
            declarations: Arc::new(interface.clone()),
            interface,
            expansion: None,
        }],
        ..Compilation::default()
    };
    let entries = [("main".into(), "App.Main".into())];
    let env = environment(pair);
    let settings = Settings {
        package: None,
        entries: if app { &entries } else { &[] },
        typescript: false,
        release: false,
        extras: Extras::default(),
        source_path: &|_| "App/Main.fl".into(),
        source_dir: &|_| None,
        out_dir: Path::new("/app/dist"),
        root: None,
        backend: Backend::Rt,
        effect_bridge: EffectBridge::Absent,
        ui: None,
        runners: &[],
        environment: Some(&env),
    };
    let mut found = Vec::new();
    target_preflight::check(&compilation, &settings, &sources, &env, &mut found);
    found
}

#[test]
fn inventory_handles_database_on_node_not_browser() {
    assert!(handles(&environment("js/node"), "Db"));
    assert!(handles(&environment("js/node"), "Database"));
    assert!(!handles(&environment("js/browser"), "Db"));
    assert_eq!(gated_service("App.Main.Db"), Some("Db"));
    assert_eq!(gated_service("Database.Database"), Some("Database"));
}

#[test]
fn browser_rejects_a_reachable_unhandled_db_at_its_use() {
    // Open Db on main is refused by entry typing; a library export is the
    // reachable unhandled case with an exact requirement span.
    let source = DB_EXPORT;
    let found = diagnostics(source, "js/browser", false);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].code, codes::E020);
    assert_eq!(found[0].message, "unhandled effect Db on js/browser");
    let span = found[0].span.expect("requirement span");
    assert_eq!(&source[span.start as usize..span.end as usize], "Db.query");
}

#[test]
fn node_accepts_the_same_reachable_db_export() {
    let found = diagnostics(DB_EXPORT, "js/node", false);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn browser_accepts_a_locally_provided_db() {
    let found = diagnostics(DB_PROVIDED, "js/browser", true);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn unused_db_does_not_fail_a_reachable_only_executable() {
    // main cannot leave Db open; unused stays unreachable from main.
    let found = diagnostics(DB_UNUSED, "js/browser", true);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_library_export_policy_checks_public_db_use() {
    let found = diagnostics(DB_EXPORT, "js/browser", false);
    assert_eq!(found[0].code, codes::E020);
}
