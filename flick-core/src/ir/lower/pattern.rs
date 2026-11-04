//! Patterns. The alternatives of an or-pattern bind the same names, so they
//! share the unique names the first alternative binds.

use super::Lower;
use super::literals::number;
use crate::ir::{Lit, Pat};
use crate::numeric::Num;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::literal;
use crate::types::Ref;

impl Lower<'_> {
    pub(super) fn pattern(&mut self, node: &Node) -> Pat {
        let children: Vec<&Node> = node.nodes().collect();
        match node.kind {
            K::VarPat => {
                let name = self.token_text(node, K::Ident).unwrap_or_default();
                Pat::Bind(self.binder(&name, node.range().0))
            }
            K::ParenPat | K::AnnotPat => self.pattern(children[0]),
            K::LitPat => {
                let tokens: Vec<_> = node.child_tokens().collect();
                let Some(last) = tokens.last() else {
                    return Pat::Wild;
                };
                let negative = tokens.len() > 1;
                let text = last.text(self.text);
                Pat::Lit(match last.kind {
                    K::TrueKw => Lit::Bool(true),
                    K::FalseKw => Lit::Bool(false),
                    K::String => Lit::String(literal::string_value(text)),
                    K::BigInt => number(Some(Num::BigInt), text, negative),
                    _ => number(
                        self.typing.numeric.get(&node.range()).copied(),
                        text,
                        negative,
                    ),
                })
            }
            K::UnitPat => Pat::Lit(Lit::Unit),
            K::TuplePat => Pat::Tuple(children.iter().map(|c| self.pattern(c)).collect()),
            K::ListPat => {
                let (rest, items): (Vec<&Node>, Vec<&Node>) =
                    children.iter().partition(|c| c.kind == K::RestPat);
                Pat::List(
                    items.iter().map(|c| self.pattern(c)).collect(),
                    rest.first()
                        .and_then(|r| r.nodes().next())
                        .map(|r| Box::new(self.pattern(r))),
                )
            }
            K::RecordPat => Pat::Record(
                node.nodes()
                    .filter(|n| n.kind == K::RecordPatField)
                    .filter_map(|field| {
                        let label = self.token_text(field, K::Ident)?;
                        let pat = match field.nodes().next() {
                            Some(p) => self.pattern(p),
                            None => Pat::Bind(self.binder(&label, field.range().0)),
                        };
                        Some((label, pat))
                    })
                    .collect(),
            ),
            K::OrPat => {
                let mark = self.scope.len();
                let mut alternatives = Vec::new();
                let mut iter = children.iter();
                if let Some(first) = iter.next() {
                    alternatives.push(self.pattern(first));
                }
                let bound = self.scope[mark..].to_vec();
                let saved = self.reuse.replace(bound);
                alternatives.extend(iter.map(|alt| self.pattern(alt)));
                self.reuse = saved;
                Pat::Or(alternatives)
            }
            K::ConPat => {
                let Some(Ref::Constructor(c)) = self.typing.names.get(&node.range()) else {
                    panic!("the checker resolved the constructor pattern");
                };
                let c = c.clone();
                Pat::Con(c, children.iter().map(|p| self.pattern(p)).collect())
            }
            _ => Pat::Wild,
        }
    }

    /// A name a pattern binds: fresh, or the first alternative's.
    fn binder(&mut self, name: &str, at: u32) -> String {
        if let Some(reuse) = &self.reuse
            && let Some((_, _, unique)) = reuse.iter().find(|(n, _, _)| n == name)
        {
            return unique.clone();
        }
        self.bind(name, at)
    }
}
