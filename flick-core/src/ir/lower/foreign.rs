//! Lowering of `foreign` bindings and `foreign module inline` preambles.

use flick_ir::{Binding, Foreign, InlineCode};

use super::Lower;
use crate::effects::key;
use crate::source::Span;
use crate::syntax::cst::{Node, foreign_words};
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::literal;

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
        let source = decl.nodes().find(|n| n.kind == K::ForeignSource)?;
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
            (None, [module, export]) => Binding::Module {
                from: key(&literal::string_value(module.text(self.text))).into_owned(),
                name: literal::string_value(export.text(self.text)),
            },
            _ => return None,
        };
        let info = self
            .typing
            .foreigns
            .get(&decl.range())
            .cloned()
            .unwrap_or_default();
        let catch = info.catch.and_then(|at| {
            let node = decl
                .nodes()
                .filter(|n| n.kind == K::ForeignMeta)
                .flat_map(Node::nodes)
                .flat_map(Node::nodes)
                .find(|n| n.range() == at)?;
            Some(self.expr(node))
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
