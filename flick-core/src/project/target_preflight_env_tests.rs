//! Regression tests for re-exporting facade gating, environment pruning, and
//! frontend-only hosts without backend descriptor loading.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::ir::{Expr, ExprKind, reach};
use crate::project::{BackendTags, Host, Project, check, compile, discover};
use crate::source::SourceMap;

struct MemHost {
    files: BTreeMap<PathBuf, Vec<u8>>,
    backends: Vec<BackendTags>,
    load_backends: bool,
}

impl MemHost {
    fn new(files: &[(&str, &str)], backends: Vec<BackendTags>) -> Self {
        Self {
            files: files
                .iter()
                .map(|(p, t)| (PathBuf::from(p), t.as_bytes().to_vec()))
                .collect(),
            backends,
            load_backends: true,
        }
    }

    fn without_backends(files: &[(&str, &str)]) -> Self {
        Self {
            files: files
                .iter()
                .map(|(p, t)| (PathBuf::from(p), t.as_bytes().to_vec()))
                .collect(),
            backends: Vec::new(),
            load_backends: false,
        }
    }
}

impl Host for MemHost {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.display().to_string()))
    }

    fn display(&self, path: &Path) -> String {
        path.display().to_string()
    }

    fn backend_tags(&self, _project: &Project) -> Vec<BackendTags> {
        self.backends.clone()
    }

    fn loads_backends(&self) -> bool {
        self.load_backends
    }
}

fn check_files(files: &[(&str, &str)], roots: &[&str]) -> (Vec<Diagnostic>, SourceMap) {
    let host = MemHost::new(files, vec![]);
    let mut sources = SourceMap::default();
    let project = discover(&host, &mut sources, Path::new("/app"), None)
        .expect("manifest parses")
        .expect("project found");
    let roots: Vec<PathBuf> = roots.iter().map(PathBuf::from).collect();
    let diags = check(&host, &mut sources, Some(&project), &roots);
    (diags, sources)
}

const APP_JS_TOML: &str = "\
schema-version = 1
[project]
kind = \"app\"
entry = \"App.Main\"
[build]
environment = \"js/node\"
";

const SHARED_MOD: &str = "\
module Shared exposing (value, Box(..), S, main)
type Box = Box Int
service S =
  ping : Unit ->{} Int
value : Int
let value = 42
main : Unit -> Unit
let main () = ()
";

const FACADE_GO_MOD: &str = "\
@when \"go\"
module Facade exposing (value, Box(..), S, main)
import Shared exposing (value, Box(..), S, main)
";

const OUTER_MOD: &str = "\
module Outer exposing (value)
import Facade exposing (value)
";

fn facade_project(main_src: &str) -> Vec<(&str, &str)> {
    vec![
        ("/app/flick.toml", APP_JS_TOML),
        ("/app/src/Shared.fl", SHARED_MOD),
        ("/app/src/Facade.fl", FACADE_GO_MOD),
        ("/app/src/Outer.fl", OUTER_MOD),
        ("/app/src/App/Main.fl", main_src),
    ]
}

