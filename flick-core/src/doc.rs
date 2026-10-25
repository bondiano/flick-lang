//! `flick doc`: a Markdown page per module from its interface, with the
//! `///` comments of its source. Types show their kinds and definitions,
//! values their full source types: `forall`, class constraints, service and
//! error rows, captures.

use std::collections::HashMap;
use std::fmt::Write;
use std::ptr;

use crate::iface::{Decl, Export, Interface, Namespace};
use crate::syntax::cst::{Element, Node};
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{Form, show_type};

/// The `///` comment right above each top-level declaration, and above
/// each constructor on a line of its own (`| Circle Number`), by name.
pub fn comments(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut pending: Vec<&str> = Vec::new();
    for line in text.lines() {
        if let Some(doc) = line.trim_start().strip_prefix("///") {
            pending.push(doc.strip_prefix(' ').unwrap_or(doc));
            continue;
        }
        if !pending.is_empty()
            && let Some(name) = declared(line).or_else(|| variant(line))
        {
            out.entry(name).or_insert_with(|| pending.join("\n"));
        }
        if !line.trim().is_empty() {
            pending.clear();
        }
    }
    out
}

/// The `///` documentation immediately before the module header.
pub fn module_comment(text: &str) -> Option<String> {
    let mut pending = Vec::new();
    for line in text.lines() {
        if line.starts_with("module ") {
            return (!pending.is_empty()).then(|| pending.join("\n"));
        }
        if let Some(doc) = line.strip_prefix("///") {
            pending.push(doc.strip_prefix(' ').unwrap_or(doc));
        } else if !line.trim().is_empty() {
            pending.clear();
        }
    }
    None
}

/// The constructor a line `| Name …` of a type declaration starts.
fn variant(line: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix('|')?.trim_start();
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    name.starts_with(|c: char| c.is_ascii_uppercase())
        .then_some(name)
}

