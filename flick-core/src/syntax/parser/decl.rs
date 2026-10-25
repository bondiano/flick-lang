//! Module header, imports and declarations.

use super::{Ctx, K, Parser, expr, pattern, types};
use crate::diagnostics::codes;
use crate::syntax::kind::FOREIGN_MODES;

pub(super) fn source_file(p: &mut Parser) {
    let m = p.start();
    p.items(top_decl, false, |_| false);
    p.complete(m, K::SourceFile);
}

fn top_decl(p: &mut Parser) {
    match p.current() {
        K::ModuleKw => module_header(p),
        K::ImportKw => import(p),
        K::RecKw => rec_group(p),
        K::KindKw => kind_decl(p),
        K::TypeKw if p.nth(1) == K::ErrorKw => error_decl(p),
        K::ServiceKw => service_decl(p),
        K::ClassKw => class_decl(p),
        K::InstanceKw => instance_decl(p),
        K::ForeignKw => foreign_decl(p),
        K::SqlKw if p.nth(1) == K::Ident => sql_table(p),
        _ => member(p),
    }
}

/// Declarations allowed in `class`, `instance`, `service` and `rec` bodies,
/// and at the top level.
fn member(p: &mut Parser) {
    match p.current() {
        K::LetKw => let_decl(p),
        K::TypeKw | K::OpaqueKw | K::RefinedKw => type_decl(p),
        // `unboxed type` and `untagged type`: the words are names elsewhere
        K::Ident if (p.at_word("unboxed") || p.at_word("untagged")) && p.nth(1) == K::TypeKw => {
            type_decl(p);
        }
        K::Ident if p.at_word("val") && p.nth(1) == K::Ident && p.nth(2) == K::Colon => {
            let val = p.raw();
            p.error_at(
                codes::S010,
                p.span(val.start, val.end),
                "Flick has no `val`: a class method is a plain signature `name : T`".into(),
            );
            let m = p.start();
            p.bump();
            p.complete(m, K::Error);
            signature(p);
        }
        K::Ident if p.nth(1) == K::Colon => signature(p),
        _ => {
            p.error_expected("a declaration");
            p.recover();
        }
    }
}

fn module_path(p: &mut Parser) {
    let m = p.start();
    if p.expect(K::Con) {
        while expr::path_continues(p) && p.toks[p.pos + 1].kind == K::Con {
            p.bump();
            p.bump();
        }
    }
    p.complete(m, K::ModulePath);
}

fn module_header(p: &mut Parser) {
    let m = p.start();
    p.bump();
    module_path(p);
    if p.at(K::ExposingKw) {
        exposing(p);
    }
    p.complete(m, K::ModuleHeader);
}

fn import(p: &mut Parser) {
    let m = p.start();
    p.bump();
    module_path(p);
    if p.eat(K::AsKw) {
        p.expect(K::Con);
    }
    if p.at(K::ExposingKw) {
        exposing(p);
    }
    p.complete(m, K::ImportDecl);
}

fn exposing(p: &mut Parser) {
    let m = p.start();
    p.bump();
    if p.at(K::LParen) {
        p.delimited(K::RParen, expose_item);
    } else {
        p.error_expected("`(` with the exposed names");
    }
    p.complete(m, K::Exposing);
}

/// `name`, `Type`, `Type(..)` or `..`.
fn expose_item(p: &mut Parser) {
    let m = p.start();
    match p.current() {
        K::Ident | K::DotDot => p.bump(),
        K::Con => {
            p.bump();
            if p.at(K::LParen) && p.adjacent() {
                let opener = p.raw();
                p.bump();
                p.expect(K::DotDot);
                p.expect_closer(K::RParen, opener);
            }
        }
        _ => p.error_expected("an exposed name"),
    }
    p.complete(m, K::ExposeItem);
}

pub(super) fn signature(p: &mut Parser) {
    let m = p.start();
    p.bump();
    p.bump();
    types::type_(p);
    p.complete(m, K::Signature);
}

/// `let [rec] name params [: T] = body`, `let pattern [: T] = body`, or
/// `let assert pattern [: T] = body [as "message"]`.
pub(super) fn let_decl(p: &mut Parser) {
    let m = p.start();
    let let_kw = p.raw();
    p.bump();
    if p.raw().kind == K::Star && p.adjacent() {
        let star = p.raw();
        p.error_at(
            codes::S010,
            p.span(let_kw.start, star.end),
            "Flick has no `let*` binding operators: bind results with plain `let`".into(),
        );
        p.bump();
    }
    let assert = p.eat(K::AssertKw);
    if p.at(K::RecKw) && assert {
        p.error_expected("a pattern after `let assert`");
    }
    if p.eat(K::RecKw) && p.at(K::AssertKw) {
        p.error_expected("a name after `let rec`");
        p.bump();
    }
    if p.at(K::Ident) {
        let name = p.start();
        p.bump();
        p.complete(name, K::VarPat);
        if pattern::at_atom_start(p) {
            let params = p.start();
            while pattern::at_atom_start(p) {
                pattern::atom_pattern(p);
            }
            p.complete(params, K::Params);
        }
    } else {
        pattern::pattern(p);
    }
    if p.eat(K::Colon) {
        types::type_(p);
    }
    if p.expect(K::Eq) {
        p.body(K::Block, expr::stmt);
    }
    if p.at(K::AsKw) {
        if !assert {
            p.error_expected("the end of the `let`: only `let assert` takes `as \"message\"`");
        }
        p.bump();
        p.expect(K::String);
    }
    p.complete(m, K::LetDecl);
}

