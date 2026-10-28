// The real compiler modules expose APIs this build-time lexer does not use.
#![allow(dead_code)]

use std::env;
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
    if cfg!(feature = "builtin-docs") {
        return Ok(());
    }
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo supplies OUT_DIR"));
    strip_tree(Path::new("src/types/builtins"), &output.join("builtins"))
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
