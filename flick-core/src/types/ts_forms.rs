//! Backend-declared type-form attributes (such as `@ts` on `flick/js` or
//! `@go` on `flick/go`) and unknown-attribute validation (`FLICK-S014`).
//!
//! For `@ts` on backends that declare it, the TypeScript form of each type or
//! service is kept in the module's interface so every module naming the type
//! writes the same form.

use std::mem;

use super::Checker;
use crate::diagnostics::codes;
use crate::syntax::cst::{Node, Token};
use crate::syntax::heads::{CHECKED_BY, INVARIANT_OF};
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::ts_mark::{self, Mark};

const CORE_ATTRS_BEFORE_BACKEND: &[&str] =
    &["lang", "derive", "maker", "cell", "label", "owner", "sync"];
const CORE_ATTRS_AFTER_BACKEND: &[&str] = &["stores", "implicit", "when", "export"];

fn is_core_attribute(name: &str) -> bool {
    CORE_ATTRS_BEFORE_BACKEND.contains(&name)
        || CORE_ATTRS_AFTER_BACKEND.contains(&name)
        || ((CHECKED_BY.ends_with(name) || INVARIANT_OF.ends_with(name)) && name.contains('$'))
}

impl Checker<'_> {
    /// Validates every non-core attribute in `tree` against the active
    /// backend's declared `type_form_attributes`, plus `@ts` form consistency
    /// when the backend declares `ts`.
    pub fn check_backend_attributes(&mut self, tree: &Node, decls: &[&Node]) {
        self.walk_attributes(tree);
        if self
            .options
            .foreign_backend
            .type_form_attribute("ts")
            .is_some()
        {
            self.check_ts_forms(decls);
        }
    }

    fn walk_attributes(&mut self, node: &Node) {
        let children: Vec<&Node> = node.nodes().collect();
        for (index, &child) in children.iter().enumerate() {
            if child.kind == K::Attribute {
                let next = children
                    .iter()
                    .skip(index + 1)
                    .copied()
                    .find(|n| n.kind != K::Attribute);
                self.check_attribute(child, next);
            }
            self.walk_attributes(child);
        }
    }

    fn check_attribute(&mut self, node: &Node, next: Option<&Node>) {
        let words: Vec<Token> = node.child_tokens().filter(|t| t.kind == K::Ident).collect();
        let Some(name_tok) = words.first() else {
            return;
        };
        let name = name_tok.text(self.text);
        if is_core_attribute(name) {
            return;
        }
        let next_kind = next.map(|n| n.kind);
        if self
            .options
            .foreign_backend
            .type_form_attribute(name)
            .is_some()
        {
            if name == "ts" {
                match ts_mark::read(node, self.text) {
                    Some(Err(why)) => {
                        self.error(codes::S014, self.span(node), why);
                    }
                    _ if !matches!(next_kind, Some(K::TypeDecl | K::ServiceDecl)) => {
                        self.error(
                            codes::S014,
                            self.span(node),
                            "`@ts` belongs before the declaration of a type or a service"
                                .to_owned(),
                        );
                    }
                    _ => {}
                }
            } else {
                let tokens: Vec<(K, &str)> = node
                    .child_tokens()
                    .filter(|t| matches!(t.kind, K::Ident | K::String))
                    .map(|t| (t.kind, t.text(self.text)))
                    .skip(1)
                    .collect();
                let syntax_ok = matches!(
                    tokens.as_slice(),
                    [(K::Ident, "none") | (K::String, _)]
                        | [
                            (K::Ident, "import" | "from"),
                            (K::String, _),
                            (K::String, _)
                        ]
                );
                if !syntax_ok {
                    self.error(
                        codes::S014,
                        self.span(node),
                        format!(
                            "`@{name}` takes a form, `\"…\"`, or `none`, or `import \"module\" \"Export\"`"
                        ),
                    );
                } else if !matches!(next_kind, Some(K::TypeDecl | K::ServiceDecl)) {
                    self.error(
                        codes::S014,
                        self.span(node),
                        format!("`@{name}` belongs before the declaration of a type or a service"),
                    );
                }
            }
            return;
        }
        if !self.options.validate_foreign_metadata {
            return;
        }
        let mut known: Vec<String> = CORE_ATTRS_BEFORE_BACKEND
            .iter()
            .map(|a| format!("`@{a}`"))
            .collect();
        for attr in &self.options.foreign_backend.type_form_attributes {
            known.push(format!("`@{}`", attr.name));
        }
        known.extend(CORE_ATTRS_AFTER_BACKEND.iter().map(|a| format!("`@{a}`")));
        let listed = match known.split_last() {
            Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
            Some((only, _)) => only.clone(),
            None => String::new(),
        };
        self.error(
            codes::S014,
            self.span(node),
            format!("unknown attribute `@{name}`: the attributes are {listed}"),
        );
    }

    /// Reports the `@ts` forms of this module that cannot be written.
    pub fn check_ts_forms(&mut self, decls: &[&Node]) {
        for (at, _, form) in forms(decls, self.text) {
            if let Err(why) = form {
                self.error(codes::S014, self.span(at), why);
            }
        }
    }

    /// The types and services of this module with a `@ts` form, each with what
    /// the interface keeps of it.
    pub(super) fn ts_forms(&self) -> Vec<(String, Vec<String>)> {
        if self
            .options
            .foreign_backend
            .type_form_attribute("ts")
            .is_none()
        {
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
            // one that does not read, or is misplaced, is reported already
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
