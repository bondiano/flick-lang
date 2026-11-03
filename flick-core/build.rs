// The real compiler modules expose APIs this build-time lexer does not use.
#![allow(dead_code)]

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[path = "src/diagnostics/codes.rs"]
mod codes;
use codes::Code;
#[path = "src/diagnostics/model.rs"]
mod model;
mod diagnostics {
    pub(crate) use crate::codes::{self, Code};
    pub(crate) use crate::model::*;
}
#[path = "src/syntax/kind.rs"]
mod kind;
#[path = "src/syntax/lexer/mod.rs"]
mod lexer;
#[path = "src/source.rs"]
mod source;
mod syntax {
    pub(crate) use crate::{kind, lexer};
}
#[path = "src/builtin_source.rs"]
mod builtin_source;

fn main() -> io::Result<()> {
    println!("cargo:rerun-if-changed=build.rs");
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo supplies OUT_DIR"));
    go_runtime(&output)?;
    go_packages(&output)?;
    if cfg!(feature = "builtin-docs") {
        return Ok(());
    }
    for package in ["core", "std"] {
        let sources = Path::new("../packages").join(package).join("src");
        strip_tree(&sources, &output.join(package))?;
    }
    Ok(())
}

/// `go_runtime.rs`: the files of the go target's runtime, the package
/// `flickrt`, by their name: every `.go` file of `runtime-go/flickrt` but
/// its tests.
fn go_runtime(output: &Path) -> io::Result<()> {
    let dir = Path::new("../runtime-go/flickrt");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut files = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        let name = path
            .file_name()
            .expect("a file name")
            .to_string_lossy()
            .into_owned();
        let go = path.extension().is_some_and(|e| e == "go");
        if path.is_file() && go && !name.ends_with("_test.go") {
            files.push(name);
        }
    }
    files.sort();
    let mut out = String::from("&[\n");
    for name in files {
        let path = fs::canonicalize(dir.join(&name))?;
        writeln!(
            out,
            "    ({name:?}, include_str!({:?})),",
            path.display().to_string()
        )
        .expect("writing to a String");
    }
    out.push_str("]\n");
    fs::write(output.join("go_runtime.rs"), out)
}

/// `go_packages.rs`: the go target's optional packages, every directory
/// of `runtime-go` but `flickrt` (the `PostgreSQL` driver `flickpg`, …):
/// their `.go` files by path in a program's Go module, and the modules of
/// their `modules.txt`, which `go build` downloads as `go.sum` pins them.
fn go_packages(output: &Path) -> io::Result<()> {
    let root = Path::new("../runtime-go");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut names = Vec::new();
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() && path.file_name().is_some_and(|n| n != "flickrt") {
            names.push(
                path.file_name()
                    .expect("a name")
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    names.sort();
    let mut out = String::from("&[\n");
    for name in names {
        let dir = root.join(&name);
        println!("cargo:rerun-if-changed={}", dir.display());
        let mut files = Vec::new();
        list(root, &dir, &mut files)?;
        files.sort();
        writeln!(
            out,
            "    Package {{\n        name: {name:?},\n        files: &["
        )
        .expect("writing to a String");
        for rel in files {
            let path = fs::canonicalize(root.join(&rel))?;
            writeln!(
                out,
                "            ({rel:?}, include_str!({:?})),",
                path.display().to_string()
            )
            .expect("writing to a String");
        }
        let modules = fs::canonicalize(dir.join("modules.txt"))?;
        writeln!(
            out,
            "        ],\n        modules: include_str!({:?}),\n    }},",
            modules.display().to_string()
        )
        .expect("writing to a String");
    }
    out.push_str("]\n");
    fs::write(output.join("go_packages.rs"), out)
}

fn list(root: &Path, dir: &Path, out: &mut Vec<String>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            list(root, &path, out)?;
        } else if path.extension().is_some_and(|e| e == "go")
            && !path.to_string_lossy().ends_with("_test.go")
        {
            let rel = path.strip_prefix(root).expect("under the root");
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
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
