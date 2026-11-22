// The real compiler modules expose APIs this build-time lexer does not use.
#![allow(dead_code)]

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[path = "../flick-diagnostics/src/diagnostics/codes.rs"]
mod codes;
use codes::Code;
#[path = "../flick-diagnostics/src/diagnostics/model.rs"]
mod model;
mod diagnostics {
    pub(crate) use crate::codes::{self, Code};
    pub(crate) use crate::model::*;
}
#[path = "../flick-syntax/src/syntax/kind.rs"]
mod kind;
#[path = "../flick-syntax/src/syntax/lexer/mod.rs"]
mod lexer;
#[path = "../flick-diagnostics/src/source.rs"]
mod source;
mod syntax {
    pub(crate) use crate::{kind, lexer};
}
#[path = "src/builtin_source.rs"]
mod builtin_source;
#[path = "src/implicit_packages.rs"]
mod implicit_packages;

use implicit_packages::IMPLICIT_PACKAGES;

fn main() -> io::Result<()> {
    println!("cargo:rerun-if-changed=build.rs");
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo supplies OUT_DIR"));
    let native = cfg!(feature = "native-project");
    if !native {
        for package in ["core"].iter().chain(IMPLICIT_PACKAGES) {
            let sources = Path::new("../packages").join(package).join("src");
            strip_tree(&sources, &output.join(package))?;
        }
    }
    implicit_sources(&output, native)
}

/// `implicit_sources.rs`: `IMPLICIT`, each implicit package with its modules
/// and, with `native-project`, the tables of its `src/go`.
fn implicit_sources(output: &Path, native: bool) -> io::Result<()> {
    let mut out = String::from("pub(super) const IMPLICIT: &[Embedded] = &[\n");
    for package in IMPLICIT_PACKAGES {
        let modules = package_modules(package, output, native)?;
        let go = if native {
            package_go_tables(package)?
        } else {
            String::new()
        };
        let _ = writeln!(out, "    ({package:?}, &[\n{modules}    ], &[\n{go}    ]),");
    }
    out.push_str("];\n");
    fs::write(output.join("implicit_sources.rs"), out)
}

/// The tables of `packages/<package>/src/go`, by file name, as array items; a
/// package without them has none.
fn package_go_tables(package: &str) -> io::Result<String> {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo supplies it"));
    let dir = manifest.join("../packages").join(package).join("src/go");
    let mut out = String::new();
    if !dir.is_dir() {
        // Watching a missing path reruns this script on every Cargo command.
        // Its existing parent also detects the first Go table being added.
        let sources = dir.parent().expect("go is under src");
        println!("cargo:rerun-if-changed={}", sources.display());
        return Ok(out);
    }
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)?
        .map(|e| e.map(|e| e.path()))
        .collect::<io::Result<_>>()?;
    files.retain(|p| p.extension().is_some_and(|e| e == "txt"));
    files.sort();
    for path in files {
        println!("cargo:rerun-if-changed={}", path.display());
        let name = path
            .file_name()
            .expect("a file")
            .to_string_lossy()
            .into_owned();
        let _ = writeln!(
            out,
            "        ({name:?}, include_str!({:?})),",
            path.display().to_string()
        );
    }
    Ok(out)
}

/// The modules of `packages/<package>` in the order of its `modules.txt`, each
/// its file as written, or without its docs, as array items.
fn package_modules(package: &str, output: &Path, docs: bool) -> io::Result<String> {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo supplies it"));
    let sources = manifest.join("../packages").join(package).join("src");
    let order = sources.join("modules.txt");
    println!("cargo:rerun-if-changed={}", order.display());
    let mut out = String::new();
    for module in fs::read_to_string(&order)?
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let file = format!("{}.fl", module.replace('.', "/"));
        let path = if docs {
            sources.join(&file)
        } else {
            output.join(package).join(&file)
        };
        let _ = writeln!(
            out,
            "        ({module:?}, include_str!({:?})),",
            path.display().to_string()
        );
    }
    Ok(out)
}

fn strip_tree(input: &Path, output: &Path) -> io::Result<()> {
    println!("cargo:rerun-if-changed={}", input.display());
    fs::create_dir_all(output)?;
    let mut paths = fs::read_dir(input)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<Vec<_>>>()?;
    paths.sort_unstable();
    for path in paths {
        if path.is_dir() {
            strip_tree(
                &path,
                &output.join(path.file_name().expect("directory entry has a name")),
            )?;
        } else if path.extension().is_some_and(|extension| extension == "fl") {
            println!("cargo:rerun-if-changed={}", path.display());
            let text = fs::read_to_string(&path)?;
            let stripped = builtin_source::without_docs(&text)
                .map_err(|problem| io::Error::other(format!("{}: {problem}", path.display())))?;
            fs::write(
                output.join(path.file_name().expect("directory entry has a name")),
                stripped,
            )?;
        }
    }
    Ok(())
}