/// The `///` comment right above `child` among the children of `parent`,
/// where the parser keeps the comments before a node.
pub fn leading_doc(parent: &Node, child: &Node, text: &str) -> Option<String> {
    let at = parent
        .children
        .iter()
        .position(|c| matches!(c, Element::Node(n) if ptr::eq(n, child)))?;
    let mut lines = Vec::new();
    for element in parent.children[..at].iter().rev() {
        match element {
            Element::Token(t) if t.kind == K::DocComment => {
                let doc = t
                    .text(text)
                    .trim_end()
                    .strip_prefix("///")
                    .unwrap_or_default();
                lines.push(doc.strip_prefix(' ').unwrap_or(doc));
            }
            Element::Token(t) if t.kind.is_trivia() && !t.kind.is_comment() => {}
            _ => break,
        }
    }
    lines.reverse();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// The name a top-level line declares: `name :`, `type [alias|error] N`,
/// `service N`, `class … N`, `sql table N`, `kind N`.
fn declared(line: &str) -> Option<String> {
    if line.starts_with(' ') {
        return None;
    }
    let words: Vec<&str> = line.split_whitespace().collect();
    let skip = [
        "type", "alias", "error", "opaque", "refined", "unboxed", "untagged", "service", "class",
        "kind", "sql", "table",
    ];
    if words.get(1) == Some(&":") {
        return Some(words[0].to_owned());
    }
    if !words.first().is_some_and(|f| skip.contains(f)) {
        return None;
    }
    let name = words.iter().find(|w| !skip.contains(w))?;
    Some(
        name.trim_end_matches(|c: char| !c.is_alphanumeric() && c != '_')
            .to_owned(),
    )
}

/// The page of `interface`, its comments from `text`.
pub fn page(interface: &Interface, text: &str) -> String {
    let docs = comments(text);
    let mut out = format!("# {}\n", interface.module);
    let doc = |name: &str, out: &mut String| {
        if let Some(d) = docs.get(name) {
            let _ = write!(out, "\n{d}\n");
        }
    };
    let types: Vec<&Export> = interface
        .exports
        .iter()
        .filter(|e| e.entity.namespace == Namespace::Type && e.parent.is_none())
        .collect();
    if !types.is_empty() {
        out.push_str("\n## Types\n");
    }
    for e in types {
        let name = &e.entity.name;
        let _ = write!(out, "\n### `{name}`\n");
        doc(name, &mut out);
        if let Some(def) = &e.definition {
            let _ = writeln!(out, "\nKind: `{}`", def.kind);
            definition(interface, e, &def.form, &mut out);
        }
    }
    let values: Vec<&Export> = interface
        .exports
        .iter()
        .filter(|e| {
            e.entity.namespace == Namespace::Value
                && e.parent.is_none()
                && !e.entity.name.contains('$')
        })
        .collect();
    if !values.is_empty() {
        out.push_str("\n## Values\n");
    }
    for e in values {
        let name = &e.entity.name;
        let ty = e.scheme.as_ref().map_or_else(|| "?".to_owned(), show_type);
        let _ = write!(out, "\n### `{name}`\n\n```flick\n{name} : {ty}\n```\n");
        if let Some(reason) = &e.blocked {
            let _ = writeln!(out, "\nBlocked: {reason}");
        }
        doc(name, &mut out);
    }
    out
}

fn definition(interface: &Interface, e: &Export, form: &Form, out: &mut String) {
    let name = &e.entity.name;
    match form {
        Form::Alias { params, body, .. } => {
            let params: String = params.iter().fold(String::new(), |mut acc, p| {
                let _ = write!(acc, " {p}");
                acc
            });
            let _ = writeln!(
                out,
                "\n```flick\ntype alias {name}{params} = {}\n```",
                show_type(body)
            );
        }
        Form::Adt { .. } | Form::Error => {
            let constructors: Vec<String> = interface
                .members(name)
                .filter(|m| m.entity.namespace == Namespace::Constructor)
                .map(|m| {
                    let ty = m.scheme.as_ref().map_or_else(String::new, show_type);
                    format!("{} : {ty}", m.entity.name)
                })
                .collect();
            if constructors.is_empty() {
                out.push_str("\nOpaque: its constructors are not exported.\n");
            } else {
                let _ = writeln!(out, "\n```flick\n{}\n```", constructors.join("\n"));
            }
        }
        Form::Service { operations, .. } => {
            let ops: Vec<String> = operations
                .iter()
                .map(|o| format!("  {} : {}", o.name, show_type(&o.ty)))
                .collect();
            let _ = writeln!(out, "\n```flick\nservice {name} =\n{}\n```", ops.join("\n"));
        }
        Form::Class {
            params,
            supers,
            methods,
            ..
        } => {
            let context: Vec<String> = supers.iter().map(show_type).collect();
            let context = if context.is_empty() {
                String::new()
            } else {
                format!("({}) => ", context.join(", "))
            };
            let methods: Vec<String> = methods
                .iter()
                .map(|m| {
                    let ty = interface
                        .exports
                        .iter()
                        .find(|x| x.entity.name == *m && x.parent.as_deref() == Some(name))
                        .and_then(|x| x.scheme.as_ref())
                        .map_or_else(String::new, show_type);
                    format!("  {m} : {ty}")
                })
                .collect();
            let _ = writeln!(
                out,
                "\n```flick\nclass {context}{name} {} =\n{}\n```",
                params.join(" "),
                methods.join("\n")
            );
        }
        Form::Kind => {
            let members: Vec<&str> = interface
                .members(name)
                .map(|m| m.entity.name.as_str())
                .collect();
            let _ = writeln!(
                out,
                "\n```flick\nkind {name} = {}\n```",
                members.join(" | ")
            );
        }
        Form::Nominal | Form::KindMember => {
            if e.decl == Decl::Opaque {
                out.push_str("\nOpaque.\n");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_belong_to_the_declaration_below() {
        let docs = comments(
            "/// Adds.\n/// Twice.\nadd : Number -> Number\n\n/// A user.\ntype alias User = {}\n/// lost\n\nlet x = 1\n",
        );
        assert_eq!(docs.get("add").map(String::as_str), Some("Adds.\nTwice."));
        assert_eq!(docs.get("User").map(String::as_str), Some("A user."));
        assert!(!docs.contains_key("x"));
    }
}
