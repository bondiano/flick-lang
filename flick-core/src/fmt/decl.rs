//! Declarations: types and their variants, classes, instances, services and
//! foreign bindings.

use flick_pretty::docvec;

use super::WIDTH;
use super::printer::{Doc, INDENT, Printer, node_position, nodes, position};
use crate::syntax::cst::{Element, Node};
use crate::syntax::kind::SyntaxKind as K;

impl<'a> Printer<'a> {
    /// `type`, `kind` and `type error` declarations. Variants go on one line
    /// when they fit, else one per line after a `|`; a record hugs the `=`;
    /// `deriving` takes its own line, one level in, but follows the `}` of a
    /// record that breaks.
    pub fn type_decl(&mut self, n: &'a Node) -> Doc<'a> {
        let a = self.a;
        let deriving = node_position(n, K::DerivingClause);
        let rest = deriving.unwrap_or(n.children.len());
        let Some(eq) = position(n, K::Eq) else {
            let head = self.elements(n, &n.children[..rest]);
            let deriving = self.deriving(n, deriving);
            return docvec![a, head, deriving];
        };
        let head = self.elements(n, &n.children[..=eq]);
        let value = &n.children[eq + 1..rest];
        let parts: Vec<&Node> = value
            .iter()
            .filter_map(|c| match c {
                Element::Node(c) => Some(c),
                Element::Token(_) => None,
            })
            .collect();
        let doc = match parts.as_slice() {
            _ if n.kind != K::ErrorDecl && parts.iter().any(|p| p.kind == K::Variant) => {
                self.variants(head, value)
            }
            [record] if record.kind == K::RecordType => {
                let record = self.node(record);
                let doc = docvec![a, head, " ", record];
                // after the `}` of a broken record, `deriving` ends its line
                if !self.fits(doc)
                    && let Some(Element::Node(d)) = deriving.and_then(|at| n.children.get(at))
                {
                    let d = self.node(d);
                    return docvec![a, doc, " ", d];
                }
                doc
            }
            [base, refinement] if refinement.kind == K::Refinement => {
                let base = self.node(base);
                let refinement = self.node(refinement);
                let refinement = docvec![a, a.break_("", " "), refinement].nest(a, INDENT);
                docvec![a, head, " ", base, refinement].group(a)
            }
            _ => {
                let value = self.elements(n, value);
                let value = docvec![a, a.break_("", " "), value].nest(a, INDENT);
                docvec![a, head, value].group(a)
            }
        };
        let deriving = self.deriving(n, deriving);
        docvec![a, doc, deriving]
    }

    /// `deriving …` on its own line after a type declaration.
    fn deriving(&mut self, n: &'a Node, at: Option<usize>) -> Doc<'a> {
        let a = self.a;
        match at.and_then(|at| n.children.get(at)) {
            Some(Element::Node(d)) => {
                let d = self.node(d);
                docvec![a, a.line(), d].nest(a, INDENT).force_break(a)
            }
            _ => a.empty(),
        }
    }

    fn variants(&mut self, head: Doc<'a>, value: &'a [Element]) -> Doc<'a> {
        let a = self.a;
        // each variant starts at its `|`, or at itself
        let mut starts = Vec::new();
        let mut pipe = None;
        let mut variants = Vec::new();
        for child in value {
            match child {
                Element::Token(t) if t.kind == K::Pipe => pipe = Some(t.start),
                Element::Node(v) if v.kind == K::Variant => {
                    starts.push(pipe.take().unwrap_or(v.range().0));
                    variants.push(v);
                }
                _ => {}
            }
        }
        let mut docs = Vec::with_capacity(variants.len());
        let mut commented = false;
        for (i, v) in variants.iter().enumerate() {
            let comments = self.comments.leading(starts[i]);
            commented |= !comments.is_empty();
            let doc = self.node(v);
            let next = starts.get(i + 1).copied().unwrap_or(v.range().1);
            let trailing = self.comments.trailing(next, None);
            commented |= trailing.is_some();
            let leading = if comments.is_empty() {
                a.empty()
            } else {
                self.comment_lines(&comments)
            };
            let doc = match trailing {
                Some(c) => docvec![a, doc, " ", c],
                None => doc,
            };
            docs.push((leading, doc));
        }
        let flat = a.join(docs.iter().map(|d| d.1), a.text(" | "));
        let flat = docvec![a, head, " ", flat];
        // the constructors of a GADT read as a table of what each builds
        let indexed = variants
            .iter()
            .any(|v| v.nodes().any(|n| n.kind == K::VariantResult));
        if !commented && !indexed && self.fits(flat) {
            return flat;
        }
        let lines = docs
            .into_iter()
            .map(|(leading, doc)| docvec![a, a.line(), leading, "| ", doc]);
        docvec![a, head, a.concat(lines).nest(a, INDENT)].force_break(a)
    }

    /// Whether `doc` fits on one line at the column of the declarations.
    fn fits(&self, doc: Doc<'a>) -> bool {
        let line = doc.to_pretty_string(usize::MAX);
        !line.contains('\n') && self.column + line.chars().count() <= WIDTH
    }

    /// Declarations with members, including anonymous static parameterized modules: the
    /// head, then the members one per line, one level in.
    pub fn class(&mut self, n: &'a Node) -> Doc<'a> {
        let a = self.a;
        let separator = if n.kind == K::ModuleParamExpr {
            K::Arrow
        } else {
            K::Eq
        };
        let (Some(eq), Some(members)) = (position(n, separator), n.nodes().last()) else {
            return self.generic(n);
        };
        if members.kind != K::Members {
            return self.generic(n);
        }
        let head = self.elements(n, &n.children[..=eq]);
        let members = nodes(members);
        let first = members.first().map_or(u32::MAX, |m| m.range().0);
        let after = self.comments.trailing(first, None);
        let after = after.map_or(a.empty(), |c| docvec![a, " ", c]);
        let doc = self.members(&members, n.range().1);
        docvec![a, head, after, docvec![a, a.line(), doc].nest(a, INDENT)].force_break(a)
    }

    /// Declarations one level in, one per line.
    pub fn members(&mut self, members: &[&'a Node], end: u32) -> Doc<'a> {
        self.column += INDENT;
        let items = self.items(members, end, Some(';'), &mut |p, m| p.node(m));
        self.column -= INDENT;
        self.join_lines(&items)
    }

    /// `foreign mode name from …`, then its metadata one per line.
    pub fn foreign(&mut self, n: &'a Node) -> Doc<'a> {
        let a = self.a;
        let Some(at) = node_position(n, K::ForeignMeta) else {
            return self.generic(n);
        };
        let head = self.elements(n, &n.children[..at]);
        let Element::Node(meta) = &n.children[at] else {
            unreachable!("a node position")
        };
        let items = self.items(&nodes(meta), meta.range().1, None, &mut |p, m| p.node(m));
        let lines = docvec![a, a.line(), self.join_lines(&items)].nest(a, INDENT);
        docvec![a, head, lines].force_break(a)
    }
}