fn rec_group(p: &mut Parser) {
    let m = p.start();
    p.bump();
    p.braces(K::Members, member, |_| false);
    p.complete(m, K::RecGroup);
}

/// `name`, or `(name : Kind)`.
fn type_params(p: &mut Parser) {
    while matches!(p.current(), K::Ident | K::LParen) {
        let m = p.start();
        if p.at(K::Ident) {
            p.bump();
        } else {
            let opener = p.raw();
            p.bump();
            p.push(Ctx::Bracket);
            if p.expect(K::Ident) && p.expect(K::Colon) {
                types::type_(p);
            }
            p.pop();
            p.expect_closer(K::RParen, opener);
        }
        p.complete(m, K::TypeParam);
    }
}

/// `type Name params = variants`, `type alias Name params = T`, the nominal
/// `opaque` and `refined` forms, or an abstract `type Name params`; a
/// variant type may name its JS representation, `unboxed` or `untagged`.
fn type_decl(p: &mut Parser) {
    let m = p.start();
    let refined = p.at(K::RefinedKw);
    if !p.eat(K::OpaqueKw) {
        p.eat(K::RefinedKw);
    }
    // `unboxed type`, `opaque unboxed type`, `untagged type`
    if p.at_word("unboxed") || p.at_word("untagged") {
        p.bump();
    }
    if p.expect(K::TypeKw) {
        // `alias` is a word only here: a type name is capitalized
        let alias = !refined && p.at_word("alias") && p.nth(1) == K::Con;
        if alias {
            p.bump();
        }
        if p.expect(K::Con) {
            type_params(p);
            if alias {
                if p.expect(K::Eq) {
                    types::type_(p);
                }
            } else if p.eat(K::Eq) {
                if refined {
                    types::type_(p);
                    let r = p.start();
                    if p.expect(K::WhereKw) {
                        expr::expr(p);
                    }
                    p.complete(r, K::Refinement);
                } else {
                    variants(p);
                }
            }
            deriving(p);
        }
    }
    p.complete(m, K::TypeDecl);
}

fn variants(p: &mut Parser) {
    p.eat(K::Pipe);
    variant(p);
    while p.eat(K::Pipe) {
        variant(p);
    }
}

/// `Con field…`; a field is an atomic type, so `Some (List a)` needs the parentheses.
fn variant(p: &mut Parser) {
    let m = p.start();
    if p.expect(K::Con) {
        if p.at_word("of") {
            let of = p.raw();
            p.error_at(
                codes::S010,
                p.span(of.start, of.end),
                "Flick has no `of` before constructor fields: write `Circle Number`, `Rect Number Number`".into(),
            );
            p.bump();
        }
        while types::at_atom_start(p) {
            types::capture(p);
        }
    }
    p.complete(m, K::Variant);
}

fn deriving(p: &mut Parser) {
    if !p.at(K::DerivingKw) {
        return;
    }
    let m = p.start();
    p.bump();
    expr::con_path(p);
    while p.eat(K::Comma) {
        expr::con_path(p);
    }
    p.complete(m, K::DerivingClause);
}

/// `sql table Name "table" { column : sql type words as T [via codec] }`.
fn sql_table(p: &mut Parser) {
    let m = p.start();
    p.bump();
    if !p.at_word("table") {
        p.error_expected("`table`");
    }
    p.bump();
    if p.expect(K::Con) && p.expect(K::String) {
        p.braces(K::Block, sql_column, |_| false);
    }
    p.complete(m, K::SqlTableDecl);
}

fn sql_column(p: &mut Parser) {
    let m = p.start();
    if p.expect(K::Ident) && p.expect(K::Colon) {
        while p.at(K::Ident) || p.at(K::NotKw) {
            p.bump();
        }
        if p.expect(K::AsKw) {
            p.sql_table = true;
            types::type_(p);
            p.sql_table = false;
            if p.at_word("via") {
                p.bump();
                expr::expr(p);
            }
        }
    }
    p.complete(m, K::SqlColumn);
}

fn kind_decl(p: &mut Parser) {
    let m = p.start();
    p.bump();
    if p.expect(K::Con) && p.expect(K::Eq) {
        variants(p);
    }
    p.complete(m, K::KindDecl);
}

