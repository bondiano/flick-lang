//! A build for the go target: for each entry a Go module under
//! `go/<entry>/`, the program's code and the runtime, which `flick build`
//! then compiles with `go build` into the executable `flick-<entry>`.

use std::env;
use std::path::Path;

use super::{Artifact, Settings, builtin_modules, generic_of, specifier};
use crate::codegen::go::{self, GO_MOD, GO_SUM, PACKAGES, Package, Program, RUNTIME};
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

/// The executable of the entry `name`, beside the runners of the JS target;
/// `.exe` when Go builds for windows, by `GOOS` or else the host.
pub fn executable(name: &str) -> String {
    let file = format!("flick-{name}");
    let windows = env::var("GOOS").map_or(cfg!(windows), |os| os == "windows");
    if windows { format!("{file}.exe") } else { file }
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
    let package = |m: &str| super::own_package(modules, settings, m);
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
        let packages: Vec<&Package> = PACKAGES
            .iter()
            .filter(|p| output.packages.contains(&p.name))
            .collect();
        out.push(Artifact {
            path: format!("{dir}/go.mod"),
            contents: go_mod(&packages),
        });
        // the modules of a package go build downloads once; go.sum pins them
        if !packages.is_empty() {
            out.push(Artifact {
                path: format!("{dir}/go.sum"),
                contents: GO_SUM.to_owned(),
            });
        }
        out.extend(
            packages
                .iter()
                .flat_map(|p| p.files)
                .map(|(path, contents)| Artifact {
                    path: format!("{dir}/{path}"),
                    contents: (*contents).to_owned(),
                }),
        );
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

/// The `go.mod` of a program with `packages`: the runtime's, requiring only
/// the modules of those packages, so a program without one builds offline
/// and a package's version skew never makes Go load the others.
pub fn go_mod(packages: &[&Package]) -> String {
    if packages.is_empty() {
        return "module flick.app\n\ngo 1.23\n".to_owned();
    }
    let needed: Vec<&str> = packages.iter().flat_map(|p| p.modules.lines()).collect();
    let mut header = String::new();
    let mut requires = String::new();
    let mut block = false;
    for line in GO_MOD.lines() {
        let line = line.trim();
        let require = if block {
            block = line != ")";
            line
        } else if line == "require (" {
            block = true;
            continue;
        } else if let Some(rest) = line.strip_prefix("require ") {
            rest
        } else {
            if line.starts_with("module ") || line.starts_with("go ") {
                header.push_str(line);
                header.push_str("\n\n");
            }
            continue;
        };
        if require
            .split_whitespace()
            .next()
            .is_some_and(|m| needed.contains(&m))
        {
            requires.push('\t');
            requires.push_str(require);
            requires.push('\n');
        }
    }
    format!("{header}require (\n{requires})\n")
}
