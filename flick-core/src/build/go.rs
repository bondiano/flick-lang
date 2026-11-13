//! A build for the go target: for each entry a Go module under
//! `go/<entry>/`, the program's code and the runtime, which `flick build`
//! then compiles with `go build` into the executable `flick-<entry>`.

use std::collections::BTreeMap;
use std::env;
use std::path::Path;
use std::sync::OnceLock;

use super::{Artifact, Settings, builtin_modules, optimized, specifier};
use crate::codegen::Known;
use crate::codegen::go::{self, Bindings, GO_MOD, GO_SUM, Input, PACKAGES, Package, RUNTIME};
use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::ir::{self, ItemKind};
use crate::project::{Compilation, normalize};
use crate::source::{SourceFile, SourceMap};
use crate::types;

/// The name of the Go module and executable that runs a project's tests.
pub const TEST: &str = "test";

/// The file of each Go module that lists what the build wrote in it, one path
/// of the output directory a line: `go build` compiles every file of a
/// package, so the next build into that directory removes the listed files it
/// no longer writes.
pub const FILES: &str = ".flick-files";

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
            if settings.release {
                optimized(modules, module, false)
            } else {
                module.ir.clone()
            }
        })
        .collect();
    let own: Vec<&ir::Module> = optimized.iter().collect();
    let builtins: Vec<&ir::Module> = builtin_modules().iter().collect();
    let foreign = foreign_files(compilation, settings, read);
    let lines: Vec<&[u32]> = sources.files().map(SourceFile::line_starts).collect();
    let lang = ir::lang_items();
    let known = Known::of(&lang);
    let mut out = Vec::new();
    for (name, entry) in programs {
        let dir = module_dir(name);
        let module_path = settings.out_dir.join(&dir);
        // a path relative to the Go module, as a source map's: no build path ships
        let paths: BTreeMap<String, String> = (modules.iter())
            .map(|c| (c.name.clone(), specifier(&module_path, &normalize(&c.path))))
            .collect();
        let output = go::go(&Input {
            own: &own,
            builtins: &builtins,
            entry,
            suites,
            paths: &paths,
            lines: &lines,
            foreign: &foreign,
            bindings: bindings(),
            lang: &known,
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
        out.push(listing(&dir, &out));
    }
    (out, diagnostics)
}

/// The Go file beside each foreign JS module a module of `compilation` names
/// relative to it: by the module and the specifier, its name in the Go module
/// and its text, which `read` gives.
fn foreign_files(
    compilation: &Compilation,
    settings: &Settings,
    read: &dyn Fn(&Path) -> Option<String>,
) -> BTreeMap<(String, String), (String, String)> {
    // `./x.mjs` of a module is `x.go` beside it, named in the Go module by its
    // path; a library's by its package and its path in it, as no store path ships
    let foreign_file = |module: &str, spec: &str| {
        let path = normalize(&(settings.source_dir)(module)?.join(spec)).with_extension("go");
        let text = read(&path)?;
        let named = (compilation.library_of(module)).and_then(|(name, dir)| {
            Some(Path::new("/").join(name).join(path.strip_prefix(dir).ok()?))
        });
        let name: String = (named.as_ref().unwrap_or(&path))
            .with_extension("")
            .to_string_lossy()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        Some((format!("ffi{name}.go"), text))
    };
    let mut foreign = BTreeMap::new();
    for c in &compilation.modules {
        for item in &c.ir.items {
            if let ItemKind::Foreign(f) = &item.kind
                && let Some(spec) = f.binding.from.as_deref().filter(|s| go::is_relative(s))
            {
                let key = (c.name.clone(), spec.to_owned());
                if !foreign.contains_key(&key)
                    && let Some(file) = foreign_file(&c.name, spec)
                {
                    foreign.insert(key, file);
                }
            }
        }
    }
    foreign
}

/// The Go functions of the runtime the tables of the implicit packages bind
/// JS exports to, read once.
pub(crate) fn bindings() -> &'static Bindings {
    static BINDINGS: OnceLock<Bindings> = OnceLock::new();
    BINDINGS.get_or_init(|| Bindings::parse(types::implicit_go_bindings().map(|(_, text)| *text)))
}

/// The [`FILES`] of the Go module `dir`: the paths of `out` in it.
fn listing(dir: &str, out: &[Artifact]) -> Artifact {
    let prefix = format!("{dir}/");
    let mut contents = String::new();
    for artifact in out.iter().filter(|a| a.path.starts_with(&prefix)) {
        contents.push_str(&artifact.path);
        contents.push('\n');
    }
    Artifact {
        path: format!("{prefix}{FILES}"),
        contents,
    }
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
