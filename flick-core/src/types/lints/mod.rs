//! Warnings about the shape of code: the size and branching of a function
//! ([`structure`]), conditions that say more than they need
//! ([`conditions`]), list and option code a library function says
//! directly ([`collections`]), expressions with a shorter form
//! ([`hygiene`]) and names and arms that hide something ([`scope`]). They
//! run once the module is solved, beside `sugar_lints`.

mod structure;

use super::Checker;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Checker<'_> {
    /// The lints of `node` itself; the caller walks its children.
    pub(super) fn shape_lints(&mut self, node: &Node) {
        if node.kind == K::LetDecl {
            self.structure(node);
        }
    }
}
