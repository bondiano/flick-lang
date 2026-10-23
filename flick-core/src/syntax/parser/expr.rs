//! Expressions and block statements.

use std::mem;

use super::{Done, K, Parser, decl, pattern, types};
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
    if matches!(p.current(), K::Minus | K::NotKw | K::AwaitKw | K::IgnoreKw) {
        let m = p.start();
        p.bump();
        expr_bp(p, PREFIX_OPERAND);
        return Some(p.complete(m, K::PrefixExpr));
    }
    match p.current() {
        K::FunKw => Some(fun_expr(p)),
        K::MatchKw => Some(match_expr(p)),
        K::TryKw => Some(try_expr(p)),
        K::IfKw => Some(if_expr(p)),
        K::DoKw => Some(not_in_v1(
            p,
            "Flick has no `do` notation: write effectful calls directly and bind them with `let`, \
             or build a computation with `task { … }` and run it with `await`",
        )),
        K::Lt if is_jsx(p) => Some(not_in_v1(
            p,
            "Flick has no JSX `<…>`: build UI with `ui { … }` or `Ui` functions",
        )),
        _ => app(p),
    }
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
        postfix(p);
    }
    Some(p.complete(m, K::AppExpr))
}

const SQL_CLAUSES: [&str; 17] = [
    "join",
    "left_join",
    "on",
    "group_by",
    "having",
    "order_by",
    "limit",
    "offset",
    "select",
    "union",
    "union_all",
    "insert_into",
    "values",
    "update",
    "set",
    "delete_from",
    "returning",
];

fn at_sql_clause(p: &Parser) -> bool {
    matches!(p.current(), K::FromKw | K::WhereKw)
        || (p.at(K::Ident) && SQL_CLAUSES.contains(&p.text_of(p.raw())))
}

pub(super) fn at_arg_start(p: &Parser) -> bool {
    match p.current() {
        K::Ident => !(p.sql_depth > 0 && at_sql_clause(p)),
        K::Con
        | K::Number
        | K::BigInt
        | K::String
        | K::TrueKw
        | K::FalseKw
        | K::LParen
        | K::LBracket
        | K::TaskKw
        | K::ScopeKw
        | K::ProvideKw
        | K::SqlKw
        | K::UiKw
        | K::CliKw
        | K::CodecKw
        | K::SchemaKw => true,
        K::LBrace => !p.no_brace,
        K::Dollar => p.sql_depth > 0,
        _ => false,
    }
}

/// An atom followed by `.field` accesses written without spaces.
pub(super) fn postfix(p: &mut Parser) -> Option<Done> {
    let mut lhs = atom(p)?;
    while p.at(K::Dot) && p.adjacent() {
        let m = p.precede(lhs);
        p.bump();
        p.expect(K::Ident);
        lhs = p.complete(m, K::FieldExpr);
    }
    Some(lhs)
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

fn atom(p: &mut Parser) -> Option<Done> {
    let m = p.start();
    let kind = match p.current() {
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
        K::LParen => paren(p),
        K::LBracket => {
            p.delimited(K::RBracket, |p| {
                expr(p);
            });
            K::ListExpr
        }
        K::LBrace if !p.no_brace => record(p),
        K::Dollar if p.sql_depth > 0 => {
            p.bump();
            // `$name`, `$name.field` or `$(e)`: a Flick expression, not SQL
            if matches!(p.current(), K::Ident | K::Con | K::LParen) && p.adjacent() {
                let depth = mem::replace(&mut p.sql_depth, 0);
                postfix(p);
                p.sql_depth = depth;
            } else {
                p.error_expected("a name or `(` right after `$`");
            }
            K::SqlCapture
        }
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
        K::CodecKw => {
            p.bump();
            head_without_braces(p, |p| {
                postfix(p);
            });
            if p.at(K::LBrace) {
                let r = p.start();
                let kind = record(p);
                p.complete(r, kind);
            } else {
                p.error_expected("`{` with the codec fields");
            }
            K::CodecExpr
        }
        K::SqlKw => {
            p.bump();
            p.eat(K::Ident);
            p.sql_depth += 1;
            p.braces(K::Block, sql_clause, at_sql_clause);
            p.sql_depth -= 1;
            K::SqlExpr
        }
        K::CliKw => {
            p.bump();
            p.expect(K::String);
            p.braces(K::Block, cli_clause, |_| false);
            K::CliExpr
        }
        K::SchemaKw => {
            p.bump();
            con_path(p);
            K::SchemaExpr
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

/// `from x in E`, `join x in E`, `on E`, `select E`, `group_by a, b`, ….
fn sql_clause(p: &mut Parser) {
    let m = p.start();
    if !at_sql_clause(p) {
        p.error_expected("a SQL clause (`from`, `where`, `select`, …)");
        p.recover();
        p.complete(m, K::Error);
        return;
    }
    let word = p.text_of(p.raw());
    p.bump();
    if matches!(word, "from" | "join" | "left_join") {
        p.expect(K::Ident);
        p.expect(K::InKw);
        expr(p);
    } else {
        expr(p);
        while p.eat(K::Comma) {
            expr(p);
        }
    }
    p.complete(m, K::SqlClause);
}

const CLI_CLAUSES: [&str; 5] = ["description", "flag", "arg", "subcommand", "handle"];

/// `flag name "alias" : T = default`, `arg name : T`, `subcommand E`, ….
fn cli_clause(p: &mut Parser) {
    let m = p.start();
    let word = if p.at(K::Ident) {
        p.text_of(p.raw())
    } else {
        ""
    };
    if !CLI_CLAUSES.contains(&word) {
        p.error_expected("a CLI clause (`flag`, `arg`, `subcommand`, `handle`, …)");
        p.recover();
        p.complete(m, K::Error);
        return;
    }
    p.bump();
    if matches!(word, "flag" | "arg") {
        p.expect(K::Ident);
        while p.eat(K::String) {}
        if p.expect(K::Colon) {
            types::type_(p);
            if p.eat(K::Eq) {
                expr(p);
            }
        }
    } else {
        expr(p);
    }
    p.complete(m, K::CliClause);
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
        if p.eat(K::ElseKw) {
            if p.at(K::IfKw) && !p.raw().line_start {
                if_expr(p);
            } else {
                p.body(K::Block, stmt);
            }
        } else {
            p.error_expected("`else`: `if` is an expression and needs both branches");
        }
    }
    p.complete(m, K::IfExpr)
}
