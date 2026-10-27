//! Hover on a field label: the field's type, its `///` comment and the
//! declaration its record belongs to, as TypeScript shows a property.

use std::fmt::Write;

use super::{Ide, tree};
use crate::doc;
use crate::iface::Namespace;
use crate::source::FileId;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{Form, Ref, Type, show_type};

/// Where a field is written: its declaration, type and comment.
struct Declared {
    module: String,
    owner: Option<String>,
    written: String,
    doc: Option<String>,
}

impl Ide<'_> {
    /// Hover on what the checker types beside names: CLI, SQL, field labels.
    pub(super) fn embedded_hover(&self, file: FileId, offset: u32) -> Option<(String, (u32, u32))> {
        self.cli_hover(file, offset)
            .or_else(|| self.sql_hover(file, offset))
            .or_else(|| self.field_hover(file, offset))
    }

    /// Markdown for the field label at `offset`, and its range.
    fn field_hover(&self, file: FileId, offset: u32) -> Option<(String, (u32, u32))> {
        let m = self.module(file)?;
        let text = self.text(file);
        if let Some(hover) = self.declared_field(file, offset) {
            return Some(hover);
        }
        let (range, record) = tree::innermost(m.typing.fields.iter(), offset)?;
        let label = &text[range.0 as usize..range.1 as usize];
        let mut fields = Vec::new();
        self.record_fields(file, record, &mut fields, 0);
        let mut labels: Vec<&str> = fields.iter().map(|(l, _)| l.as_str()).collect();
        labels.sort_unstable();
        let declared = self.field_of(file, offset, record, label, &labels);
        let ty = fields
            .iter()
            .find(|(l, _)| l == label)
            .map(|(_, t)| show_type(t))
            .or_else(|| declared.as_ref().map(|d| d.written.clone()))?;
        Some((describe(label, &ty, declared.as_ref()), range))
    }

    /// A field in a record type as written: `{ name : String }`.
    fn declared_field(&self, file: FileId, offset: u32) -> Option<(String, (u32, u32))> {
        let m = self.module(file)?;
        let text = self.text(file);
        let path = tree::path_to(&m.tree, offset);
        let at = path.iter().rposition(|n| n.kind == K::RecordTypeField)?;
        let field = path[at];
        let label = field.child_tokens().find(|t| t.kind == K::Ident)?;
        if !(label.start <= offset && offset <= label.end) {
            return None;
        }
        let declared = written(m.name.as_str(), &path[..=at], field, text);
        let name = label.text(text);
        let hover = describe(name, &declared.written, Some(&declared));
        Some((hover, (label.start, label.end)))
    }

    /// The fields of a record type, through transparent aliases, the local
    /// ones of `file` too.
    fn record_fields(&self, file: FileId, ty: &Type, out: &mut Vec<(String, Type)>, depth: u32) {
        let mut ty = ty;
        while let Type::Captured { ty: inner, .. } | Type::Bounded { ty: inner, .. } = ty {
            ty = inner;
        }
        match ty {
            Type::Record(row) => out.extend(row.entries.iter().cloned()),
            Type::App { .. } | Type::Con { .. } if depth < 8 => {
                let (Type::Con { module, name }, args) = ty.spine() else {
                    return;
                };
                let Some((params, body)) = self.alias(file, module, name) else {
                    return;
                };
                let subst: Vec<(String, Type)> =
                    params.into_iter().zip(args.into_iter().cloned()).collect();
                self.record_fields(file, &body.substitute(&subst), out, depth + 1);
            }
            _ => {}
        }
    }

    fn alias(&self, file: FileId, module: &str, name: &str) -> Option<(Vec<String>, Type)> {
        let m = self.module(file)?;
        if m.name == module
            && let Some(alias) = m.typing.aliases.get(name)
        {
            return Some(alias.clone());
        }
        let def = self
            .interface(module)?
            .find(Namespace::Type, name)?
            .definition
            .clone()?;
        match def.form {
            Form::Alias { params, body, .. } => Some((params, body)),
            _ => None,
        }
    }

    /// Where the field `label` of `record` is written. An alias names its
    /// declaration; a record a function takes is the one its signature
    /// writes; otherwise a record type with the same labels in a module the
    /// file refers to.
    fn field_of(
        &self,
        file: FileId,
        offset: u32,
        record: &Type,
        label: &str,
        labels: &[&str],
    ) -> Option<Declared> {
        let m = self.module(file)?;
        if let (Type::Con { module, name }, _) = record.spine() {
            let source = self.source(module)?;
            let decl = source.tree.nodes().find(|d| {
                d.kind == K::TypeDecl && tree::name_of(d, K::Con, &source.text) == Some(name)
            })?;
            return find_field(module, decl, label, labels, &source.text);
        }
        // `div { class_name = … }`: the record of the function applied
        let path = tree::path_to(&m.tree, offset);
        let callee = path
            .iter()
            .rev()
            .find(|n| n.kind == K::AppExpr)
            .and_then(|app| app.nodes().next())
            .and_then(|head| match m.typing.names.get(&head.range())? {
                Ref::Value { module, name } => Some((module.clone(), name.clone())),
                _ => None,
            });
        if let Some((module, name)) = callee
            && let Some(source) = self.source(&module)
            && let Some(sig) = source.tree.nodes().find(|d| {
                d.kind == K::Signature && tree::name_of(d, K::Ident, &source.text) == Some(&name)
            })
            && let Some(found) = find_field(&module, sig, label, labels, &source.text)
        {
            return Some(found);
        }
        // shortcut: matches records by their labels, so two records with the
        // same labels may be confused; record the origin in the checker if so
        let mut modules = vec![m.name.clone()];
        for r in m.typing.names.values() {
            if let Ref::Value { module, .. } = r
                && !modules.contains(module)
            {
                modules.push(module.clone());
            }
        }
        let mut best: Option<Declared> = None;
        for module in &modules {
            let Some(source) = self.source(module) else {
                continue;
            };
            for decl in source.tree.nodes() {
                if let Some(found) = find_field(module, decl, label, labels, &source.text) {
                    if found.doc.is_some() {
                        return Some(found);
                    }
                    best.get_or_insert(found);
                }
            }
        }
        best
    }
}

