//! Expressions and block statements.

use super::{Done, K, Parser, decl, pattern, plugin, types};
use crate::diagnostics::codes;

/// A statement of a block: `let`, a local signature or an expression.
pub(super) fn stmt(p: &mut Parser) {
    match p.current() {
        K::LetKw => decl::let_decl(p),
        K::Ident if p.nth(1) == K::Colon => decl::signature(p),
        _ => {
            expr(p);
        }
    }
}

pub(super) fn expr(p: &mut Parser) -> Option<Done> {
    expr_bp(p, 0)
}

/// Binding powers of the fixed operator table, weakest first (grammar.md).
fn infix(kind: K) -> Option<(u8, u8)> {
    Some(match kind {
        K::PipeGt => (2, 3),
        K::OrOr => (4, 4),
        K::AndAnd => (6, 6),
        K::EqEq | K::NotEq | K::Lt | K::Le | K::Gt | K::Ge => (8, 9),
        K::PlusPlus => (10, 10),
        K::Plus | K::Minus => (12, 13),
        K::Star | K::Slash | K::Percent => (14, 15),
        K::StarStar => (18, 18),
        _ => return None,
    })
}

/// Operand of a prefix operator: binds tighter than `*`, looser than `**`.
const PREFIX_OPERAND: u8 = 18;

/// A range binds looser than `++` and arithmetic, tighter than comparisons.
const RANGE: (u8, u8) = (9, 10);

fn is_comparison(kind: K) -> bool {
    matches!(kind, K::EqEq | K::NotEq | K::Lt | K::Le | K::Gt | K::Ge)
}

fn expr_bp(p: &mut Parser, min_bp: u8) -> Option<Done> {
    if !p.enter() {
        return None;
    }
    let mut lhs = prefix(p);
    while let Some(done) = lhs {
        let op = p.current();
        if op == K::LArrow {
            let m = p.precede(done);
            let span = p.here_span();
            p.error_at(
                codes::S013,
                span,
                "`<-` binds only in `do` and `for`: bind with `let`, or write `a < -1` for a comparison"
                    .into(),
            );
            p.bump();
            expr_bp(p, 0);
            lhs = Some(p.complete(m, K::Error));
            break;
        }
        if matches!(op, K::DotDot | K::DotDotLt) {
            if RANGE.0 < min_bp {
                break;
            }
            let m = p.precede(done);
            p.bump();
            // `a..` has no end; `a..<` needs one
            if op == K::DotDotLt || at_operand_start(p) {
                expr_bp(p, RANGE.1);
            }
            lhs = Some(p.complete(m, K::RangeExpr));
            if matches!(p.current(), K::DotDot | K::DotDotLt) {
                let span = p.here_span();
                p.error_at(
                    codes::S007,
                    span,
                    "ranges do not chain: a range is `a..b`, `a..<b` or `a..`".into(),
                );
            }
            continue;
        }
        let Some((left, right)) = infix(op) else {
            break;
        };
        if left < min_bp {
            break;
        }
        let m = p.precede(done);
        p.bump();
        expr_bp(p, right);
        lhs = Some(p.complete(m, K::BinExpr));
        if is_comparison(op) && is_comparison(p.current()) {
            let span = p.here_span();
            p.error_at(
                codes::S007,
                span,
                "comparison operators do not chain; combine the comparisons with `&&`".into(),
            );
        }
    }
    p.leave();
    lhs
}

fn prefix(p: &mut Parser) -> Option<Done> {
    if matches!(
        p.current(),
        K::Minus | K::NotKw | K::AwaitKw | K::IgnoreKw | K::EchoKw
    ) {
        let m = p.start();
        let echo = p.at(K::EchoKw);
        p.bump();
        // a bare `echo`, as in `x |> echo`, is a function
        if !echo || at_operand_start(p) {
            expr_bp(p, PREFIX_OPERAND);
        }
        return Some(p.complete(m, K::PrefixExpr));
    }
    match p.current() {
        K::FunKw => Some(fun_expr(p)),
        K::MatchKw => Some(match_expr(p)),
        K::TryKw => Some(try_expr(p)),
        K::IfKw => Some(if_expr(p)),
        K::Lt if is_jsx(p) => Some(not_in_v1(
            p,
            "Flick has no JSX `<…>`: build UI with `ui { … }` or `Ui` functions",
        )),
        _ => app(p),
    }
}

