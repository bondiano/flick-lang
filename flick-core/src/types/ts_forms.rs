//! Types marked `@ts`: the TypeScript form each has in generated code, kept
//! in the interface of the module so that every module naming the type
//! writes the same. The checker reads the marks and refuses a form that
//! cannot be written; whether the form is true of the values is the word of
//! the package that declares the type, as the signature of a `foreign` is.

use std::mem;

use super::Checker;
use crate::diagnostics::codes;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::ts_mark::{self, Mark};

impl Checker<'_> {
    /// Reports the forms of this module that cannot be written.
    pub fn check_ts_forms(&mut self, decls: &[&Node]) {
        for (at, _, form) in forms(decls, self.text) {
            if let Err(why) = form {
                self.error(codes::S014, self.span(at), why);
            }
        }
    }

    /// The types and services of this module with a form, each with what
    /// the interface keeps of it.
    pub(super) fn ts_forms(&self) -> Vec<(String, Vec<String>)> {
        let decls: Vec<&Node> = self.root.into_iter().flat_map(Node::nodes).collect();
        let mut kept = Vec::new();
        for (_, name, form) in forms(&decls, self.text) {
            if let Ok(form) = form {
                kept.push((name, form));
            }
        }
        kept
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