/// `type error Name params [= Name field…]`: the one constructor is named like the error.
fn error_decl(p: &mut Parser) {
    let m = p.start();
    p.bump();
    p.bump();
    let name = p.at(K::Con).then(|| p.text_of(p.raw()));
    if p.expect(K::Con) {
        type_params(p);
        if p.eat(K::Eq) {
            let con = p.raw();
            if con.kind == K::Con && Some(p.text_of(con)) != name {
                p.error_at(
                    codes::S007,
                    p.span(con.start, con.end),
                    format!(
                        "the constructor of an error is named like the error: `{}`",
                        name.unwrap_or_default()
                    ),
                );
            }
            variant(p);
        }
        deriving(p);
    }
    p.complete(m, K::ErrorDecl);
}

fn service_decl(p: &mut Parser) {
    let m = p.start();
    p.bump();
    if p.expect(K::Con) {
        type_params(p);
        if p.eat(K::Eq) {
            p.body(K::Members, member);
        }
    }
    p.complete(m, K::ServiceDecl);
}

/// Whether `=>` comes before the `=` of a class or instance head.
fn context_ahead(p: &Parser) -> bool {
    for i in p.pos..p.toks.len() {
        if p.sep_at(i).is_some() {
            break;
        }
        match p.toks[i].kind {
            K::FatArrow => return true,
            K::Eq => break,
            _ => {}
        }
    }
    false
}

fn context(p: &mut Parser) {
    if context_ahead(p) {
        let m = p.start();
        types::union(p);
        p.expect(K::FatArrow);
        p.complete(m, K::Context);
    }
}

/// `class [C =>] Name params [| a -> b, …] [= members]`; a class without
/// methods only relates its parameters.
fn class_decl(p: &mut Parser) {
    let m = p.start();
    p.bump();
    context(p);
    if p.expect(K::Con) {
        type_params(p);
        if p.eat(K::Pipe) {
            fundep(p);
            while p.eat(K::Comma) {
                fundep(p);
            }
        }
        if p.eat(K::Eq) {
            p.body(K::Members, member);
        }
    }
    p.complete(m, K::ClassDecl);
}

fn fundep(p: &mut Parser) {
    let m = p.start();
    while p.eat(K::Ident) {}
    if p.expect(K::Arrow) {
        if !p.at(K::Ident) {
            p.error_expected("a class parameter");
        }
        while p.eat(K::Ident) {}
    }
    p.complete(m, K::Fundep);
}

fn instance_decl(p: &mut Parser) {
    let m = p.start();
    p.bump();
    context(p);
    types::union(p);
    if p.eat(K::Eq) {
        p.body(K::Members, member);
    }
    p.complete(m, K::InstanceDecl);
}

const FOREIGN_META: [&str; 9] = [
    "arity",
    "this",
    "catch",
    "cancel",
    "nullable",
    "retention",
    "optional",
    "variadic",
    "reviewed",
];

/// `foreign mode name from (global "path" | "module" "export")` + metadata
/// lines, or `foreign blocked name "reason"`.
fn foreign_decl(p: &mut Parser) {
    let m = p.start();
    p.bump();
    let mode = p.at(K::Ident).then(|| p.text_of(p.raw()));
    if !mode.is_some_and(|w| FOREIGN_MODES.contains(&w)) {
        p.error_expected("a foreign mode: `pure`, `sync`, `async`, `effect` or `blocked`");
    }
    // an unknown mode word still takes the mode position
    if p.at(K::Ident) && p.nth(1) == K::Ident {
        p.bump();
    }
    p.expect(K::Ident);
    if mode == Some("blocked") {
        p.expect(K::String);
        p.complete(m, K::ForeignDecl);
        return;
    }
    let source = p.start();
    if p.expect(K::FromKw) {
        if p.at_word("global") {
            p.bump();
        } else {
            p.expect(K::String);
        }
        p.expect(K::String);
    }
    p.complete(source, K::ForeignSource);
    if p.sep().is_none() && p.raw().line_start {
        p.body(K::ForeignMeta, meta_item);
    }
    p.complete(m, K::ForeignDecl);
}

fn meta_item(p: &mut Parser) {
    let m = p.start();
    if p.at(K::Ident) {
        let name = p.raw();
        if !FOREIGN_META.contains(&p.text_of(name)) {
            p.error_at(
                codes::S011,
                p.span(name.start, name.end),
                format!(
                    "unknown foreign metadata `{}`; expected one of: {}",
                    p.text_of(name),
                    FOREIGN_META.join(", ")
                ),
            );
        }
        p.bump();
        while expr::at_arg_start(p) {
            expr::postfix(p);
        }
    } else {
        p.error_expected("foreign metadata such as `arity 1`");
        p.recover();
    }
    p.complete(m, K::MetaItem);
}