/// Whether the next token can start the operand of a prefix operator.
fn at_operand_start(p: &Parser) -> bool {
    at_arg_start(p)
        || matches!(
            p.current(),
            K::Minus
                | K::NotKw
                | K::AwaitKw
                | K::IgnoreKw
                | K::EchoKw
                | K::FunKw
                | K::MatchKw
                | K::TryKw
                | K::IfKw
                | K::DoKw
                | K::ForKw
        )
}

fn is_jsx(p: &Parser) -> bool {
    let next = p.toks[p.pos + 1];
    matches!(next.kind, K::Ident | K::Con | K::Gt) && next.start == p.raw().end
}

/// Reports a construct from other ML languages that Flick does not have, and
/// skips the statement.
fn not_in_v1(p: &mut Parser, message: &str) -> Done {
    let span = p.here_span();
    p.error_at(codes::S010, span, message.into());
    let m = p.start();
    while p.sep().is_none() && !(p.raw().kind.is_closer() && p.in_brackets()) {
        p.bump();
    }
    p.complete(m, K::Error)
}

fn app(p: &mut Parser) -> Option<Done> {
    let head = postfix(p)?;
    if !at_arg_start(p) {
        return Some(head);
    }
    let m = p.precede(head);
    while at_arg_start(p) {
        argument(p);
    }
    Some(p.complete(m, K::AppExpr))
}

pub(super) fn at_arg_start(p: &Parser) -> bool {
    match p.current() {
        K::Ident => !at_and(p),
        K::Con
        | K::Number
        | K::BigInt
        | K::String
        | K::TrueKw
        | K::FalseKw
        | K::LParen
        | K::LBracket
        | K::TaskKw
        | K::TodoKw
        | K::ScopeKw
        | K::ProvideKw
        | K::UiKw => true,
        K::LBrace => !p.no_brace,
        _ => false,
    }
}

/// An argument of an application, where `server` is a name: `f server x`.
fn argument(p: &mut Parser) -> Option<Done> {
    if !p.at_word("server") {
        return postfix(p);
    }
    let m = p.start();
    p.bump();
    let name = p.complete(m, K::NameExpr);
    accesses(p, name)
}

/// An atom followed by `.field` accesses and `?`, written without spaces.
pub(super) fn postfix(p: &mut Parser) -> Option<Done> {
    let lhs = atom(p)?;
    accesses(p, lhs)
}

/// `lhs` followed by `.field` accesses and `?`.
fn accesses(p: &mut Parser, mut lhs: Done) -> Option<Done> {
    loop {
        if p.at(K::Dot) && p.adjacent() {
            let m = p.precede(lhs);
            p.bump();
            p.expect(K::Ident);
            lhs = p.complete(m, K::FieldExpr);
        } else if p.at(K::Question) && p.adjacent() {
            let m = p.precede(lhs);
            p.bump();
            lhs = p.complete(m, K::QuestionExpr);
        } else {
            return Some(lhs);
        }
    }
}

/// Whether `.Name` or `.name` directly continues a qualified path.
pub(super) fn path_continues(p: &Parser) -> bool {
    let dot = p.raw();
    let next = p.toks[(p.pos + 1).min(p.toks.len() - 1)];
    dot.kind == K::Dot
        && p.adjacent()
        && matches!(next.kind, K::Con | K::Ident)
        && next.start == dot.end
}

/// `Con(.Con)*(.name)?` as tokens of the current node.
pub(super) fn con_path(p: &mut Parser) {
    if !p.expect(K::Con) {
        return;
    }
    while path_continues(p) {
        p.bump();
        let lower = p.raw().kind == K::Ident;
        p.bump();
        if lower {
            break;
        }
    }
}

/// `todo` and the string that says what it stands for.
fn todo(p: &mut Parser) -> K {
    p.bump();
    p.eat(K::String);
    K::TodoExpr
}

