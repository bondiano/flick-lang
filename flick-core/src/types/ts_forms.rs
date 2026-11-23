//! Backend-declared type-form attributes (`@ts` on JS, and any custom
//! type-form attribute declared by another backend): the compiler validates
//! `@ts` marks when the active backend declares `ts` and stores declared
//! type-form attributes in the module interface.

use std::collections::BTreeMap;
use std::mem;

use super::Checker;
use crate::diagnostics::codes;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::literal;
use crate::syntax::ts_mark::{self, Mark};

impl Checker<'_> {
    /// Reports `@ts` forms of this module that cannot be written (when the
    /// active backend declares `@ts`).
    pub fn check_ts_forms(&mut self, decls: &[&Node]) {
        if !self.foreign_meta().has_type_form_attr(ts_mark::TAG) {
            return;
        }
        for (at, _, form) in forms(decls, self.text) {
            if let Err(why) = form {
                self.error(codes::S014, self.span(at), why);
            }
        }
    }

    /// The types and services of this module with a `@ts` form, each with what
    /// the interface keeps of it.
    pub(super) fn ts_forms(&self) -> Vec<(String, Vec<String>)> {
        if !self.foreign_meta().has_type_form_attr(ts_mark::TAG) {
            return Vec::new();
        }
        let decls: Vec<&Node> = self.root.into_iter().flat_map(Node::nodes).collect();
        let mut kept = Vec::new();
        for (_, name, form) in forms(&decls, self.text) {
            if let Ok(form) = form {
                kept.push((name, form));
            }
        }
        kept
    }

    /// Non-`@ts` backend-declared type-form attributes attached to types or
    /// services of this module, grouped by attribute name for interface tags.
    pub(super) fn backend_type_form_tags(&self) -> BTreeMap<String, Vec<(String, Vec<String>)>> {
        let meta = self.foreign_meta();
        let has_other = meta
            .type_form_attributes
            .iter()
            .any(|attr| attr.name != ts_mark::TAG);
        if !has_other {
            return BTreeMap::new();
        }
        let decls: Vec<&Node> = self.root.into_iter().flat_map(Node::nodes).collect();
        let mut out: BTreeMap<String, Vec<(String, Vec<String>)>> = BTreeMap::new();
        let mut pending: Vec<(String, Vec<String>)> = Vec::new();
        for decl in decls {
            if decl.kind == K::Attribute {
                let mut words = decl
                    .child_tokens()
                    .filter(|t| matches!(t.kind, K::Ident | K::String));
                if let Some(name_tok) = words.next().filter(|t| t.kind == K::Ident) {
                    let name = name_tok.text(self.text);
                    if name != ts_mark::TAG && meta.has_type_form_attr(name) {
                        let args = words
                            .map(|t| {
                                let raw = t.text(self.text);
                                if t.kind == K::String {
                                    literal::string_value(raw)
                                } else {
                                    raw.to_owned()
                                }
                            })
                            .collect();
                        pending.push((name.to_owned(), args));
                    }
                }
                continue;
            }
            let attrs = mem::take(&mut pending);
            let name = (decl.child_tokens().find(|t| t.kind == K::Con))
                .filter(|_| matches!(decl.kind, K::TypeDecl | K::ServiceDecl));
            if let Some(name) = name {
                let decl_name = name.text(self.text).to_owned();
                for (tag, args) in attrs {
                    out.entry(tag).or_default().push((decl_name.clone(), args));
                }
            }
        }
        out
    }
}

/// A type or service `@ts` attributes stand before: the first attribute,
/// the name, and what the interface keeps of the form or why there is none.
type Marked<'a> = (&'a Node, String, Result<Vec<String>, String>);

/// The types and services among `decls` that `@ts` attributes stand before.
fn forms<'a>(decls: &[&'a Node], text: &str) -> Vec<Marked<'a>> {
    let mut out = Vec::new();
    let mut pending: Vec<Mark> = Vec::new();
    let mut first = None;
    for decl in decls {
        if decl.kind == K::Attribute {
            // one that does not read, or is misplaced, is reported as syntax
            if let Some(Ok(mark)) = ts_mark::read(decl, text) {
                pending.push(mark);
                first.get_or_insert(*decl);
            }
            continue;
        }
        let (marks, at) = (mem::take(&mut pending), first.take());
        let name = (decl.child_tokens().find(|t| t.kind == K::Con))
            .filter(|_| matches!(decl.kind, K::TypeDecl | K::ServiceDecl));
        if let (Some(at), Some(name)) = (at, name) {
            let arity = decl.nodes().filter(|n| n.kind == K::TypeParam).count();
            out.push((at, name.text(text).to_owned(), ts_mark::parts(marks, arity)));
        }
    }
    out
}