#[test]
fn reexport_facade_with_when_rejects_reachable_uses_and_allows_unreachable() {
    // 1. Unused import & unreachable helper using gated facade must pass.
    let unused = "\
module App.Main exposing (main)
import Facade
import Outer
dead : Unit -> Int
let dead () = Facade.value + Outer.value
main : Unit -> Unit
let main () = ()
";
    let (diags, _) = check_files(&facade_project(unused), &["/app/src/App/Main.fl"]);
    assert!(
        !diags.iter().any(|d| d.severity == Severity::Error),
        "unused facade reference must not fail: {diags:?}"
    );

    // 2. Qualified 1-hop, aliased qualified, unqualified explicit, unqualified open, and 2-hop facade uses fail with P064.
    let cases = [
        (
            "qualified",
            "module App.Main exposing (main)\nimport Facade\nmain : Unit -> Unit\nlet main () =\n  let _ = Facade.value\n  ()\n",
            "Facade.value",
        ),
        (
            "aliased",
            "module App.Main exposing (main)\nimport Facade as F\nmain : Unit -> Unit\nlet main () =\n  let _ = F.value\n  ()\n",
            "F.value",
        ),
        (
            "unqualified_explicit",
            "module App.Main exposing (main)\nimport Facade exposing (value)\nmain : Unit -> Unit\nlet main () =\n  let _ = value\n  ()\n",
            "value",
        ),
        (
            "unqualified_open",
            "module App.Main exposing (main)\nimport Facade exposing (..)\nmain : Unit -> Unit\nlet main () =\n  let _ = value\n  ()\n",
            "value",
        ),
        (
            "two_hop",
            "module App.Main exposing (main)\nimport Outer\nmain : Unit -> Unit\nlet main () =\n  let _ = Outer.value\n  ()\n",
            "Outer.value",
        ),
        (
            "constructor_and_pattern",
            "module App.Main exposing (main)\nimport Facade exposing (Box(..))\nmain : Unit -> Unit\nlet main () =\n  let _ = match Box 1 with\n    | Box n -> n\n  ()\n",
            "Box",
        ),
        (
            "reexported_entry",
            "module App.Main exposing (main)\nimport Facade exposing (main)\n",
            "main",
        ),
    ];

    for (label, src, expected_slice) in cases {
        let (diags, sources) = check_files(&facade_project(src), &["/app/src/App/Main.fl"]);
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(!errors.is_empty(), "{label}: expected FLICK-P064, got none");
        let p064 = errors
            .iter()
            .find(|d| d.code == codes::P064)
            .unwrap_or_else(|| panic!("{label}: expected FLICK-P064 in {errors:?}"));
        assert!(
            p064.message.contains("`Facade`"),
            "{label}: expected Facade in {}",
            p064.message
        );
        let span = p064.span.expect("use span");
        assert_eq!(
            &sources.get(span.file).text[span.range()],
            expected_slice,
            "{label}"
        );
    }
}

#[test]
fn package_when_facade_reexporting_unconstrained_package_fails_on_reachable_use() {
    let files = [
        (
            "/app/flick.toml",
            "schema-version = 1\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n\
             [dependencies]\ngo-facade = { path = \"../go-facade\" }\n\
             [build]\nenvironment = \"js/node\"\n",
        ),
        (
            "/shared/flick.toml",
            "schema-version = 1\n[package]\nname = \"shared-pkg\"\nversion = \"0.1.0\"\n\
             [project]\nkind = \"lib\"\n",
        ),
        (
            "/shared/src/SharedPkg.fl",
            "module SharedPkg exposing (answer)\nanswer : Int\nlet answer = 7\n",
        ),
        (
            "/go-facade/flick.toml",
            "schema-version = 1\n[package]\nname = \"go-facade\"\nversion = \"0.1.0\"\n\
             when = \"go\"\n[project]\nkind = \"lib\"\n\
             [dependencies]\nshared-pkg = { path = \"../shared\" }\n",
        ),
        (
            "/go-facade/src/GoFacade.fl",
            "module GoFacade exposing (answer)\nimport SharedPkg exposing (answer)\n",
        ),
        (
            "/app/src/App/Main.fl",
            "module App.Main exposing (main)\nimport GoFacade\n\
             main : Unit -> Unit\nlet main () =\n  let _ = GoFacade.answer\n  ()\n",
        ),
    ];
    let (diags, sources) = check_files(&files, &["/app/src/App/Main.fl"]);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, codes::P064);
    assert!(
        errors[0].message.contains("go-facade") && errors[0].message.contains("GoFacade"),
        "{}",
        errors[0].message
    );
    let span = errors[0].span.expect("span");
    assert_eq!(
        &sources.get(span.file).text[span.range()],
        "GoFacade.answer"
    );
}