fn atom(p: &mut Parser) -> Option<Done> {
    let m = p.start();
    let kind = match p.current() {
        K::Ident if plugin::at_block(p) => plugin::block(p),
        K::Ident => {
            p.bump();
            K::NameExpr
        }
        K::Con => {
            con_path(p);
            K::NameExpr
        }
        K::Number | K::BigInt | K::String | K::TrueKw | K::FalseKw => {
            p.bump();
            K::Literal
        }
        K::TodoKw => todo(p),
        K::LParen => paren(p),
        K::LBracket => {
            p.delimited(K::RBracket, |p| {
                expr(p);
            });
            K::ListExpr
        }
        K::LBrace if !p.no_brace => record(p),
        K::DoKw => do_expr(p),
        K::ForKw => for_expr(p),
        K::TaskKw | K::UiKw => {
            let kind = if p.at(K::TaskKw) {
                K::TaskExpr
            } else {
                K::UiExpr
            };
            p.bump();
            p.braces(K::Block, stmt, |_| false);
            kind
        }
        K::ScopeKw => {
            p.bump();
            p.eat(K::Ident);
            p.braces(K::Block, stmt, |_| false);
            K::ScopeExpr
        }
        K::ProvideKw => {
            p.bump();
            head_without_braces(p, |p| {
                expr(p);
            });
            p.braces(K::Block, stmt, |_| false);
            K::ProvideExpr
        }
        _ => {
            p.error_expected("an expression");
            p.complete(m, K::Error);
            return None;
        }
    };
    Some(p.complete(m, kind))
}

fn head_without_braces(p: &mut Parser, head: fn(&mut Parser)) {
    let saved = p.no_brace;
    p.no_brace = true;
    head(p);
    p.no_brace = saved;
}

/// `()`, `(e)`, `(e : T)` or `(a, b, …)`; returns the node kind.
fn paren(p: &mut Parser) -> K {
    let opener = p.raw();
    p.bump();
    if p.raw().kind == K::RParen {
        p.bump();
        return K::UnitExpr;
    }
    p.push(super::Ctx::Bracket);
    expr(p);
    let kind = if p.eat(K::Colon) {
        types::type_(p);
        K::AnnotExpr
    } else if p.at(K::Comma) {
        while p.eat(K::Comma) && !p.at(K::RParen) {
            expr(p);
        }
        K::TupleExpr
    } else {
        K::ParenExpr
    };
    p.pop();
    p.expect_closer(K::RParen, opener);
    kind
}

/// `{ f = v; … }` or `{ base with f = v; … }`; returns the node kind.
fn record(p: &mut Parser) -> K {
    let opener = p.raw();
    p.bump();
    if p.raw().kind == K::RBrace {
        p.bump();
        return K::RecordExpr;
    }
    p.push_aligned_here();
    let kind = if p.at(K::Ident) && p.nth(1) == K::Eq {
        p.items(record_field, true, |_| false);
        K::RecordExpr
    } else {
        expr(p);
        if p.expect(K::WithKw) {
            p.push_aligned_here();
            p.items(record_field, true, |_| false);
            p.pop();
        }
        K::RecordUpdate
    };
    p.pop();
    p.expect_closer(K::RBrace, opener);
    kind
}

fn record_field(p: &mut Parser) {
    let m = p.start();
    if p.expect(K::Ident) && p.expect(K::Eq) {
        p.body(K::Block, stmt);
    }
    p.complete(m, K::RecordField);
}

fn do_expr(p: &mut Parser) -> K {
    let span = p.here_span();
    p.bump();
    if p.at(K::LBrace) {
        p.braces(K::Block, do_stmt, |_| false);
    } else {
        p.error_at(
            codes::S010,
            span,
            "a `do` block takes braces: `do { x <- e; … }`".into(),
        );
    }
    K::DoExpr
}

/// `for { clauses } yield e`, or the clauses inline, separated by `;` or a
/// guard's `if`: `[for x <- xs if x > 0 yield x]`. `do body` instead of
/// `yield e` runs the body for each element.
fn for_expr(p: &mut Parser) -> K {
    p.bump();
    if p.at(K::LBrace) {
        p.braces(K::Block, do_stmt, |_| false);
    } else {
        let m = p.start();
        loop {
            for_clause(p);
            if !(p.eat(K::Semi) || p.at(K::IfKw)) {
                break;
            }
        }
        p.complete(m, K::Block);
    }
    if p.eat(K::DoKw) {
        if p.at(K::LBrace) {
            p.braces(K::Block, stmt, |_| false);
        } else {
            p.body(K::Block, stmt);
        }
    } else if p.expect(K::YieldKw) {
        expr(p);
    }
    K::ForExpr
}

