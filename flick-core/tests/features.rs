use std::io;
use std::path::{Path, PathBuf};

use flick_core::build::{self, Artifact, Extras, Settings};
use flick_core::codegen::{self, EffectBridge};
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::iface;
use flick_core::ir::ItemKind;
use flick_core::project::config::Backend;
use flick_core::project::{self, Compilation, Host};
use flick_core::source::{FileId, SourceMap};
use flick_core::types::{Kind, Type};

struct Module<'a>(&'a str);

impl Host for Module<'_> {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        if path == Path::new("/Main.fl") {
            Ok(self.0.as_bytes().to_vec())
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    }

    fn display(&self, path: &Path) -> String {
        path.display().to_string()
    }
}

fn compile(source: &str) -> (Vec<Diagnostic>, SourceMap, Compilation) {
    let mut sources = SourceMap::default();
    let (diagnostics, compilation) = project::compile(
        &Module(source),
        &mut sources,
        None,
        &[PathBuf::from("/Main.fl")],
    );
    (diagnostics, sources, compilation)
}

fn source_path(_: FileId) -> String {
    "file:///Main.fl".to_owned()
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "the build settings require this shape"
)]
fn source_dir(_: &str) -> Option<PathBuf> {
    Some(PathBuf::from("/"))
}

fn settings(release: bool) -> Settings<'static> {
    Settings {
        package: None,
        entries: &[],
        typescript: false,
        release,
        extras: Extras::default(),
        source_path: &source_path,
        source_dir: &source_dir,
        out_dir: Path::new("/dist"),
        root: None,
        backend: Backend::Effect,
        effect_bridge: EffectBridge::Absent,
        ui: None,
        runners: &[],
    }
}

fn emit(source: &str, settings: &Settings) -> (Vec<Artifact>, Vec<Diagnostic>) {
    let (diagnostics, sources, compilation) = compile(source);
    assert!(
        diagnostics.iter().all(|d| d.severity != Severity::Error),
        "{diagnostics:?}"
    );
    build::emit(&compilation, &sources, settings)
}

const MAIN: &str = "module Main exposing (answer)\nanswer : Int\nlet answer = 42\n";

#[test]
fn optional_outputs_have_real_artifacts_only_when_enabled() {
    for release in [false, true] {
        let (artifacts, diagnostics) = emit(MAIN, &settings(release));
        assert_eq!(diagnostics, []);
        let file = |path: &str| artifacts.iter().find(|artifact| artifact.path == path);
        assert!(file("Main.js").is_some());
        assert_eq!(
            file("package.json").is_some(),
            cfg!(feature = "native-project")
        );
        assert_eq!(
            file("Main.iface").is_some(),
            cfg!(feature = "native-project")
        );
        assert_eq!(
            file("Main.js.map").is_some(),
            cfg!(feature = "native-project")
        );
        if let Some(artifact) = file("Main.iface") {
            let interface =
                iface::read(artifact.contents.as_bytes(), "Main").expect("published interface");
            assert!(interface.exports_value("answer"));
            let answer = interface
                .abi
                .expect("published ABI")
                .values
                .into_iter()
                .find(|value| value.name == "answer")
                .expect("answer ABI");
            assert!(!answer.partial);
        }
        if let Some(artifact) = file("Main.js.map") {
            let map: serde_json::Value =
                serde_json::from_str(&artifact.contents).expect("source map");
            assert_eq!(map["version"], 3);
            assert_eq!(map["sources"], serde_json::json!(["file:///Main.fl"]));
        }
    }
}

#[test]
fn shared_browser_and_sql_programs_compile_without_server_modules() {
    let browser = "module Main exposing (view, fetch, main)\n\
        view : Ui\nlet view = Ui.text \"browser\"\n\
        fetch : String ->{Async, Http.Client.HttpClient; Http.ClientError.HttpClientError} Http.ClientResponse.ClientResponse\n\
        let fetch url = Http.Client.get url\n\
        main : Unit ->{} Unit\nlet main () = Platform.stdout_line \"ok\"\n";
    let sql = "module Main exposing (query)\n\
        import Sql\n\
        sql table Users \"users\" { id : bigint primary key as BigInt }\n\
        query : Sql.Template Sql.Pg {} { id : BigInt }\n\
        let query = sql {\n  from user in Users\n  select { id = user.id }\n}\n";
    for source in [browser, sql] {
        for release in [false, true] {
            let (artifacts, diagnostics) = emit(source, &settings(release));
            assert_eq!(diagnostics, []);
            assert!(artifacts.iter().any(|artifact| artifact.path == "Main.js"));
        }
    }
}