#[test]
fn prune_for_environment_removes_gated_modules_and_transitively_tainted_dead_items() {
    let files = [
        (
            "/app/flick.toml",
            "schema-version = 1\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n\
             [build]\nenvironment = \"js/browser\"\n",
        ),
        (
            "/app/src/NodeOnly.fl",
            "@when \"js && node\"\nmodule NodeOnly exposing (secret)\n\
             secret : Unit -> String\nlet secret () = \"env\"\n",
        ),
        (
            "/app/src/GoOnly.fl",
            "@when \"go\"\nmodule GoOnly exposing (native_id)\n\
             native_id : Int\nlet native_id = 1\n",
        ),
        (
            "/app/src/App/Main.fl",
            "module App.Main exposing (main, dead_outer)\n\
             import GoOnly\nimport NodeOnly\n\
             dead_inner : Unit -> String\nlet dead_inner () = NodeOnly.secret ()\n\
             dead_outer : Unit -> String\nlet dead_outer () = dead_inner ()\n\
             main : Unit -> Unit\nlet main () = ()\n",
        ),
    ];
    let host = MemHost::new(&files, vec![]);
    let mut sources = SourceMap::default();
    let project = discover(&host, &mut sources, Path::new("/app"), None)
        .expect("manifest")
        .expect("project");
    let (diags, compilation) = compile(
        &host,
        &mut sources,
        Some(&project),
        &[
            PathBuf::from("/app/src/App/Main.fl"),
            PathBuf::from("/app/src/NodeOnly.fl"),
            PathBuf::from("/app/src/GoOnly.fl"),
        ],
    );
    assert!(
        !diags.iter().any(|d| d.severity == Severity::Error),
        "{diags:?}"
    );
    let pruned = compilation.prune_for_environment(&sources, true);
    let names: Vec<&str> = pruned.modules.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, ["App.Main"]);
    let app = &pruned.modules[0];
    let item_names: Vec<String> = app.ir.items.iter().flat_map(reach::defined).collect();
    assert_eq!(item_names, ["main"]);
    assert!(app.imports.is_empty(), "{:?}", app.imports);
    assert_eq!(
        app.ir
            .exports
            .values
            .iter()
            .map(|e| e.name.as_str())
            .collect::<Vec<_>>(),
        ["main"]
    );
}

