//! Backend-declared type-form attributes (such as `@ts` on `flick/js` or
//! `@go` on `flick/go`) and unknown-attribute validation (`FLICK-S014`).
//!
//! For `@ts` on backends that declare it, the TypeScript form of each type or
//! service is kept in the module's interface so every module naming the type
//! writes the same form.

use std::mem;

use flick_plugin::AttributeTarget;

use super::Checker;
use crate::diagnostics::codes;
use crate::syntax::attr::attr_parts;
use crate::syntax::cst::Node;
use crate::syntax::heads::{CHECKED_BY, INVARIANT_OF};
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::ts_mark::{self, Mark};

const CORE_ATTRS_BEFORE_BACKEND: &[&str] =
    &["lang", "derive", "maker", "cell", "label", "owner", "sync"];
const CORE_ATTRS_AFTER_BACKEND: &[&str] = &["stores", "implicit", "when", "export"];

pub(crate) fn is_reserved_core_attribute(name: &str) -> bool {
    CORE_ATTRS_BEFORE_BACKEND.contains(&name)
        || CORE_ATTRS_AFTER_BACKEND.contains(&name)
        || name == "native"
}

fn is_core_attribute(name: &str) -> bool {
    CORE_ATTRS_BEFORE_BACKEND.contains(&name)
        || CORE_ATTRS_AFTER_BACKEND.contains(&name)
        || ((CHECKED_BY.ends_with(name) || INVARIANT_OF.ends_with(name)) && name.contains('$'))
}

fn attribute_target(parent_kind: K, next_kind: Option<K>) -> Option<AttributeTarget> {
    match parent_kind {
        K::RecordTypeField => Some(AttributeTarget::RecordField),
        K::Variant => Some(AttributeTarget::Variant),
        K::SourceFile | K::Members
            if matches!(
                next_kind,
                Some(
                    K::TypeDecl
                        | K::ErrorDecl
                        | K::LetDecl
                        | K::Signature
                        | K::ClassDecl
                        | K::InstanceDecl
                        | K::ServiceDecl
                        | K::ForeignDecl
                        | K::KindDecl
                        | K::ModuleTypeDecl
                        | K::InlineModuleDecl
                        | K::PluginDecl
                )
            ) =>
        {
            Some(AttributeTarget::Declaration)
        }
        _ => None,
    }
}

pub(crate) fn describe_targets(targets: &[AttributeTarget]) -> String {
    let labels: Vec<&str> = targets
        .iter()
        .map(|t| match t {
            AttributeTarget::Declaration => "a declaration",
            AttributeTarget::Variant => "a variant",
            AttributeTarget::RecordField => "a record field",
        })
        .collect();
    match labels.as_slice() {
        [] => "nothing".to_owned(),
        [only] => (*only).to_owned(),
        [a, b] => format!("{a} or {b}"),
        [rest @ .., last] => format!("{}, or {last}", rest.join(", ")),
    }
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
                self.check_attribute(node.kind, child, next);
            }
            self.walk_attributes(child);
        }
    }

    fn check_attribute(&mut self, parent_kind: K, node: &Node, next: Option<&Node>) {
        let Some(parts) = attr_parts(node, self.text) else {
            return;
        };
        let name = parts.name;
        if !parts.namespaced && is_core_attribute(name) {
            return;
        }
        let next_kind = next.map(|n| n.kind);
        if !parts.namespaced
            && self
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
                    .filter(|t| t.start >= parts.name_end)
                    .map(|t| (t.kind, t.text(self.text)))
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
        if let Some(declared) = self
            .options
            .plugin_attributes
            .iter()
            .find(|a| a.spec.name == name)
        {
            let site = attribute_target(parent_kind, next_kind);
            if !site.is_some_and(|t| declared.spec.targets.contains(&t)) {
                let allowed = describe_targets(&declared.spec.targets);
                self.error(
                    codes::S014,
                    self.span(node),
                    format!("`@{name}` belongs on {allowed}"),
                );
            }
            return;
        }
        if !self.options.validate_foreign_metadata {
            return;
        }
        self.report_unknown_attribute(node, name);
    }

    fn report_unknown_attribute(&mut self, node: &Node, name: &str) {
        let mut known: Vec<String> = CORE_ATTRS_BEFORE_BACKEND
            .iter()
            .map(|a| format!("`@{a}`"))
            .collect();
        for attr in &self.options.foreign_backend.type_form_attributes {
            known.push(format!("`@{}`", attr.name));
        }
        known.extend(CORE_ATTRS_AFTER_BACKEND.iter().map(|a| format!("`@{a}`")));
        for attr in self.options.plugin_attributes {
            known.push(format!("`@{}`", attr.spec.name));
        }
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
