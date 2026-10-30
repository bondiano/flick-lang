use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::source::SourceMap;

mod decl;
mod layout;

/// The formatted text, checked to be a fixed point of the formatter, to keep
/// every declaration formatted and to end no line with whitespace.
fn fmt(text: &str) -> String {
    let mut map = SourceMap::default();
    let file = map.add("t.fl", text);
    let (once, kept) = format_reporting(text, file).unwrap_or_else(|d| panic!("{d:?}"));
    assert!(kept.is_empty(), "left as written: {kept:?}\n{once}");
    assert_eq!(
        format(&once, file).as_deref(),
        Ok(once.as_str()),
        "idempotent"
    );
    assert!(
        once.lines().all(|l| l.trim_end() == l),
        "no trailing whitespace:\n{once}"
    );
    once
}

#[test]
fn refuses_files_with_syntax_errors() {
    let mut map = SourceMap::default();
    let file = map.add("t.fl", "let = 1\n");
    let errors = format("let = 1\n", file).expect_err("syntax error");
    assert_eq!(errors[0].code, codes::S007);
}

#[test]
fn leaves_a_declaration_with_a_comment_it_cannot_place_as_written() {
    let src = "f : A -> // why\n    B\nlet  x  =  1\n";
    let mut map = SourceMap::default();
    let file = map.add("t.fl", src);
    let (out, kept) = format_reporting(src, file).expect("formats");
    assert_eq!(out, "f : A -> // why\n    B\nlet x = 1\n");
    assert_eq!(kept, [(0, 21)]);
    assert_eq!(format(&out, file).as_deref(), Ok(out.as_str()));
}

#[test]
fn formats_an_empty_file_and_a_file_of_comments() {
    assert_eq!(fmt(""), "");
    assert_eq!(
        fmt("// only\n\n\n/// a comment\n"),
        "// only\n\n/// a comment\n"
    );
}

/// Every `.fl` file of the repository, test fixtures and tools included.
fn repository_sources() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).expect("readable dir") {
            let path = entry.expect("entry").path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with('.') || matches!(name, "node_modules" | "target" | "dist") {
                continue;
            }
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "fl") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join(".."), &mut out);
    out.sort();
    out
}

/// The whole corpus formats without leaving a declaration as written, keeps
/// its meaning (`format` checks it) and is a fixed point after one pass.
#[test]
fn the_repository_formats_canonically_and_idempotently() {
    let sources = repository_sources();
    assert!(sources.len() > 500, "only {} sources", sources.len());
    let mut problems = Vec::new();
    for path in sources {
        let text = fs::read_to_string(&path).expect("UTF-8 source");
        let mut map = SourceMap::default();
        let file = map.add(path.display().to_string(), text.as_str());
        // files with syntax errors are fixtures of those errors
        if !parse(&text, file).diagnostics.is_empty() {
            continue;
        }
        let name = path.display();
        match format_reporting(&text, file) {
            Err(e) => problems.push(format!("{name}: {}", e[0].message)),
            Ok((out, kept)) => {
                for (start, _) in kept {
                    let line = text[..start as usize].matches('\n').count() + 1;
                    problems.push(format!("{name}:{line}: left as written"));
                }
                if format(&out, file).as_deref() != Ok(out.as_str()) {
                    problems.push(format!("{name}: not idempotent"));
                }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