/// The field `label` of a record type in `decl` with exactly `labels`.
fn find_field(
    module: &str,
    decl: &Node,
    label: &str,
    labels: &[&str],
    text: &str,
) -> Option<Declared> {
    fn walk<'n>(
        path: &mut Vec<&'n Node>,
        node: &'n Node,
        label: &str,
        labels: &[&str],
        text: &str,
    ) -> Option<Vec<&'n Node>> {
        path.push(node);
        if node.kind == K::RecordType {
            let fields: Vec<&Node> = node
                .nodes()
                .filter(|n| n.kind == K::RecordTypeField)
                .collect();
            let mut names: Vec<&str> = fields
                .iter()
                .filter_map(|f| tree::name_of(f, K::Ident, text))
                .collect();
            names.sort_unstable();
            if names == labels
                && let Some(field) = fields
                    .iter()
                    .find(|f| tree::name_of(f, K::Ident, text) == Some(label))
            {
                path.push(field);
                return Some(path.clone());
            }
        }
        for child in node.nodes() {
            if let Some(found) = walk(path, child, label, labels, text) {
                return Some(found);
            }
        }
        path.pop();
        None
    }
    let path = walk(&mut Vec::new(), decl, label, labels, text)?;
    let field = *path.last()?;
    Some(written(module, &path[..path.len()], field, text))
}

/// A written field: its type as the source writes it, its comment, and the
/// innermost declaration or constructor around it.
fn written(module: &str, path: &[&Node], field: &Node, text: &str) -> Declared {
    let written = field.nodes().next().map_or_else(String::new, |t| {
        t.text(text)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    });
    let doc = path
        .iter()
        .rev()
        .nth(1)
        .and_then(|parent| doc::leading_doc(parent, field, text));
    let owner = path.iter().rev().find_map(|n| match n.kind {
        K::Variant | K::TypeDecl => tree::name_of(n, K::Con, text),
        K::Signature | K::ForeignDecl => tree::name_of(n, K::Ident, text),
        _ => None,
    });
    Declared {
        module: module.to_owned(),
        owner: owner.map(str::to_owned),
        written,
        doc,
    }
}

fn describe(label: &str, ty: &str, declared: Option<&Declared>) -> String {
    let mut out = format!("```flick\n{label} : {ty}\n```");
    if let Some(d) = declared {
        if let Some(doc) = &d.doc {
            out.push_str("\n\n");
            out.push_str(doc);
        }
        out.push_str("\n\n");
        match &d.owner {
            Some(owner) => _ = write!(out, "field of `{owner}` from `{}`", d.module),
            None => _ = write!(out, "from `{}`", d.module),
        }
    }
    out
}

#[cfg(test)]
#[path = "fields_tests.rs"]
mod tests;
