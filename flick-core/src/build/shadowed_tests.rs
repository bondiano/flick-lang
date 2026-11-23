//! Regression tests ensuring user modules that shadow built-in module names
//! (such as `Int` or `String`) are never treated as built-in native primitives
//! in `build::optimized`, `build::emit` (JS Effect/RT), or `build::go::emit`.

use std::io;
use std::path::{Path, PathBuf};

use super::{Backend, Extras, Settings};
use crate::codegen::EffectBridge;
use crate::diagnostics::Severity;
use crate::ir::{Expr, ExprKind, ItemKind, Lit, Module};
use crate::project::{self, Compilation, Host};
use crate::source::{FileId, SourceMap};

struct MultiMem<'a>(&'a [(&'a str, &'a str)]);
impl Host for MultiMem<'_> {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        for &(p, text) in self.0 {
            if path == Path::new(p) {
                return Ok(text.as_bytes().to_vec());
            }
        }
        Err(io::ErrorKind::NotFound.into())
    }
    fn display(&self, path: &Path) -> String {
        path.display().to_string()
    }
}

fn int_source_path(_: FileId) -> String {
    "file:///Int.fl".to_owned()
}

#[expect(clippy::unnecessary_wraps, reason = "build Settings callback shape")]
fn int_source_dir(_: &str) -> Option<PathBuf> {
    Some(PathBuf::from("/"))
}

fn release_settings(entries: &[(String, String)], backend: Backend) -> Settings<'_> {
    Settings {
        package: None,
        entries,
        typescript: false,
        release: true,
        extras: Extras::default(),
        source_path: &int_source_path,
        source_dir: &int_source_dir,
        out_dir: Path::new("/dist"),
        root: None,
        backend,
        effect_bridge: EffectBridge::Absent,
        ui: None,
        runners: &[],
        environment: None,
    }
}

const SHADOWED_INT_SRC: &str = "\
module Int exposing (parse, result, earth_len)

parse : String -> String
foreign pure parse from \"./custom.mjs\" \"myParse\"

result : String
let result = parse \"36\"

earth_len : Int
let earth_len = String.length \"a\\u{1F30D}b\"
";

const CALLER_MAIN_SRC: &str = "\
module Main exposing (from_shadowed)
import Int

from_shadowed : String
let from_shadowed = Int.parse \"36\"
";

fn compile_shadowed_pair() -> (SourceMap, Compilation) {
    let files = [("/Int.fl", SHADOWED_INT_SRC), ("/Main.fl", CALLER_MAIN_SRC)];
    let mut sources = SourceMap::default();
    let (diags, compilation) = project::compile(
        &MultiMem(&files),
        &mut sources,
        None,
        &[PathBuf::from("/Int.fl"), PathBuf::from("/Main.fl")],
    );
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "{diags:?}"
    );
    (sources, compilation)
}

fn value_body<'a>(module: &'a Module, target: &str) -> &'a Expr {
    module
        .items
        .iter()
        .find_map(|i| match &i.kind {
            ItemKind::Value { name, body, .. } if name == target => Some(body),
            _ => None,
        })
        .expect("value item")
}

#[test]
fn shadowed_builtin_module_foreign_homonym_is_never_folded_in_optimized_ir() {
    let (_sources, compilation) = compile_shadowed_pair();
    let int_mod = compilation
        .modules
        .iter()
        .find(|m| m.name == "Int")
        .expect("Int module");
    let main_mod = compilation
        .modules
        .iter()
        .find(|m| m.name == "Main")
        .expect("Main module");

    for by_class in [false, true] {
        let opt_int = super::optimized(&compilation.modules, int_mod, by_class);
        let result_body = value_body(&opt_int, "result");
        assert!(
            matches!(
                &result_body.kind,
                ExprKind::App { func, .. }
                    if matches!(&func.kind, ExprKind::Local(n) if n == "parse")
                        || matches!(&func.kind, ExprKind::Global { module, name, .. } if module == "Int" && name == "parse")
            ),
            "shadowed Int.parse must stay a call (by_class={by_class}): {result_body:?}"
        );
        let opt_main = super::optimized(&compilation.modules, main_mod, by_class);
        let caller_body = value_body(&opt_main, "from_shadowed");
        assert!(
            matches!(
                &caller_body.kind,
                ExprKind::App { func, .. }
                    if matches!(&func.kind, ExprKind::Global { module, name, .. } if module == "Int" && name == "parse")
            ),
            "caller Int.parse must stay a call (by_class={by_class}): {caller_body:?}"
        );
        assert_eq!(
            value_body(&opt_int, "earth_len").kind,
            ExprKind::Lit(Lit::Int(4))
        );
    }
}

