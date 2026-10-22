//! Instances: the methods are checked against their own signatures, with
//! the variables of the instance head in scope.

use super::Checker;
use super::bindings::let_parts;
use super::convert::{Free, is_type};
use super::kinds::Kind;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Checker<'_> {
    /// The methods of an instance are checked against their own signatures,
    /// with the variables of the head in scope.
    pub(super) fn instance(&mut self, decl: &Node) {
        let mark = self.tyvars.len();
        for part in decl.nodes() {
            match part.kind {
                K::Context => {
                    for c in part.nodes() {
                        self.ty(c, &Kind::Constraint, Free::Quantify);
                    }
                }
                kind if is_type(kind) => {
                    self.ty(part, &Kind::Constraint, Free::Quantify);
                }
                _ => {}
            }
        }
        for i in mark..self.tyvars.len() {
            let name = self.tyvars[i].name.clone();
            self.tyvars[i].kind = self.kinds.zonk(&self.tyvars[i].kind, true);
            self.tyvars[i].ty = self.store.skolem(&name);
        }
        let members: Vec<&Node> = decl
            .nodes()
            .filter(|n| n.kind == K::Members)
            .flat_map(Node::nodes)
            .collect();
        for (j, member) in members.iter().enumerate() {
            match member.kind {
                K::LetDecl => {
                    let parts = let_parts(member, self.text);
                    let name = parts.name.clone().unwrap_or_default();
                    let sig = j
                        .checked_sub(1)
                        .map(|k| members[k])
                        .filter(|s| s.kind == K::Signature && self.pairs(s, member))
                        .and_then(|s| s.nodes().find(|n| is_type(n.kind)));
                    match sig {
                        Some(sig) => {
                            let (scheme, _) = self.signature(sig);
                            self.check_signed(member, &parts, &scheme, None);
                        }
                        None => {
                            self.unsigned(member, &parts, &name);
                        }
                    }
                    self.settle_operators();
                }
                K::Signature if !members.get(j + 1).is_some_and(|d| self.pairs(member, d)) => {
                    self.lone_signature(member, false);
                }
                _ => {}
            }
        }
        self.tyvars.truncate(mark);
    }
}
