//! Contextual loop recognition and lexical control expressions.
use super::expr::{at_operand_start, expr, head_without_braces, stmt};
use super::{Done, K, Parser};

pub(super) fn while_ahead(p: &Parser) -> bool {
    let mut depth = 0;
    let mut conditional = 0;
    let mut match_or_try = false;
    let mut lambda = false;
    for i in p.pos + 1..p.toks.len() {
        let kind = p.toks[i].kind;
        if depth == 0 {
            if p.sep_at(i).is_some()
                || matches!(
                    kind,
                    K::Comma | K::Semi | K::RParen | K::RBracket | K::RBrace
                )
            {
                return false;
            }
            if kind == K::DoKw {
                return true;
            }
            match kind {
                K::IfKw => conditional += 1,
                K::ThenKw | K::ElseKw if conditional == 0 => return false,
                K::ElseKw => conditional -= 1,
                K::MatchKw | K::TryKw => match_or_try = true,
                K::WithKw | K::Pipe if !match_or_try => return false,
                K::FunKw => lambda = true,
                K::Arrow if !match_or_try && !lambda => return false,
                _ => {}
            }
        }
        match kind {
            K::LParen | K::LBracket | K::LBrace => depth += 1,
            K::RParen | K::RBracket | K::RBrace => depth -= 1,
            _ => {}
        }
    }
    false
}

pub(super) fn control_expr(p: &mut Parser) -> Done {
    let m = p.start();
    let keyword = if p.at_word("loop") {
        K::LoopKw
    } else if p.at_word("while") {
        K::WhileKw
    } else {
        p.current()
    };
    p.bump_as(keyword);
    let kind = match keyword {
        K::LoopKw => {
            p.braces(K::Block, stmt, |_| false);
            K::LoopExpr
        }
        K::WhileKw => {
            head_without_braces(p, |p| {
                expr(p);
            });
            if p.expect(K::DoKw) {
                if p.at(K::LBrace) {
                    p.braces(K::Block, stmt, |_| false);
                } else {
                    p.body(K::Block, stmt);
                }
            }
            K::WhileExpr
        }
        K::BreakKw => {
            if at_operand_start(p) {
                expr(p);
            }
            K::BreakExpr
        }
        _ => K::ContinueExpr,
    };
    p.complete(m, kind)
}
