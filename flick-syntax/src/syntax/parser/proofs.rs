//! `proof name params where goal = proof`: a statement about the module's
//! definitions and its proof, which the compiler checks and erases. The goal
//! and the proof are ordinary expressions, except a `same_type` goal, whose
//! two arguments are types, and a premise parameter `(h : goal)`.

use super::{Ctx, Parser, expr, pattern, types};
use crate::syntax::kind::SyntaxKind as K;

/// The words that begin a goal: what a premise parameter names.
const GOALS: [&str; 3] = ["equal", "holds", "same_type"];

/// `proof name`: the word is a name everywhere else.
pub(super) fn at_proof(p: &Parser) -> bool {
    p.at_word("proof") && p.nth(1) == K::Ident
}

pub(super) fn proof_decl(p: &mut Parser) {
    let m = p.start();
    p.bump_as(K::ProofKw);
    let name = p.start();
    p.bump();
    p.complete(name, K::VarPat);
    if at_param(p) {
        let params = p.start();
        while at_param(p) {
            if at_premise(p) {
                premise(p);
            } else {
                pattern::atom_pattern(p);
            }
        }
        p.complete(params, K::Params);
    }
    if p.expect(K::WhereKw) {
        goal(p);
    }
    if p.expect(K::Eq) {
        p.body(K::Block, expr::stmt);
    }
    p.complete(m, K::ProofDecl);
}

fn at_param(p: &Parser) -> bool {
    pattern::at_atom_start(p)
}

/// `(h : equal …)`: a parameter that is a proof of a goal.
fn at_premise(p: &Parser) -> bool {
    p.at(K::LParen)
        && p.nth(1) == K::Ident
        && p.nth(2) == K::Colon
        && p.nth(3) == K::Ident
        && GOALS.iter().any(|w| super::decl::word_at(p, 3, w))
}

fn premise(p: &mut Parser) {
    let m = p.start();
    let opener = p.raw();
    p.bump();
    p.push(Ctx::Bracket);
    let name = p.start();
    p.bump();
    p.complete(name, K::VarPat);
    p.bump();
    goal(p);
    p.pop();
    p.expect_closer(K::RParen, opener);
    p.complete(m, K::Premise);
}

/// `same_type A B`, or an expression: `equal l r`, `holds p`.
fn goal(p: &mut Parser) {
    let m = p.start();
    if p.at_word("same_type") {
        p.bump();
        types::atom(p);
        types::atom(p);
        p.complete(m, K::SameTypeGoal);
    } else {
        expr::expr(p);
        p.complete(m, K::Goal);
    }
}