#[test]
fn server_fns_and_cli_blocks_are_plugins_of_std_in_every_build() {
    let server = "module Main exposing (ping)\nimport Server\n\
        ping : Unit ->{Server.Server;} Unit\nserver fn ping () = ()\n";
    let (diagnostics, _, compilation) = compile(server);
    assert!(
        diagnostics.iter().all(|d| d.severity != Severity::Error),
        "{diagnostics:?}"
    );
    assert!(compilation.modules[0].interface.exports_value("ping"));
    // `cli` is a plugin of std, which every build has
    let cli = "module Main exposing (command)\nimport Cli\n\
        let command = cli \"hello\" { handle fun _ -> Platform.stdout_line \"hello\" }\n";
    let (diagnostics, _, compilation) = compile(cli);
    assert!(
        diagnostics.iter().all(|d| d.severity != Severity::Error),
        "{diagnostics:?}"
    );
    assert!(compilation.modules[0].interface.exports_value("command"));
    // the server modules are part of the std package
    let (diagnostics, _, _) =
        compile("module Main exposing (answer)\nimport File\nanswer : Int\nlet answer = 42\n");
    let missing = diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error && d.code == codes::R003);
    assert!(!missing, "{diagnostics:?}");
}

#[test]
fn typed_declarations_and_facades_do_not_require_native_artifact_publication() {
    let source = "module Main exposing (answer)\nanswer : Unit ->{} Int\nlet answer () = 42\n";
    for typescript in [false, true] {
        let mut settings = settings(false);
        settings.typescript = typescript;
        settings.extras.dts = true;
        settings.extras.promise_facade = true;
        let (artifacts, diagnostics) = emit(source, &settings);
        assert_eq!(diagnostics, []);
        let contains = |path: &str| artifacts.iter().any(|artifact| artifact.path == path);
        assert!(contains(if typescript { "Main.ts" } else { "Main.js" }));
        assert!(contains("Main.d.ts"));
        assert!(contains("Main.promise.js"));
        assert!(contains("Main.promise.d.ts"));
        assert_eq!(contains("Main.iface"), cfg!(feature = "native-project"));
    }
}

#[test]
fn an_inferred_row_argument_takes_the_kind_of_its_parameter() {
    let source = "module Main exposing (functions)\n\nimport Server\n\n\
                  greet : String ->{Server.Server} String\n\
                  server fn greet name = \"Hello, \" ++ name\n\n\
                  let functions = [server greet]\n";
    let (diagnostics, _, compilation) = compile(source);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
    let main = compilation
        .modules
        .iter()
        .find(|m| m.name == "Main")
        .expect("Main compiles");
    let functions = main
        .interface
        .exports
        .iter()
        .find(|e| e.entity.name == "functions")
        .and_then(|e| e.scheme.clone());
    // `Server.Function (r : ServiceRow)`: the open tail of `{Server.Server | a}`
    let Some(Type::Forall { vars, .. }) = functions else {
        panic!("`functions` is polymorphic: {functions:?}");
    };
    let kinds: Vec<Kind> = vars.into_iter().map(|b| b.kind).collect();
    assert_eq!(kinds, [Kind::ServiceRow]);
}

#[test]
fn an_explicit_build_package_qualifies_tags_without_project_discovery() {
    let source = "module Main exposing (Token(..), token)\ntype Token = Token String\ntoken : Token\nlet token = Token \"x\"\n";
    let (diagnostics, sources, compilation) = compile(source);
    assert!(
        diagnostics.iter().all(|d| d.severity != Severity::Error),
        "{diagnostics:?}"
    );
    let constructor = compilation
        .modules
        .iter()
        .flat_map(|m| &m.ir.items)
        .find_map(|item| {
            if let ItemKind::Constructor(c) = &item.kind {
                Some(c)
            } else {
                None
            }
        })
        .expect("the Token constructor");
    let expected = codegen::tag(constructor, Some("@acme/fallback"));
    for typescript in [false, true] {
        let mut options = settings(false);
        options.package = Some("@acme/fallback");
        options.typescript = typescript;
        let (artifacts, diagnostics) = build::emit(&compilation, &sources, &options);
        assert_eq!(diagnostics, []);
        let path = if typescript { "Main.ts" } else { "Main.js" };
        let code = &artifacts
            .iter()
            .find(|a| a.path == path)
            .expect("emitted module")
            .contents;
        assert!(
            code.contains(&expected),
            "{path}: expected package-qualified tag {expected}: {code}"
        );
    }
}
