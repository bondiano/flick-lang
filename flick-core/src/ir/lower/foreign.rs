//! Lowering of `foreign` bindings, `foreign module inline` preambles, and
//! selected `@native` overrides on top-level `let` declarations.

use flick_ir::{Binding, Foreign, InlineCode, Item, ItemKind};

use super::Lower;
use crate::effects::key;
use crate::source::Span;
use crate::syntax::cst::{Node, foreign_words};
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::literal;
use crate::types::{ForeignInfo, NativeOverrideInfo};

impl Lower<'_> {
    pub(super) fn foreign_preamble(&self, decl: &Node) -> Option<InlineCode> {
        let tok = decl.child_tokens().find(|t| t.kind == K::String)?;
        Some(InlineCode {
            code: literal::string_value(tok.text(self.text)),
            span: Span::new(self.file, tok.start, tok.end),
        })
    }

    pub(super) fn foreign(&mut self, decl: &Node) -> Option<Foreign> {
        let name = foreign_words(decl).1?.text(self.text).to_owned();
        let info = self
            .typing
            .foreigns
            .get(&decl.range())
            .cloned()
            .unwrap_or_default();
        self.build_foreign(decl, name, info, false)
    }

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
        if self.no_native {
            return false;
        }
        let Some((_, selected)) = self
            .typing
            .native_overrides
            .iter()
            .find(|(range, ov)| *range == decl.range() && ov.enabled)
            .cloned()
        else {
            return false;
        };
        self.native_items(decl, name, span, selected, items)
    }

    /// The items of the `@native` clause `selected` of the top-level `let`
    /// `decl`; `false` when it has none.
    pub(super) fn native_items(
        &mut self,
        decl: &Node,
        name: &str,
        span: Span,
        selected: NativeOverrideInfo,
        items: &mut Vec<Item>,
    ) -> bool {
        let Some(ov) = decl
            .nodes()
            .find(|n| n.kind == K::NativeOverride && n.range() == selected.override_range)
        else {
            return false;
        };
        let scoped = selected.info.scoped_stages.clone();
        let owner_result = selected.info.owner_result.clone();
        let Some(foreign) = self.build_foreign(ov, name.to_owned(), selected.info, true) else {
            return false;
        };
        if scoped.iter().any(|&s| s) || owner_result.is_some() {
            self.fresh_definition();
            self.scoped_foreign(foreign, &scoped, owner_result.as_deref(), span, items);
        } else {
            items.push(Item {
                kind: ItemKind::Foreign(foreign),
                span,
            });
        }
        true
    }

    fn build_foreign(
        &mut self,
        node: &Node,
        name: String,
        info: ForeignInfo,
        bare_global: bool,
    ) -> Option<Foreign> {
        let source = node.nodes().find(|n| n.kind == K::ForeignSource)?;
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
            (Some("global"), [tok]) => Binding::Global {
                name: literal::string_value(tok.text(self.text)),
            },
            (None, [tok]) if bare_global => Binding::Global {
                name: literal::string_value(tok.text(self.text)),
            },
            (None, [module, export]) => Binding::Module {
                from: key(&literal::string_value(module.text(self.text))).into_owned(),
                name: literal::string_value(export.text(self.text)),
            },
            _ => return None,
        };
        let catch = info.catch.and_then(|at| {
            let expr_node = node
                .nodes()
                .filter(|n| n.kind == K::ForeignMeta)
                .flat_map(Node::nodes)
                .flat_map(Node::nodes)
                .find(|n| n.range() == at)?;
            Some(self.expr(expr_node))
        });
        let (type_params, _, ty) = self.ir_top_scheme(&name);
        Some(Foreign {
            source_name: None,
            name,
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
