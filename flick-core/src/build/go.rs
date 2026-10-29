//! A build for the go target: for each entry a Go module under
//! `go/<entry>/`, the program's code and the runtime, which `flick build`
//! then compiles with `go build` into the executable `flick-<entry>`.

use std::path::Path;

use super::{Artifact, Settings, builtin_modules, generic_of, specifier};
use crate::codegen::go::{self, Program, RUNTIME};
use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::ir;
use crate::opt;
use crate::project::{Compilation, normalize};
use crate::source::{SourceMap, Span};

/// The name of the Go module and executable that runs a project's tests.
pub const TEST: &str = "test";

/// The directory of the Go module of the entry `name`.
pub fn module_dir(name: &str) -> String {
    format!("go/{name}")
}

/// The executable of the entry `name`, beside the runners of the JS target.
pub fn executable(name: &str) -> String {
    let file = format!("flick-{name}");
    if cfg!(windows) {
        format!("{file}.exe")
    } else {
        file
    }
}

/// The Go modules of the app's entries, or with `suites` the one Go module
/// `test` that runs their tests, and what could not be compiled; `read`
/// gives the text of a Go file beside a foreign JS module, if any.
pub fn emit(
    compilation: &Compilation,
    sources: &SourceMap,
    settings: &Settings,
    suites: &[String],
    read: &dyn Fn(&Path) -> Option<String>,
) -> (Vec<Artifact>, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let tests = [(TEST.to_owned(), String::new())];
    let programs = if suites.is_empty() {
        settings.entries
    } else {
        &tests[..]
    };
    if programs.is_empty() {
        diagnostics.push(Diagnostic::new(
            codes::P012,
            Severity::Error,
            "the go target builds an app: there is no entry whose `main` it runs",
        ));
        return (Vec::new(), diagnostics);
    }
    let modules = &compilation.modules;
    // the same optimization as the JS target's, so constant folding agrees
    let optimized: Vec<ir::Module> = modules
        .iter()
        .map(|module| {
            let mut ir = module.ir.clone();
            if settings.release {
                let exported = |name: &str| module.interface.exports_value(name);
                let generics = |m: &str, name: &str| generic_of(modules, m, name);
                opt::optimize(&mut ir, &exported, &generics);
            }
            ir
        })
        .collect();
    let all: Vec<&ir::Module> = optimized.iter().chain(builtin_modules()).collect();
    let own = |m: &str| modules.iter().any(|c| c.name == m);
    let package = |m: &str| {
        own(m)
            .then(|| settings.package.map(str::to_owned))
            .flatten()
    };
    // `./x.mjs` of a module is `x.go` beside it, named in the Go module by its path
    let foreign_file = |module: &str, spec: &str| {
        let path = normalize(&(settings.source_dir)(module)?.join(spec)).with_extension("go");
        let text = read(&path)?;
        let name: String = path
            .with_extension("")
            .to_string_lossy()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        Some((format!("ffi{name}.go"), text))
    };
    let mut out = Vec::new();
    for (name, entry) in programs {
        let dir = module_dir(name);
        let module_path = settings.out_dir.join(&dir);
        // a path relative to the Go module, as a source map's: no build path ships
        let location = |module: &str, span: Span| {
            let compiled = modules.iter().find(|m| m.name == module)?;
            let file = sources.get(span.file);
            let path = specifier(&module_path, &normalize(&compiled.path));
            Some((path, file.line_col(span.start).0 + 1))
        };
        let output = go::go(&Program {
            modules: &all,
            entry,
            suites,
            package: &package,
            own: &own,
            location: &location,
            foreign_file: &foreign_file,
        });
        diagnostics.extend(output.diagnostics);
        out.push(Artifact {
            path: format!("{dir}/go.mod"),
            contents: "module flick.app\n\ngo 1.23\n".to_owned(),
        });
        out.extend(output.files.into_iter().map(|(path, contents)| Artifact {
            path: format!("{dir}/{path}"),
            contents,
        }));
        out.extend(RUNTIME.iter().map(|(path, contents)| Artifact {
            path: format!("{dir}/flickrt/{path}"),
            contents: (*contents).to_owned(),
        }));
    }
    (out, diagnostics)
}
