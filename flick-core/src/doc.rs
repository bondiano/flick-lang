//! `flick doc`: a Markdown page per module from its interface, with the
//! `///` comments of its source. Types show their kinds and definitions,
//! values their full source types: `forall`, class constraints, service and
//! error rows, captures.

use std::collections::HashMap;
use std::fmt::Write;
use std::ptr;

use crate::iface::{Decl, Export, Interface, Namespace};
use crate::source::FileId;
use crate::syntax;
use crate::syntax::cst::{Element, Node};
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{
    Form, ModuleNominalKind, ModuleRequirement, ModuleSignature, show_signature, show_type,
};

/// The `///` comment right above each top-level declaration, and above
/// each constructor on a line of its own (`| Circle Float`), by name. A
/// `let` takes the lines only right above it, as generated code writes it.
pub fn comments(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut pending: Vec<&str> = Vec::new();
    let mut adjacent = false;
    for line in text.lines() {
        if let Some(doc) = line.trim_start().strip_prefix("///") {
            pending.push(doc.strip_prefix(' ').unwrap_or(doc));
            adjacent = true;
            continue;
        }
        let name = declared(line).or_else(|| variant(line));
        if !pending.is_empty()
            && let Some(name) = name.or_else(|| adjacent.then(|| defined(line)).flatten())
        {
            out.entry(name).or_insert_with(|| pending.join("\n"));
        }
        adjacent = false;
        if !line.trim().is_empty() {
            pending.clear();
        }
    }
    out
}

/// The value a top-level `let [rec] name …` defines.
fn defined(line: &str) -> Option<String> {
    let mut words = line.strip_prefix("let ")?.split_whitespace();
    let name = words
        .next()
        .filter(|w| *w != "rec")
        .or_else(|| words.next())?;
    Some(name.to_owned())
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
    if words.starts_with(&["type", "module"]) {
        return words
            .get(2)
            .map(|name| name.trim_end_matches('=').to_owned());
    }
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
    let parsed = syntax::parse(text, FileId::new(0));
    let contracts: HashMap<_, _> = parsed
        .tree
        .nodes()
        .filter(|node| node.kind == K::ModuleTypeDecl)
        .filter_map(|node| {
            let name = node.child_tokens().find(|token| token.kind == K::Con)?;
            Some((name.text(text), node.text(text)))
        })
        .collect();
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
            if matches!(
                def.form,
                Form::ModuleContract { .. } | Form::ModuleSignature { .. }
            ) && e.entity.module == interface.module
                && let Some(declaration) = contracts.get(name.as_str())
            {
                let _ = writeln!(out, "\n```flick\n{declaration}\n```");
                match &def.form {
                    Form::ModuleSignature { signature } => {
                        expanded_signature(name, signature, &mut out);
                    }
                    Form::ModuleContract { requirements } => expanded_signature(
                        name,
                        &ModuleSignature::Structure {
                            requirements: requirements.clone(),
                        },
                        &mut out,
                    ),
                    _ => unreachable!("module contract declaration"),
                }
            } else {
                definition(interface, e, &def.form, &mut out);
            }
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
        let ty = e
            .scheme
            .as_ref()
            .map_or_else(|| "?".to_owned(), show_signature);
        let _ = write!(out, "\n### `{name}`\n\n```flick\n{name} : {ty}\n```\n");
        if let Some(reason) = &e.blocked {
            let _ = writeln!(out, "\nBlocked: {reason}");
        }
        let implementation = format!("{name}$server");
        if interface
            .exports
            .iter()
            .any(|x| x.entity.name == implementation)
        {
            let _ = writeln!(
                out,
                "\nServer function: a public endpoint, `POST /_flick/fn`. Anyone who reaches \
                 the server can call it with any payload, so it checks who calls itself."
            );
        }
        doc(name, &mut out);
    }
    out
}

fn expanded_signature(name: &str, signature: &ModuleSignature, out: &mut String) {
    let _ = writeln!(
        out,
        "\nExpanded module signature:\n\n```text\n{name} : {}\n```",
        signature.display()
    );
}

