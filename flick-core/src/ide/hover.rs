//! Hover: the source type of what a position names, as Flick writes it,
//! with its rows, captures and constraints, never a TypeScript helper.

use std::fmt::Write;

use super::{Ide, Symbol, tree};
use crate::doc;
use crate::iface::Namespace;
use crate::source::FileId;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{Form, show_type};

impl Ide<'_> {
    /// The signature of a top-level value as its source writes it.
    fn signature(&self, module: &str, name: &str) -> Option<String> {
        let source = self.source(module)?;
        let sig = source.tree.nodes().find(|d| {
            d.kind == K::Signature
                && d.child_tokens()
                    .next()
                    .is_some_and(|t| t.text(&source.text) == name)
        })?;
        let ty = sig.nodes().next()?;
        Some(
            ty.text(&source.text)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
        )
    }

    /// A `task`, `scope` or `ignore` keyword at `offset`: the type of the
    /// whole block it opens, or of what it discards, and what it does.
    fn keyword(&self, file: FileId, offset: u32) -> Option<(String, String, (u32, u32))> {
        let m = self.module(file)?;
        let parent = *tree::path_to(&m.tree, offset).last()?;
        let kw = parent.child_tokens().find(|t| {
            matches!(t.kind, K::TaskKw | K::ScopeKw | K::IgnoreKw)
                && t.start <= offset
                && offset <= t.end
        })?;
        let ty = m.typing.types.get(&(kw.start, kw.end))?;
        let shown = show_type(ty);
        let (code, note) = match kw.kind {
            K::TaskKw => (
                format!("task {{ … }} : {shown}"),
                "Builds this computation without running it; `await` runs it.".to_owned(),
            ),
            K::ScopeKw => (
                format!("scope {{ … }} : {shown}"),
                "Runs its body in a fresh region and releases what the body \
                 acquired when it ends; the value may not mention the region."
                    .to_owned(),
            ),
            _ => {
                let mut note = format!("Discards a value of type `{shown}`.");
                if ty.spine().0.is_prim("Eff") {
                    note.push_str(
                        " The computation is not run: `ignore (await …)` runs it \
                         and discards its result.",
                    );
                }
                (format!("ignore : {shown} -> Unit"), note)
            }
        };
        Some((code, note, (kw.start, kw.end)))
    }

    /// Markdown for the symbol at `offset`, and the range it covers.
    pub fn hover(&self, file: FileId, offset: u32) -> Option<(String, (u32, u32))> {
        if let Some((code, note, range)) = self.keyword(file, offset) {
            return Some((format!("```flick\n{code}\n```\n\n{note}"), range));
        }
        let (symbol, range) = self.symbol_at(file, offset)?;
        let (code, note) = match &symbol {
            Symbol::Local { name, binder, .. } => {
                let m = self.module(file)?;
                let ty = tree::innermost(m.typing.types.iter(), offset)
                    .or_else(|| tree::innermost(m.typing.types.iter(), binder.0))?
                    .1;
                (format!("{name} : {}", show_type(ty)), None)
            }
            Symbol::Entity(e) => {
                let interface = self.interface(&e.module)?;
                let export = interface.find(e.namespace, &e.name)?;
                let code = match (e.namespace, &export.definition) {
                    (Namespace::Type, Some(def)) => match &def.form {
                        Form::Alias { params, body, .. } => {
                            let params = params.iter().fold(String::new(), |mut acc, p| {
                                let _ = write!(acc, " {p}");
                                acc
                            });
                            format!("type alias {}{params} = {}", e.name, show_type(body))
                        }
                        _ => format!("type {} : {}", e.name, def.kind),
                    },
                    _ => match (self.signature(&e.module, &e.name), &export.scheme) {
                        (Some(written), _) => format!("{} : {written}", e.name),
                        (None, Some(scheme)) => format!("{} : {}", e.name, show_type(scheme)),
                        (None, None) => e.name.clone(),
                    },
                };
                let doc = self
                    .source(&e.module)
                    .and_then(|source| doc::comments(&source.text).remove(&e.name));
                let from = format!("from `{}`", e.module);
                (
                    code,
                    Some(doc.map_or(from.clone(), |d| format!("{d}\n\n{from}"))),
                )
            }
            Symbol::Operation {
                module,
                service,
                name,
            } => {
                let source = self.source(module)?;
                let decl = source.tree.nodes().find(|d| {
                    d.kind == K::ServiceDecl
                        && tree::name_of(d, K::Con, &source.text) == Some(service.as_str())
                })?;
                let sig = decl
                    .nodes()
                    .filter(|n| n.kind == K::Members)
                    .flat_map(Node::nodes)
                    .find(|s| tree::name_of(s, K::Ident, &source.text) == Some(name.as_str()))?;
                let ty = sig.nodes().next()?.text(&source.text);
                let ty = ty.split_whitespace().collect::<Vec<_>>().join(" ");
                (
                    format!("{service}.{name} : {ty}"),
                    Some(format!("an operation of `{service}`")),
                )
            }
            Symbol::InstanceMethod {
                module,
                instance,
                method,
            } => (
                method.clone(),
                Some(format!("from `instance {instance}` in `{module}`")),
            ),
        };
        let mut text = format!("```flick\n{code}\n```");
        if let Some(note) = note {
            let _ = write!(text, "\n\n{note}");
        }
        Some((text, range))
    }
}
