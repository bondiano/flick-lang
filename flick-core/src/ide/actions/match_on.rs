//! "Pattern match on `x`": a local of a variant type used as a result
//! becomes a `match` with an arm for each constructor.

use super::super::{Ide, tree};
use super::{Action, edit, indent_at};
use crate::iface::Namespace;
use crate::resolve::module_syntax;
use crate::source::FileId;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{Ref, Type};

impl Ide<'_> {
    pub(super) fn match_on(&self, file: FileId, offset: u32) -> Option<Action> {
        let m = self.module(file)?;
        let text = self.text(file);
        let path = tree::path_to(&m.tree, offset);
        let (node, block) = match path[..] {
            [.., block, node] if node.kind == K::NameExpr && block.kind == K::Block => {
                (node, block)
            }
            _ => return None,
        };
        // only the result of a block: elsewhere a `match` needs parentheses
        if block.nodes().last().map(Node::range) != Some(node.range()) {
            return None;
        }
        if !matches!(m.typing.names.get(&node.range()), Some(Ref::Local)) {
            return None;
        }
        let ty = m.typing.types.get(&node.range())?;
        let Type::Con { module, name } = ty.spine().0 else {
            return None;
        };
        let interface = self.interface(module)?;
        let qualifier = if *module == m.name {
            String::new()
        } else {
            let syntax = module_syntax(&m.tree, text, file);
            let import = syntax.imports.iter().find(|i| i.module == *module);
            format!("{}.", import.map_or(module.as_str(), |i| i.qualifier()))
        };
        let indent = format!("{}  ", indent_at(text, node.range().0));
        let arms: Vec<String> = interface
            .members(name)
            .filter(|e| e.entity.namespace == Namespace::Constructor)
            .map(|e| {
                let fields = e.scheme.as_ref().map_or(0, arity);
                let wild = " _".repeat(fields);
                format!("\n{indent}| {qualifier}{}{wild} -> todo", e.entity.name)
            })
            .collect();
        if arms.is_empty() {
            return None;
        }
        let variable = node.text(text);
        let (start, end) = node.range();
        Some(Action {
            title: format!("pattern match on `{variable}`"),
            kind: "refactor.rewrite",
            edits: vec![edit(
                file,
                start,
                end,
                format!("match {variable} with{}", arms.concat()),
            )],
        })
    }
}

/// The number of fields of a constructor, by the arrows of its type.
fn arity(t: &Type) -> usize {
    match t {
        Type::Forall { body, .. } => arity(body),
        Type::Fun { result, .. } => 1 + arity(result),
        _ => 0,
    }
}
