//! Document and workspace symbols: the declarations of modules, with the
//! constructors of a type and the methods of a class nested under it.

use std::iter;

use super::Ide;
use crate::iface::{Decl, Namespace};
use crate::resolve::{Declared, declarations};
use crate::source::FileId;
use crate::syntax::cst::Node;

/// A declaration as an outline entry; `kind` is an LSP `SymbolKind`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outline {
    pub name: String,
    pub kind: u8,
    /// The whole declaration.
    pub range: (u32, u32),
    /// Its name.
    pub selection: (u32, u32),
    pub children: Vec<Outline>,
}

impl Ide<'_> {
    /// The declarations of `file`, in source order.
    pub fn document_symbols(&self, file: FileId) -> Vec<Outline> {
        let Some(m) = self.module(file) else {
            return Vec::new();
        };
        let decls: Vec<Declared> = declarations(&m.tree, self.text(file), file)
            .into_iter()
            .filter(|d| !d.signature)
            .collect();
        let tops: Vec<(u32, u32)> = m.tree.nodes().map(Node::range).collect();
        let enclosing = |d: &Declared| {
            tops.iter()
                .copied()
                .find(|(s, e)| *s <= d.span.start && d.span.end <= *e)
                .unwrap_or((d.span.start, d.span.end))
        };
        let outline = |d: &Declared, children| Outline {
            name: d.name.clone(),
            kind: kind(d),
            range: enclosing(d),
            selection: (d.span.start, d.span.end),
            children,
        };
        decls
            .iter()
            .filter(|d| d.parent.is_none() || !nested(d))
            .map(|d| {
                let range = enclosing(d);
                let children = decls
                    .iter()
                    .filter(|c| {
                        nested(c)
                            && c.parent.as_deref() == Some(d.name.as_str())
                            && range.0 <= c.span.start
                            && c.span.end <= range.1
                    })
                    .map(|c| outline(c, Vec::new()))
                    .collect();
                outline(d, children)
            })
            .collect()
    }

    /// Declarations of every module whose name has the letters of `query` in
    /// order, ignoring case.
    pub fn workspace_symbols(&self, query: &str) -> Vec<(FileId, Outline)> {
        let query = query.to_lowercase();
        self.analysis
            .modules
            .iter()
            .flat_map(|m| {
                self.document_symbols(m.file)
                    .into_iter()
                    .flat_map(|o| {
                        let children = o.children.clone();
                        iter::once(o).chain(children)
                    })
                    .filter(|o| subsequence(&query, &o.name.to_lowercase()))
                    .map(move |o| {
                        (
                            m.file,
                            Outline {
                                children: Vec::new(),
                                ..o
                            },
                        )
                    })
            })
            .collect()
    }
}

/// Constructors and methods go under their type or class.
fn nested(d: &Declared) -> bool {
    matches!(d.decl, Decl::Constructor | Decl::Method | Decl::KindMember)
}

fn kind(d: &Declared) -> u8 {
    match (d.namespace, d.decl) {
        (_, Decl::Constructor | Decl::KindMember) => 22,
        (_, Decl::Method) => 6,
        (_, Decl::Class | Decl::Service) => 11,
        (_, Decl::Adt | Decl::Error | Decl::Kind) => 10,
        (Namespace::Type, _) => 5,
        (_, Decl::Foreign | Decl::Blocked) => 12,
        _ => 13,
    }
}

fn subsequence(query: &str, name: &str) -> bool {
    let mut chars = name.chars();
    query.chars().all(|q| chars.any(|c| c == q))
}
