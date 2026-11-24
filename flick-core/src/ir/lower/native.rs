//! Lowering of selected `@native` overrides on top-level `let` declarations.

use flick_ir::{Binding, Foreign, InlineCode, Item, ItemKind};

use super::Lower;
use crate::effects::key;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::literal;
use crate::types::NativeOverrideInfo;

impl Lower<'_> {
    /// Lowers a top-level `let` whose selected `@native` override is enabled
    /// into `ItemKind::Foreign` (or a scoped foreign wrapper when required by
    /// region ownership). Returns `true` when the override was emitted.
    pub(super) fn try_native_override(
        &mut self,
        decl: &Node,
        name: &str,
        span: Span,
        items: &mut Vec<Item>,
    ) -> bool {
        let Some(selected) = self
            .typing
            .native_overrides
            .get(&decl.range())
            .filter(|n| n.enabled)
            .cloned()
        else {
            return false;
        };
        let Some(ov) = decl
            .nodes()
            .find(|n| n.kind == K::NativeOverride && n.range() == selected.override_range)
        else {
            return false;
        };
        let Some(foreign) = self.build_native_override(ov, name, &selected) else {
            return false;
        };
        let scoped = selected.info.scoped_stages.clone();
        if scoped.iter().any(|&s| s) || selected.info.owner_result.is_some() {
            self.fresh_definition();
            let result = selected.info.owner_result.as_deref();
            self.scoped_foreign(foreign, &scoped, result, span, items);
        } else {
            items.push(Item {
                kind: ItemKind::Foreign(foreign),
                span,
            });
        }
        true
    }

    fn build_native_override(
        &mut self,
        ov: &Node,
        name: &str,
        selected: &NativeOverrideInfo,
    ) -> Option<Foreign> {
        let source = ov.nodes().find(|n| n.kind == K::ForeignSource)?;
        let kind_word = source
            .child_tokens()
            .find(|t| t.kind == K::Ident)
            .map(|t| t.text(self.text));
        let strings: Vec<_> = source
            .child_tokens()
            .filter(|t| t.kind == K::String)
            .collect();
        let binding = match (kind_word, &strings[..]) {
            (Some("inline"), [tok]) => Binding::Inline(InlineCode {
                code: literal::string_value(tok.text(self.text)),
                span: Span::new(self.file, tok.start, tok.end),
            }),
            (Some("global") | None, [tok]) => Binding::Global {
                name: literal::string_value(tok.text(self.text)),
            },
            (None, [module, export]) => Binding::Module {
                from: key(&literal::string_value(module.text(self.text))).into_owned(),
                name: literal::string_value(export.text(self.text)),
            },
            _ => return None,
        };
        let info = selected.info.clone();
        let catch = info.catch.and_then(|at| {
            let node = ov
                .nodes()
                .filter(|n| n.kind == K::ForeignMeta)
                .flat_map(Node::nodes)
                .flat_map(Node::nodes)
                .find(|n| n.range() == at)?;
            Some(self.expr(node))
        });
        let (type_params, _, ty) = self.ir_top_scheme(name);
        Some(Foreign {
            source_name: None,
            name: name.to_owned(),
            mode: info.mode,
            binding,
            stages: info.stages.clone(),
            dicts: info.dicts,
            arity: info.js_arity(),
            adapter: info.adapter,
            catch,
            type_params,
            ty,
        })
    }
}
