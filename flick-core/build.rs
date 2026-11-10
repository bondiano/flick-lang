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
#[path = "src/implicit_packages.rs"]
mod implicit_packages;

use implicit_packages::IMPLICIT_PACKAGES;

fn main() -> io::Result<()> {
    println!("cargo:rerun-if-changed=build.rs");
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo supplies OUT_DIR"));
    binding_policy(&output)?;
    go_runtime(&output)?;
    go_packages(&output)?;
    let docs = cfg!(feature = "native-project");
    if !docs {
        for package in ["core"].iter().chain(IMPLICIT_PACKAGES) {
            let sources = Path::new("../packages").join(package).join("src");
            strip_tree(&sources, &output.join(package))?;
        }
    }
    implicit_sources(&output, docs)
}

/// `implicit_sources.rs`: `IMPLICIT`, each implicit package with its modules
/// and the tables of its `src/go`.
fn implicit_sources(output: &Path, docs: bool) -> io::Result<()> {
    let mut out = String::from("pub(super) const IMPLICIT: &[Embedded] = &[\n");
    for package in IMPLICIT_PACKAGES {
        let modules = package_modules(package, output, docs)?;
        let go = package_go_tables(package)?;
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
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut out = String::new();
    if !dir.is_dir() {
        return Ok(out);
    }
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

/// Compile package-owned binding metadata; the compiler has no module tables.
fn binding_policy(output: &Path) -> io::Result<()> {
    let data = binding_data()?;
    let mut out = String::new();
    for (key, name) in [
        ("packages", "PACKAGES"),
        ("core_builtins", "CORE_BUILTINS"),
        ("families", "FAMILIES"),
        ("ui_renderer", "UI_RENDERER"),
        ("zone_exports", "ZONE_EXPORTS"),
    ] {
        let values: Vec<_> = data[key]
            .as_array()
            .expect("string array")
            .iter()
            .map(|v| v.as_str().expect("string"))
            .collect();
        if name == "ZONE_EXPORTS" {
            out.push_str("#[cfg(feature = \"native-project\")]\n");
        }
        writeln!(out, "pub const {name}: &[&str] = &{values:?};").expect("write string");
    }
    for (key, name) in [
        ("owned_prefix", "OWNED_PREFIX"),
        ("adapter_prefix", "ADAPTER_PREFIX"),
        ("native_entry", "NATIVE_ENTRY"),
        ("adapter_bridge", "ADAPTER_BRIDGE"),
        ("adapter_ui", "ADAPTER_UI"),
        ("zone_module", "ZONE_MODULE"),
    ] {
        if name == "ZONE_MODULE" {
            out.push_str("#[cfg(feature = \"native-project\")]\n");
        }
        if name == "ADAPTER_BRIDGE" {
            out.push_str("#[cfg(not(target_arch = \"wasm32\"))]\n");
        }
        writeln!(
            out,
            "pub const {name}: &str = {:?};",
            data[key].as_str().expect("string")
        )
        .expect("write string");
    }
    let aliases: Vec<_> = data["aliases"]
        .as_array()
        .expect("aliases")
        .iter()
        .map(|r| {
            (
                r[0].as_str().expect("from"),
                r[1].as_str().expect("to"),
                r[2].as_bool().expect("prefix"),
            )
        })
        .collect();
    writeln!(
        out,
        "pub const ALIASES: &[(&str, &str, bool)] = &{aliases:?};"
    )
    .expect("write string");
    let routes: Vec<_> = data["routes"]
        .as_array()
        .expect("routes")
        .iter()
        .map(|r| (r[0].as_str().expect("from"), r[1].as_str(), r[2].as_str()))
        .collect();
    writeln!(
        out,
        "pub const ROUTES: &[(&str, Option<&str>, Option<&str>)] = &{routes:?};"
    )
    .expect("write string");
    for (path, name) in [
        ("../runtime/rt_exports.txt", "EXPORTS"),
        ("../runtime/rt_stubs.txt", "STUBS"),
        ("../packages/effect-ts/rt_bridge_stubs.txt", "BRIDGE_STUBS"),
    ] {
        println!("cargo:rerun-if-changed={path}");
        writeln!(
            out,
            "pub const {name}: &str = {:?};",
            fs::read_to_string(path)?
        )
        .expect("write string");
    }
    binding_go(&data, &mut out);
    fs::write(output.join("bindings.rs"), out)
}

fn binding_data() -> io::Result<serde_json::Value> {
    let mut data = serde_json::Map::new();
    for path in [
        "../packages/effect-ts/compiler-bindings.json",
        "../packages/effect-ts/compiler-families.json",
    ] {
        println!("cargo:rerun-if-changed={path}");
        let part: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&fs::read_to_string(path)?)?;
        for (key, value) in part {
            if data.insert(key.clone(), value).is_some() {
                return Err(io::Error::other(format!("duplicate binding key: {key}")));
            }
        }
    }
    Ok(serde_json::Value::Object(data))
}

fn binding_go(data: &serde_json::Value, out: &mut String) {
    out.push_str("#[cfg(feature = \"native-project\")]\npub const GO_OPTIONAL: &[(&str, &str, &[&str])] = &[\n");
    for row in data["go_optional"].as_array().expect("Go packages") {
        let exports: Vec<_> = row[2]
            .as_array()
            .expect("exports")
            .iter()
            .map(|v| v.as_str().expect("export"))
            .collect();
        writeln!(
            out,
            "({:?}, {:?}, &{exports:?}),",
            row[0].as_str().expect("package"),
            row[1].as_str().expect("module")
        )
        .expect("write string");
    }
    out.push_str("];\n");
}