#[test]
fn host_without_backends_skips_tag_validation_for_uncached_custom_backend() {
    let sha = "a".repeat(64);
    let toml = format!(
        "schema-version = 1\n[project]\nkind = \"app\"\nentry = \"App\"\n\
         [dependencies]\nhost-alpha = {{ path = \"../host-alpha\" }}\n\
         [build]\nenvironment = \"literal/alpha-host\"\n\
         [build.backend]\nid = \"test/literal\"\nversion = \"1.0.0\"\n\
         sha256 = \"{sha}\"\ncomponent = \"backend.wasm\"\n"
    );
    let host_toml = "\
schema-version = 1
[package]
name = \"host-alpha\"
version = \"0.1.0\"
[project]
kind = \"lib\"
[host]
name = \"alpha-host\"
backend = \"literal\"
requires = \"alpha\"
tags = [\"console\"]
";
    let app_src = "@when \"literal && alpha && console\"\nmodule App exposing (main)\n\
        main : Unit -> Unit\nlet main () = ()\n";
    let host = MemHost::without_backends(&[
        ("/app/flick.toml", &toml),
        ("/host-alpha/flick.toml", host_toml),
        ("/app/src/App.fl", app_src),
    ]);
    let mut sources = SourceMap::default();
    let project = discover(&host, &mut sources, Path::new("/app"), None)
        .expect("manifest")
        .expect("project");
    let (diags, compilation) = compile(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/app/src/App.fl")],
    );
    assert!(
        !diags.iter().any(|d| d.severity == Severity::Error),
        "uncached custom backend without backend loading must not emit false errors: {diags:?}"
    );
    assert!(compilation.environment.is_none());
    assert!(compilation.modules.iter().any(|m| m.name == "App"));
}

#[test]
fn locally_handled_helper_passes_e020_while_unhandled_call_still_fails() {
    let browser_toml = "\
schema-version = 1
[project]
kind = \"app\"
entry = \"App.Main\"
[build]
environment = \"js/browser\"
";
    let db_mod = "\
module Store exposing (Db)
service Db =
  query : String ->{} String
";
    // 1. Helper called ONLY inside `provide Db` on `js/browser` -> no FLICK-E020.
    let handled_main = "\
module App.Main exposing (main)
import Store exposing (Db)
run_query : String ->{Db} String
let run_query sql = Db.query sql
main : Unit ->{} Unit
let main () =
  provide Db ({ query = fun _ -> \"ok\" }) {
    ignore (run_query \"select 1\")
  }
";
    let (diags, _) = check_files(
        &[
            ("/app/flick.toml", browser_toml),
            ("/app/src/Store.fl", db_mod),
            ("/app/src/App/Main.fl", handled_main),
        ],
        &["/app/src/App/Main.fl"],
    );
    assert!(
        !diags.iter().any(|d| d.severity == Severity::Error),
        "locally handled helper must not trigger FLICK-E020 or any error: {diags:?}"
    );

    // 2. Helper called BOTH inside `provide Db` and unhandled in an exported library function -> FLICK-E020 must fire.
    let browser_lib_toml = "\
schema-version = 1
[project]
kind = \"lib\"
[build]
environment = \"js/browser\"
";
    let mixed_lib = "\
module App.Main exposing (mixed)
import Store exposing (Db)
run_query : String ->{Db} String
let run_query sql = Db.query sql
mixed : Unit ->{Db} Unit
let mixed () =
  provide Db ({ query = fun _ -> \"ok\" }) {
    ignore (run_query \"handled\")
  }
  ignore (run_query \"unhandled\")
";
    let (diags, _) = check_files(
        &[
            ("/app/flick.toml", browser_lib_toml),
            ("/app/src/Store.fl", db_mod),
            ("/app/src/App/Main.fl", mixed_lib),
        ],
        &["/app/src/App/Main.fl"],
    );
    assert!(
        diags.iter().any(|d| d.code == codes::E020),
        "unhandled call site to helper must still report FLICK-E020: {diags:?}"
    );
}

#[test]
fn unreferenced_local_function_in_main_passes_preflight_and_is_pruned() {
    let go_toml = "\
schema-version = 1
[project]
kind = \"app\"
entry = \"App.Main\"
[build]
environment = \"go/native\"
";
    let js_only = "\
@when \"js\"
module JsOnly exposing (label)
label : Unit -> String
let label () = \"js\"
";
    // 1. Dead local function (and dead recursive local function) must not report P064.
    let dead_local_main = "\
module App.Main exposing (main)
import JsOnly
main : Unit -> Unit
let main () =
  let unused () = JsOnly.label ()
  dead_rec : Int -> String
  let rec dead_rec n =
    if n == 0 then JsOnly.label () else dead_rec (n - 1)
  ()
";
    let host = MemHost::new(
        &[
            ("/app/flick.toml", go_toml),
            ("/app/src/JsOnly.fl", js_only),
            ("/app/src/App/Main.fl", dead_local_main),
        ],
        vec![],
    );
    let mut sources = SourceMap::default();
    let project = discover(&host, &mut sources, Path::new("/app"), None)
        .expect("manifest")
        .expect("project");
    let (diags, compilation) = compile(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/app/src/App/Main.fl")],
    );
    assert!(
        !diags.iter().any(|d| d.severity == Severity::Error),
        "dead local functions must not trigger P064: {diags:?}"
    );

    // Pruning must strip dead tainted local functions so `main` keeps no references to `JsOnly`.
    let pruned = compilation.prune_for_environment(&sources, false);
    assert_eq!(pruned.modules.len(), 1);
    let mut uses = Vec::new();
    for item in &pruned.modules[0].ir.items {
        item.each_body(&mut |expr| {
            fn scan(expr: &Expr, out: &mut Vec<String>) {
                if let ExprKind::Global { module, .. } = &expr.kind {
                    out.push(module.clone());
                }
                expr.each_child(&mut |c| scan(c, out));
            }
            scan(expr, &mut uses);
        });
    }
    assert!(
        !uses.iter().any(|m| m == "JsOnly"),
        "pruned main still references JsOnly: {uses:?}"
    );

    // 2. A referenced local function MUST still report P064.
    let live_local_main = "\
module App.Main exposing (main)
import JsOnly
main : Unit -> Unit
let main () =
  let used () = JsOnly.label ()
  let _ = used ()
  ()
";
    let (diags, _) = check_files(
        &[
            ("/app/flick.toml", go_toml),
            ("/app/src/JsOnly.fl", js_only),
            ("/app/src/App/Main.fl", live_local_main),
        ],
        &["/app/src/App/Main.fl"],
    );
    assert!(
        diags.iter().any(|d| d.code == codes::P064),
        "referenced local function must still report P064: {diags:?}"
    );
}