fn module_contract(name: &str, requirements: &[ModuleRequirement], out: &mut String) {
    if requirements
        .iter()
        .any(|r| matches!(r, ModuleRequirement::Module { .. }))
    {
        let signature = ModuleSignature::Structure {
            requirements: requirements.to_vec(),
        };
        expanded_signature(name, &signature, out);
        return;
    }
    let mut lines = Vec::new();
    for requirement in requirements {
        lines.push(match requirement {
            ModuleRequirement::Module {
                name, signature, ..
            } => {
                format!("  module {name} : {}", signature.display())
            }
            ModuleRequirement::Type {
                name,
                params,
                manifest,
            } => {
                let params = params
                    .iter()
                    .map(|p| p.name.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                let head = if params.is_empty() {
                    name.clone()
                } else {
                    format!("{name} {params}")
                };
                manifest.as_ref().map_or_else(
                    || format!("  type {head}"),
                    |ty| format!("  type {head} = {}", show_type(ty)),
                )
            }
            ModuleRequirement::Value { name, ty } => {
                format!("  {name} : {}", show_signature(ty))
            }
            ModuleRequirement::Nominal {
                name,
                category,
                identity,
                definition,
                methods,
            } => {
                let keyword = match category {
                    ModuleNominalKind::Class => "class",
                    ModuleNominalKind::Service => "service",
                };
                if let Some(e) = identity {
                    format!("  {keyword} {name} = {}.{}", e.module, e.name)
                } else if let Some(def) = definition {
                    let (params, members) = match &def.form {
                        Form::Class { params, .. } => (
                            params.as_slice(),
                            methods
                                .iter()
                                .map(|(n, ty)| format!("    {n} : {}", show_signature(ty)))
                                .collect::<Vec<_>>(),
                        ),
                        Form::Service {
                            params, operations, ..
                        } => (
                            params.as_slice(),
                            operations
                                .iter()
                                .map(|op| format!("    {} : {}", op.name, show_signature(&op.ty)))
                                .collect::<Vec<_>>(),
                        ),
                        _ => (&[][..], Vec::new()),
                    };
                    let suffix = if params.is_empty() {
                        String::new()
                    } else {
                        format!(" {}", params.join(" "))
                    };
                    if members.is_empty() {
                        format!("  {keyword} {name}{suffix}")
                    } else {
                        format!("  {keyword} {name}{suffix} =\n{}", members.join("\n"))
                    }
                } else {
                    format!("  {keyword} {name}")
                }
            }
            ModuleRequirement::Dictionary { head, from } => {
                format!("  instance {} from {from}", show_type(head))
            }
        });
    }
    let _ = writeln!(
        out,
        "\n```flick\ntype module {name} =\n{}\n```",
        lines.join("\n")
    );
}

fn definition(interface: &Interface, e: &Export, form: &Form, out: &mut String) {
    let name = &e.entity.name;
    match form {
        Form::ModuleContract { requirements } => module_contract(name, requirements, out),
        Form::ModuleSignature { signature } => expanded_signature(name, signature, out),
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
                    let ty = m.scheme.as_ref().map_or_else(String::new, show_signature);
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
                .map(|o| format!("  {} : {}", o.name, show_signature(&o.ty)))
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
                        .map_or_else(String::new, show_signature);
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
        Form::Nominal { .. } | Form::KindMember => {
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
            "/// Adds.\n/// Twice.\nadd : Float -> Float\n\n/// A user.\ntype alias User = {}\n/// lost\n\nlet x = 1\n",
        );
        assert_eq!(docs.get("add").map(String::as_str), Some("Adds.\nTwice."));
        assert_eq!(docs.get("User").map(String::as_str), Some("A user."));
        assert!(!docs.contains_key("x"));
        let docs = comments("/// Made.\nlet rec made = 1\n");
        assert_eq!(docs.get("made").map(String::as_str), Some("Made."));
        let docs = comments("/// Contract.\ntype module HashKey =\n  type Key\n");
        assert_eq!(docs.get("HashKey").map(String::as_str), Some("Contract."));
    }
}

#[cfg(test)]
mod module_signature_tests {
    use crate::ir::tests::check_text;
    use crate::source::FileId;
    use crate::syntax;

    #[test]
    fn exported_contract_aliases_document_members_of_private_source_signatures() {
        let source = "module Contracts exposing (Public)\ntype module Hidden =\n  seed : Int\ntype module Public = Hidden\n";
        let (_, interface) = check_text("Contracts", source).expect("checks");
        let page = super::page(&interface, source);
        assert!(page.contains("type module Public = Hidden"), "{page}");
        assert!(page.contains("seed : Int"), "{page}");
    }
    #[test]
    fn module_signature_documentation_preserves_valid_source_declarations() {
        let source = "module App.Contracts exposing (..)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\ntype module Factory = (K : Input) -> Output\ntype module Namespace =\n  module Make : Factory\n";
        let (_, interface) = check_text("App.Contracts", source).expect("checks");
        let page = super::page(&interface, source);
        assert!(
            page.contains("type module Factory = (K : Input) -> Output"),
            "{page}"
        );
        for block in page.split("```flick\n").skip(1) {
            let code = block.split("```").next().expect("code fence");
            let parsed = syntax::parse(code, FileId::new(0));
            assert!(
                parsed.diagnostics.is_empty(),
                "{code}: {:?}",
                parsed.diagnostics
            );
        }
        let without_source = super::page(&interface, "");
        assert!(
            without_source.contains("Expanded module signature"),
            "{without_source}"
        );
        assert!(without_source.contains("```text\n"), "{without_source}");
    }
}