#[test]
fn shadowed_builtin_module_foreign_homonym_is_never_folded_in_js_release() {
    let (sources, compilation) = compile_shadowed_pair();
    for backend in [Backend::Effect, Backend::Rt] {
        let (artifacts, emit_diags) =
            super::emit(&compilation, &sources, &release_settings(&[], backend));
        assert!(emit_diags.is_empty(), "{emit_diags:?}");
        let js = artifacts
            .iter()
            .find(|a| a.path.ends_with("Int.js"))
            .expect("Int.js")
            .contents
            .as_str();
        let result_line = js
            .lines()
            .find(|l| l.starts_with("export const result ="))
            .expect("result export");
        assert!(
            result_line.contains("$f_parse(\"36\")") || result_line.contains("myParse(\"36\")"),
            "shadowed Int.parse miscompiled on {backend:?}: {result_line}\n{js}"
        );
        assert!(js.contains("export const earth_len = 4;"), "{js}");

        let main_js = artifacts
            .iter()
            .find(|a| a.path == "Main.js")
            .expect("Main.js")
            .contents
            .as_str();
        assert!(
            main_js.contains("parse(\"36\")") && !main_js.contains("some(36)"),
            "Main.js miscompiled shadowed Int.parse on {backend:?}:\n{main_js}"
        );
    }
}

#[cfg(feature = "native-project")]
#[test]
fn shadowed_builtin_module_foreign_homonym_is_never_folded_in_go_release() {
    let go_source = "\
module Int exposing (main, parse, earth_len)

parse : String -> String
foreign pure parse from \"./custom.mjs\" \"myParse\"

earth_len : Int
let earth_len = String.length \"a\\u{1F30D}b\"

main : Unit ->{Host.Console; AppError} Unit
let main () =
  Platform.stdout_line (parse \"36\")
  Platform.stdout_line (show earth_len)
";
    let go_files = [("/Int.fl", go_source)];
    let mut go_sources = SourceMap::default();
    let (go_diags, go_comp) = project::compile(
        &MultiMem(&go_files),
        &mut go_sources,
        None,
        &[PathBuf::from("/Int.fl")],
    );
    assert!(
        go_diags.iter().all(|d| d.severity != Severity::Error),
        "{go_diags:?}"
    );
    let entries = vec![("main".to_owned(), "Int".to_owned())];
    let (go_artifacts, _) = super::go::emit(
        &go_comp,
        &go_sources,
        &release_settings(&entries, Backend::Effect),
        &[],
        false,
        &|_| Some("package main\nfunc myParse(s string) string { return s }\n".into()),
    );
    let go_code = go_artifacts
        .iter()
        .find(|a| a.path.ends_with("m_Int.go"))
        .expect("m_Int.go")
        .contents
        .as_str();
    assert!(
        !go_code.contains("flickrt.NewData(\"Some\", int32(36))"),
        "Go release miscompiled shadowed Int.parse to Some(36):\n{go_code}"
    );
    assert!(
        go_code.contains("F_Int__parse(fb, \"36\")"),
        "Go release must keep shadowed Int.parse call:\n{go_code}"
    );
    assert!(
        go_code.contains("F_Platform__stdout_line(fb, \"4\")"),
        "Unshadowed String.length should still fold in Go:\n{go_code}"
    );
}