/// A clause of an inline `for`: a guard, a `let` or a binding.
fn for_clause(p: &mut Parser) {
    match p.current() {
        K::IfKw => {
            let m = p.start();
            p.bump();
            expr(p);
            p.complete(m, K::GuardStmt);
        }
        K::LetKw => decl::let_decl(p),
        _ => {
            let m = p.start();
            pattern::pattern(p);
            p.expect(K::LArrow);
            expr(p);
            p.complete(m, K::BindStmt);
        }
    }
}

/// A statement of `do { … }`: `pattern <- e`, a guard `if c` without `then`,
/// or a statement of an ordinary block.
fn do_stmt(p: &mut Parser) {
    if p.at(K::IfKw) && !ahead(p, K::ThenKw) {
        let m = p.start();
        p.bump();
        expr(p);
        p.complete(m, K::GuardStmt);
    } else if !p.at(K::LetKw) && ahead(p, K::LArrow) {
        let first = bind_stmt(p);
        if at_and(p) {
            let m = p.precede(first);
            while at_and(p) {
                p.bump_as(K::AndKw);
                bind_stmt(p);
            }
            p.complete(m, K::BindGroup);
        }
    } else {
        stmt(p);
    }
}

fn bind_stmt(p: &mut Parser) -> Done {
    let m = p.start();
    pattern::pattern(p);
    p.expect(K::LArrow);
    expr(p);
    p.complete(m, K::BindStmt)
}

/// Whether the word `and` joins another binding to a `do` group here.
fn at_and(p: &Parser) -> bool {
    p.at_word("and") && ahead(p, K::LArrow)
}

/// Whether `kind` comes before the current statement ends, outside brackets.
fn ahead(p: &Parser, kind: K) -> bool {
    let mut depth = 0u32;
    for i in p.pos..p.toks.len() {
        if i > p.pos && p.sep_at(i).is_some() {
            return false;
        }
        match p.toks[i].kind {
            K::Eof => return false,
            K::LParen | K::LBracket | K::LBrace => depth += 1,
            K::RParen | K::RBracket | K::RBrace | K::Semi | K::Eq if depth == 0 => return false,
            K::RParen | K::RBracket | K::RBrace => depth -= 1,
            k if depth == 0 && k == kind => return true,
            _ => {}
        }
    }
    false
}

fn fun_expr(p: &mut Parser) -> Done {
    let m = p.start();
    p.bump();
    if !pattern::at_atom_start(p) {
        p.error_expected("a parameter");
    }
    while pattern::at_atom_start(p) {
        pattern::atom_pattern(p);
    }
    if p.expect(K::Arrow) {
        p.body(K::Block, stmt);
    }
    p.complete(m, K::FunExpr)
}

fn arms(p: &mut Parser) {
    if !p.at(K::Pipe) {
        p.error_expected("a match arm `| pattern -> …`");
    }
    while p.at(K::Pipe) {
        let m = p.start();
        p.bump();
        pattern::or_pattern(p);
        if p.at(K::WhenKw) {
            let guard = p.start();
            p.bump();
            expr(p);
            p.complete(guard, K::Guard);
        }
        if p.expect(K::Arrow) {
            p.body(K::Block, stmt);
        }
        p.complete(m, K::MatchArm);
    }
}

fn match_expr(p: &mut Parser) -> Done {
    let m = p.start();
    p.bump();
    expr(p);
    if p.expect(K::WithKw) {
        arms(p);
    }
    p.complete(m, K::MatchExpr)
}

fn try_expr(p: &mut Parser) -> Done {
    let m = p.start();
    p.bump();
    p.body(K::Block, stmt);
    if p.expect(K::WithKw) {
        arms(p);
    }
    p.complete(m, K::TryExpr)
}

fn if_expr(p: &mut Parser) -> Done {
    let m = p.start();
    p.bump();
    expr(p);
    if p.expect(K::ThenKw) {
        p.body(K::Block, stmt);
        // without `else` the `if` is `Unit`, which the checker requires of `then`
        if p.eat(K::ElseKw) {
            if p.at(K::IfKw) && !p.raw().line_start {
                if_expr(p);
            } else {
                p.body(K::Block, stmt);
            }
        }
    }
    p.complete(m, K::IfExpr)
}
