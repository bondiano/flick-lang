//! Definitions of local foreign modules and their named JS/Go bindings.

use std::collections::{HashMap, HashSet};
use std::fs;

use flick_core::ide::{Location, Target};
use flick_core::project::config::Emit;
use flick_core::project::normalize;
use flick_core::source::FileId;
use flick_core::syntax::kind::SyntaxKind as K;
use flick_core::syntax::literal::string_value;

use super::super::Snapshot;

pub(super) fn definition(snapshot: &Snapshot, file: FileId, offset: u32) -> Option<Location> {
    let module = snapshot.analysis.by_file(file)?;
    let text = &snapshot.sources.get(file).text;
    let source = module
        .tree
        .nodes()
        .filter(|n| n.kind == K::ForeignDecl)
        .find_map(|decl| {
            let source = decl.nodes().find(|n| n.kind == K::ForeignSource)?;
            let strings: Vec<_> = source
                .child_tokens()
                .filter(|t| t.kind == K::String)
                .collect();
            let [spec, export] = strings.as_slice() else {
                return None;
            };
            let which = if spec.start <= offset && offset <= spec.end {
                0
            } else if export.start <= offset && offset <= export.end {
                1
            } else {
                return None;
            };
            Some((
                string_value(spec.text(text)),
                string_value(export.text(text)),
                which,
            ))
        })?;
    let (spec, export, which) = source;
    if !(spec.starts_with("./") || spec.starts_with("../")) {
        return None;
    }
    let path = normalize(&module.path.parent()?.join(spec));
    let extension = path.extension()?.to_str()?;
    if !matches!(extension, "js" | "mjs" | "ts" | "mts") {
        return None;
    }
    let path = if snapshot.emit == Emit::Go {
        path.with_extension("go")
    } else {
        path
    };
    if which == 0 {
        path.is_file().then_some(Location {
            target: Target::Path(path),
            start: 0,
            end: 0,
        })
    } else {
        let text = fs::read_to_string(&path).ok()?;
        let (start, end) = if snapshot.emit == Emit::Go {
            go_name(&text, &export)?
        } else {
            js_name(&text, &export)?
        };
        Some(Location {
            target: Target::Path(path),
            start,
            end,
        })
    }
}

#[derive(Clone, Copy)]
struct Token<'a> {
    text: &'a str,
    start: u32,
    end: u32,
    depth: u32,
}

/// A small structural scanner for declarations. Strings and comments never
/// become tokens, and braces identify declarations at file scope.
fn tokens(text: &str) -> Vec<Token<'_>> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut depth: u32 = 0;
    while i < b.len() {
        if b[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if b[i] == b'/' && b.get(i + 1) == Some(&b'/') {
            i += 2;
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < b.len() && &b[i..i + 2] != b"*/" {
                i += 1;
            }
            i = (i + 2).min(b.len());
            continue;
        }
        if matches!(b[i], b'\'' | b'"' | b'`') {
            let quote = b[i];
            i += 1;
            while i < b.len() {
                if b[i] == b'\\' {
                    i = (i + 2).min(b.len());
                } else if b[i] == quote {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
            continue;
        }
        let start = i;
        if b[i].is_ascii_alphabetic() || matches!(b[i], b'_' | b'$') {
            i += 1;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || matches!(b[i], b'_' | b'$')) {
                i += 1;
            }
        } else {
            i += 1;
        }
        if b[start] == b'}' {
            depth = depth.saturating_sub(1);
        }
        // Non-ASCII bytes are ignored: identifiers used by foreign bindings
        // are ASCII, and slicing here must always start at a UTF-8 boundary.
        if b[start].is_ascii() {
            out.push(Token {
                text: &text[start..i],
                start: u32::try_from(start).unwrap_or(u32::MAX),
                end: u32::try_from(i).unwrap_or(u32::MAX),
                depth,
            });
        }
        if b[start] == b'{' {
            depth += 1;
        }
    }
    out
}

fn js_name(text: &str, export: &str) -> Option<(u32, u32)> {
    let t = tokens(text);
    let mut declarations = HashMap::new();
    for (i, token) in t.iter().enumerate() {
        if token.depth != 0 {
            continue;
        }
        let name = match token.text {
            "function" | "class" | "const" | "let" | "var" => t.get(i + 1),
            _ => None,
        };
        if let Some(name) = name.filter(|n| n.depth == 0) {
            declarations
                .entry(name.text)
                .or_insert((name.start, name.end));
        }
    }
    let mut exported = HashSet::new();
    let mut aliases = HashMap::new();
    for (i, token) in t.iter().enumerate() {
        if token.depth != 0 || token.text != "export" {
            continue;
        }
        let mut j = i + 1;
        if t.get(j).is_some_and(|n| n.text == "async") {
            j += 1;
        }
        if t.get(j)
            .is_some_and(|n| matches!(n.text, "function" | "class" | "const" | "let" | "var"))
        {
            if let Some(name) = t.get(j + 1) {
                exported.insert(name.text);
            }
            continue;
        }
        if t.get(j).is_none_or(|n| n.text != "{") {
            continue;
        }
        let mut pairs = Vec::new();
        j += 1;
        while let Some(local) = t.get(j).filter(|n| n.depth == 1 && n.text != "}") {
            let mut visible = local.text;
            j += 1;
            if t.get(j).is_some_and(|n| n.text == "as") {
                if let Some(alias) = t.get(j + 1) {
                    visible = alias.text;
                }
                j += 2;
            }
            pairs.push((visible, local.text));
            if t.get(j).is_some_and(|n| n.text == ",") {
                j += 1;
            }
        }
        // `export { name } from "…"` has no local declaration.
        if t.get(j + 1).is_some_and(|n| n.text == "from") {
            continue;
        }
        for (visible, local) in pairs {
            aliases.insert(visible, local);
        }
    }
    if exported.contains(export) {
        declarations.get(export).copied()
    } else {
        declarations.get(aliases.get(export)?).copied()
    }
}

fn go_name(text: &str, name: &str) -> Option<(u32, u32)> {
    let t = tokens(text);
    t.windows(2).find_map(|pair| {
        let [head, ident] = pair else { return None };
        (head.depth == 0
            && ident.depth == 0
            && matches!(head.text, "func" | "var")
            && ident.text == name)
            .then_some((ident.start, ident.end))
    })
}

#[cfg(test)]
mod tests {
    use super::{go_name, js_name};

    #[test]
    fn finds_declarations_without_matching_comments_or_strings() {
        let js = "// export function count() {}\nconst note = 'export const count = 0';\nexport function count() {}\n";
        let at = u32::try_from(js.rfind("count").expect("count")).expect("small");
        assert_eq!(js_name(js, "count"), Some((at, at + 5)));
        assert_eq!(js_name(js, "missing"), None);
        let go = "// var count = 0\nvar count = 1\n";
        let start = u32::try_from(go.rfind("count").expect("count")).expect("small");
        assert_eq!(go_name(go, "count"), Some((start, start + 5)));
    }
}
