//! A `let` with a contract: one entry stays on the `let`'s line when the
//! whole declaration fits, as a proof's goal does; otherwise `where` and `=`
//! each start a line at the declaration's column, the entries and the body
//! indented under them.

use flick_pretty::docvec;

use super::block::Value;
use super::printer::{Doc, INDENT, Printer, node_position, nodes};
use crate::syntax::cst::{Element, Node};
use crate::syntax::kind::SyntaxKind as K;

impl<'a> Printer<'a> {
    pub fn contract_let(&mut self, n: &'a Node, at: usize) -> Doc<'a> {
        let a = self.a;
        let (Some(c), Some(block)) = (node_position(n, K::Contract), node_position(n, K::Block))
        else {
            return self.generic(n);
        };
        let (Element::Node(contract), Element::Node(body)) = (&n.children[c], &n.children[block])
        else {
            unreachable!("node positions")
        };
        let head = self.elements(n, &n.children[..at]);
        let overrides: Vec<&'a Node> = n.nodes().filter(|c| c.kind == K::NativeOverride).collect();
        let body_end = overrides
            .first()
            .map_or_else(|| body.range().1, |o| o.range().0);
        let entries = nodes(contract);
        let doc = if let [entry] = entries.as_slice()
            && !self.comments.any_before(entry.range().1)
        {
            let entry = self.node(entry);
            let goal = docvec![a, "where ", entry, " ="];
            let goal = self.body(goal, Value::Block(body), body_end, &[]);
            docvec![a, head, " ", goal].group(a)
        } else {
            // the entries as statements are, with the comments between them
            let items = self.items(&entries, contract.range().1, None, &mut |p, e| p.node(e));
            let entries = docvec![a, a.line(), self.join_lines(&items)].nest(a, INDENT);
            let statements = nodes(body);
            let items = self.items(&statements, body_end, Some(';'), &mut |p, s| p.statement(s));
            let body = docvec![a, a.line(), self.join_lines(&items)].nest(a, INDENT);
            docvec![a, head, a.line(), "where", entries, a.line(), "=", body].force_break(a)
        };
        if overrides.is_empty() {
            doc
        } else {
            let items = self.items(&overrides, n.range().1, None, &mut |p, m| p.node(m));
            let lines = docvec![a, a.line(), self.join_lines(&items)].nest(a, INDENT);
            docvec![a, doc, lines].force_break(a)
        }
    }
}
