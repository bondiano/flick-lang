//! Function types: broken after each arrow, the effect annotation of an
//! arrow kept with it.

use flick_pretty::docvec;

use super::printer::{Doc, INDENT, Printer, position, token};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl<'a> Printer<'a> {
    /// `name : T`: on one line, or the type on the next lines one level in,
    /// `forall a.` and a context `C a =>` on lines of their own when the
    /// rest does not fit, and the parameters under each other.
    pub fn signature(&mut self, n: &'a Node) -> Doc<'a> {
        let a = self.a;
        let (Some(colon), Some(ty)) = (position(n, K::Colon), n.nodes().last()) else {
            return self.generic(n);
        };
        let head = self.elements(n, &n.children[..=colon]);
        let ty = self.type_at_start(ty);
        docvec![a, head, docvec![a, a.break_("", " "), ty].nest(a, INDENT)].group(a)
    }

    /// A type that starts its line.
    fn type_at_start(&mut self, n: &'a Node) -> Doc<'a> {
        let a = self.a;
        let separator = match n.kind {
            K::ForallType => K::Dot,
            K::ConstrainedType => K::FatArrow,
            K::ArrowType => return self.arrow_chain(n, true),
            _ => return self.node(n),
        };
        let (Some(at), Some(body)) = (position(n, separator), n.nodes().last()) else {
            return self.generic(n);
        };
        let head = self.elements(n, &n.children[..=at]);
        let body = self.type_at_start(body);
        docvec![a, head, a.break_("", " "), body].group(a)
    }

    /// `A -> B ->{r; e} C`: on one line, or a line per parameter after its
    /// arrow, one level in.
    pub fn arrow(&mut self, n: &'a Node) -> Doc<'a> {
        self.arrow_chain(n, false)
    }

    /// An arrow type; `aligned` puts the parameters under the first one,
    /// which starts its line.
    fn arrow_chain(&mut self, n: &'a Node, aligned: bool) -> Doc<'a> {
        let a = self.a;
        let mut steps = Vec::new();
        let mut current = n;
        let result = loop {
            let parts: Vec<&Node> = current.nodes().collect();
            let annot = parts.iter().find(|p| p.kind == K::EffectAnnot);
            let types: Vec<&&Node> = parts.iter().filter(|p| p.kind != K::EffectAnnot).collect();
            let (Some(arrow), [parameter, result]) = (token(current, K::Arrow), types.as_slice())
            else {
                return self.generic(n);
            };
            steps.push((**parameter, arrow, annot.copied()));
            if result.kind == K::ArrowType {
                current = result;
            } else {
                break **result;
            }
        };
        let mut docs = Vec::with_capacity(steps.len() + 1);
        for (i, (parameter, arrow, annot)) in steps.into_iter().enumerate() {
            let parameter = self.node(parameter);
            let arrow = self.tok(arrow);
            let annot = annot.map_or(a.empty(), |e| self.node(e));
            let step = docvec![a, parameter, " ", arrow, annot];
            docs.push(if i == 0 {
                step
            } else {
                docvec![a, a.break_("", " "), step]
            });
        }
        let result = self.node(result);
        docs.push(docvec![a, a.break_("", " "), result]);
        let first = docs.remove(0);
        let rest = a.concat(docs);
        let rest = if aligned { rest } else { rest.nest(a, INDENT) };
        docvec![a, first, rest].group(a)
    }
}
