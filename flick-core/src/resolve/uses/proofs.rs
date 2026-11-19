//! The names of a proof: its parameters scope over its goal and its proof,
//! and its rules are names no binder of it has.

use super::Walker;
use crate::syntax::cst::Node;
use crate::syntax::heads::PROOF_WORDS;
use crate::syntax::kind::SyntaxKind as K;

impl Walker<'_> {
    /// A proof: its parameters and premises scope over its goal and its
    /// proof, and a premise's goal over the parameters before it.
    pub(super) fn proof_decl(&mut self, node: &Node) {
        let mark = self.locals.len();
        self.proof.0 = true;
        for param in node
            .nodes()
            .filter(|n| n.kind == K::Params)
            .flat_map(Node::nodes)
        {
            if param.kind == K::Premise {
                for goal in param.nodes().filter(|n| n.kind != K::VarPat) {
                    self.walk(goal);
                }
                param
                    .nodes()
                    .filter(|n| n.kind == K::VarPat)
                    .for_each(|p| self.bind(p));
            } else {
                self.bind(param);
                // the types it names are uses
                self.walk(param);
            }
        }
        for child in node
            .nodes()
            .filter(|n| !matches!(n.kind, K::Params | K::VarPat))
        {
            self.walk(child);
        }
        self.release(mark);
        self.proof.0 = false;
        // a later proof may use this one as a lemma; a law proves nothing
        let name =
            (node.nodes().find(|n| n.kind == K::VarPat)).filter(|_| node.kind == K::ProofDecl);
        self.proof
            .1
            .extend(name.map(|n| n.text(self.text).trim().to_owned()));
    }

    /// Whether `name`, unqualified, is a rule of the proof being walked or
    /// a proof before it.
    pub(super) fn is_rule(&self, name: &str, generated: bool) -> bool {
        self.proof.0
            && (PROOF_WORDS.contains(&name) || self.proof.1.iter().any(|p| p == name))
            && !self
                .locals
                .iter()
                .any(|l| l.name == name && l.generated == generated)
    }
}
