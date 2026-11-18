//! Embeds the go target's runtime: the package `flickrt` and the optional
//! packages of `runtime-go`, which a program's Go module gets beside it.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn main() -> io::Result<()> {
    println!("cargo:rerun-if-changed=build.rs");
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo supplies OUT_DIR"));
    go_runtime(&output)?;
    go_packages(&output)
}

/// `go_runtime.rs`: the files of the go target's runtime, the package
/// `flickrt`, by their name: every `.go` file of `runtime-go/flickrt` but
/// its tests.
fn go_runtime(output: &Path) -> io::Result<()> {
    let dir = Path::new("../../runtime-go/flickrt");
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
    let root = Path::new("../../runtime-go");
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
