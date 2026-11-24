//! References and renaming of a field of a record type alias: its label in
//! the alias and every label whose record the checker typed as the alias.
//! A rename is one edit of all of them, or none: a label whose record is
//! not shown to be one alias, a record type written with the same fields,
//! a foreign boundary or an encoding that names the field by its spelling
//! refuses it, with the place that does.

use super::Occurrence;
use crate::diagnostics::Severity;
use crate::ide::hints::strip;
use crate::ide::provenance::{Owner, alias_record};
use crate::ide::{Ide, Location, Target, tree};
use crate::iface::Namespace;
use crate::project::Analyzed;
use crate::source::FileId;
use crate::syntax::cst::{Node, foreign_words};
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{Elem, Type};

impl Ide<'_> {
    /// The declaration of the field `label` of `module.record`, then each
    /// label of a record the checker typed as that alias.
    pub(in crate::ide) fn field_references(
        &self,
        module: &str,
        record: &str,
        label: &str,
    ) -> Vec<Occurrence> {
        let mut out = Vec::new();
        if let Some(Location {
            target: Target::File(file),
            start,
            end,
        }) = self.field_declaration(module, record, label)
        {
            out.push((file, (start, end)));
        }
        for m in &self.analysis.modules {
            let own = self.text(m.file).len();
            for (range, ty) in self.labels_named(m, label) {
                let owned = self.owner_at(m, range, ty, label);
                if range.1 as usize <= own && owned.is_ok_and(|(o, r)| o == module && r == record) {
                    out.push((m.file, range));
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Why the fields of `module.record` cannot be renamed, if so.
    pub(in crate::ide) fn renamable_record(
        &self,
        module: &str,
        record: &str,
    ) -> Result<(), String> {
        let m = (self.analysis.module(module))
            .ok_or_else(|| format!("`{record}` is declared outside the project"))?;
        let text = self.text(m.file);
        let written = (m.tree.nodes())
            .any(|d| d.kind == K::TypeDecl && tree::name_of(d, K::Con, text) == Some(record));
        if !written {
            return Err(format!(
                "`{record}` is declared by a plugin: rename the field in its block"
            ));
        }
        Ok(())
    }

    /// The edits renaming the field `label` of `module.record` to
    /// `new_name` in this analysis, or why the rename is refused.
    pub(in crate::ide) fn field_rename(
        &self,
        module: &str,
        record: &str,
        label: &str,
        new_name: &str,
    ) -> Result<Vec<Occurrence>, String> {
        self.renamable_record(module, record)?;
        let refuse = |why: String| {
            Err(format!(
                "cannot rename the field `{label}` of `{record}`: {why}"
            ))
        };
        if (self.analysis.diagnostics.iter()).any(|d| d.severity == Severity::Error) {
            return refuse("the project has errors, and an unchecked label may read it".into());
        }
        let labels = self.alias_labels(module, record);
        if labels.iter().any(|l| l == new_name) {
            return refuse(format!("`{record}` already has a field `{new_name}`"));
        }
        // records compare, hash and show their fields in the order of their labels
        let before = labels.iter().filter(|l| l.as_str() < label).count();
        let after = labels
            .iter()
            .filter(|l| l.as_str() != label && l.as_str() < new_name)
            .count();
        if before != after {
            return refuse(format!(
                "`{new_name}` takes another place among the fields of `{record}`, which orders its comparisons and its text"
            ));
        }
        let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
        let owner: Owner = (module.to_owned(), record.to_owned());
        for m in &self.analysis.modules {
            if let Some(why) = self.unaccounted(m, &owner, label, &labels) {
                return refuse(why);
            }
        }
        let edits = self.field_references(module, record, label);
        for (file, (start, end)) in &edits {
            let text = self.text(*file);
            if text.get(*start as usize..*end as usize) != Some(label) {
                return refuse("an occurrence has no exact mapping to the label".into());
            }
            if pun(self.module(*file).map(|m| &m.tree), *start) {
                return refuse(format!(
                    "`{{ {label} }}` at {} also binds `{label}`: write `{{ {label} = {label} }}` first",
                    self.place(*file, *start)
                ));
            }
        }
        Ok(edits)
    }

    /// The labels of the record the alias `module.record` is.
    fn alias_labels(&self, module: &str, record: &str) -> Vec<String> {
        match self.alias_body(module, record).map(strip) {
            Some(Type::Record(row)) => (row.entries.iter())
                .filter_map(|(e, _)| match e {
                    Elem::Named(l) => Some(l.clone()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// What in `m` may read the field `label` of `owner` without being shown
    /// to: the first such place, described.
    fn unaccounted(
        &self,
        m: &Analyzed,
        owner: &Owner,
        label: &str,
        labels: &[&str],
    ) -> Option<String> {
        let own = self.text(m.file).len();
        for (range, ty) in self.labels_named(m, label) {
            let shown = self.owner_at(m, range, ty, label);
            if range.1 as usize > own && (shown.as_ref() == Ok(owner) || self.may_be(ty, labels, 0))
            {
                return Some(format!(
                    "code a plugin generates in `{}` reads it by its spelling",
                    m.name
                ));
            }
            if let Err(why) = shown
                && self.may_be(ty, labels, 0)
            {
                return Some(format!(
                    "`{label}` at {} may read it: {}",
                    self.place(m.file, range.0),
                    why.reason()
                ));
            }
        }
        if let Some(at) = (m.typing.labels.iter()).find_map(|(at, l)| (l == label).then_some(at.0))
        {
            return Some(format!(
                "`Record.label` at {} names it by its spelling",
                self.place(m.file, at)
            ));
        }
        let text = self.text(m.file);
        let own_body = (m.name == owner.0)
            .then(|| {
                m.tree.nodes().find(|d| {
                    d.kind == K::TypeDecl && tree::name_of(d, K::Con, text) == Some(&owner.1)
                })
            })
            .flatten()
            .and_then(alias_record)
            .map(Node::range);
        if let Some(at) = written_alike(&m.tree, text, label, labels, own_body) {
            return Some(format!(
                "the record type at {} has its fields, so a value of either is a value of both",
                self.place(m.file, at)
            ));
        }
        self.boundary(m, owner, labels)
    }

    /// A foreign declaration or a derived encoding of `m` that reaches the
    /// alias `owner`: either names its fields by their spelling.
    fn boundary(&self, m: &Analyzed, owner: &Owner, labels: &[&str]) -> Option<String> {
        let text = self.text(m.file);
        // the members of a group of recursive definitions are declarations too
        let members = (m.tree.nodes())
            .filter(|n| n.kind == K::RecGroup)
            .flat_map(Node::nodes)
            .filter(|n| n.kind == K::Members)
            .flat_map(Node::nodes);
        for decl in m.tree.nodes().chain(members) {
            let name = match decl.kind {
                K::ForeignDecl => foreign_words(decl).1.map(|t| t.text(text)),
                K::LetDecl
                    if m.typing
                        .native_overrides
                        .iter()
                        .any(|(r, _)| *r == decl.range()) =>
                {
                    decl.nodes()
                        .next()
                        .and_then(|p| tree::name_of(p, K::Ident, text))
                }
                K::TypeDecl if derives_schema(decl, text) => {
                    let name = tree::name_of(decl, K::Con, text)?;
                    let ty = Type::Con {
                        module: m.name.clone(),
                        name: name.to_owned(),
                    };
                    if self.reaches(&ty, owner, labels, &mut Vec::new()) {
                        return Some(format!(
                            "`{name}` derives `Schema`, whose encoding names the fields of `{}`",
                            owner.1
                        ));
                    }
                    continue;
                }
                _ => continue,
            };
            let scheme = name
                .and_then(|n| m.typing.declarations.find(Namespace::Value, n))
                .and_then(|e| e.scheme.as_ref());
            if let (Some(name), Some(scheme)) = (name, scheme)
                && self.reaches(scheme, owner, labels, &mut Vec::new())
            {
                return Some(format!(
                    "`{name}` at {} passes it to foreign code, which reads fields by their names",
                    self.place(m.file, decl.range().0)
                ));
            }
        }
        None
    }

    /// Whether `ty` may hold a value of the alias `owner` with `labels`: as
    /// itself, as a record with its fields, through aliases or the
    /// constructors of the types it names.
    fn reaches(&self, ty: &Type, owner: &Owner, labels: &[&str], seen: &mut Vec<Owner>) -> bool {
        let mut found = false;
        ty.visit(&mut |t| {
            if !found && matches!(t, Type::Record(_)) && self.may_be(t, labels, 0) {
                found = true;
            }
            let Type::Con { module, name } = t else {
                return;
            };
            if found || seen.iter().any(|(m, n)| m == module && n == name) {
                return;
            }
            if *module == owner.0 && *name == owner.1 {
                found = true;
                return;
            }
            seen.push((module.clone(), name.clone()));
            if let Some(body) = self.alias_body(module, name) {
                found = self.reaches(body, owner, labels, seen);
            } else if let Some(m) = self.analysis.module(module) {
                found = (m.typing.declarations.exports.iter())
                    .filter(|e| e.entity.namespace == Namespace::Constructor)
                    .filter(|e| e.parent.as_deref() == Some(name.as_str()))
                    .filter_map(|e| e.scheme.as_ref())
                    .any(|s| self.reaches(s, owner, labels, seen));
            }
        });
        found
    }

    /// Whether a record of type `record`, not shown to be one alias, may be
    /// of a record with `labels`: closed with those labels, open with some
    /// of them, or not known.
    fn may_be(&self, record: &Type, labels: &[&str], depth: u32) -> bool {
        match strip(record) {
            Type::Record(row) => {
                let named: Vec<&str> = (row.entries.iter())
                    .filter_map(|(e, _)| match e {
                        Elem::Named(l) => Some(l.as_str()),
                        _ => None,
                    })
                    .collect();
                (row.tail.is_some() || named.len() == labels.len())
                    && named.iter().all(|l| labels.contains(l))
            }
            t @ (Type::App { .. } | Type::Con { .. }) => match t.spine().0 {
                Type::Con { module, name } if depth < 8 => (self.alias_body(module, name))
                    .is_some_and(|body| self.may_be(body, labels, depth + 1)),
                _ => false,
            },
            Type::Var(_) | Type::Meta(_) | Type::Skolem(_) | Type::Error => true,
            _ => false,
        }
    }

    /// `path:line` of `offset` in `file`, as a refusal names it.
    fn place(&self, file: FileId, offset: u32) -> String {
        let source = self.sources.get(file);
        format!("{}:{}", source.path, source.line_col(offset).0 + 1)
    }
}

/// Whether the label at `start` is a record pattern's field that also binds
/// its name, `{ name }`.
fn pun(tree: Option<&Node>, start: u32) -> bool {
    tree.is_some_and(|tree| {
        let path = tree::path_to(tree, start);
        path.last()
            .is_some_and(|n| n.kind == K::RecordPatField && n.nodes().next().is_none())
    })
}

/// Whether a type declaration derives `Schema`.
fn derives_schema(decl: &Node, text: &str) -> bool {
    (decl.nodes())
        .filter(|n| n.kind == K::DerivingClause)
        .flat_map(Node::child_tokens)
        .any(|t| t.kind == K::Con && t.text(text) == "Schema")
}

/// The start of a record type written in `root`, other than `own`, that a
/// record with `labels` is, or that an open one with `label` may be.
fn written_alike(
    root: &Node,
    text: &str,
    label: &str,
    labels: &[&str],
    own: Option<(u32, u32)>,
) -> Option<u32> {
    if root.kind == K::RecordType && Some(root.range()) != own {
        let written: Vec<&str> = (root.nodes())
            .filter(|f| f.kind == K::RecordTypeField)
            .filter_map(|f| tree::name_of(f, K::Ident, text))
            .collect();
        let open = root.nodes().any(|n| n.kind == K::RowTail);
        let alike = if open {
            written.contains(&label) && written.iter().all(|l| labels.contains(l))
        } else {
            written.len() == labels.len() && written.iter().all(|l| labels.contains(l))
        };
        if alike {
            return Some(root.range().0);
        }
    }
    root.nodes()
        .find_map(|n| written_alike(n, text, label, labels, own))
}
