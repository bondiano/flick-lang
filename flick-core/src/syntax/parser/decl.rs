//! Module header, imports and declarations.

use super::{Ctx, Done, K, Parser, expr, pattern, types};
use crate::diagnostics::codes;
use crate::syntax::kind::FOREIGN_MODES;

pub(super) fn source_file(p: &mut Parser) {
    let m = p.start();
    p.items(top_decl, false, |_| false);
    p.complete(m, K::SourceFile);
}

/// The declarations of a fragment, in a `SourceFile`: the code a plugin
/// gives for a block that stands as declarations.
pub(super) fn declarations(p: &mut Parser) -> Done {
    let m = p.start();
    p.items(top_decl, false, |_| false);
    p.complete(m, K::SourceFile)
}

/// One declaration of a fragment, in a `SourceFile`: what the user wrote in
/// a block that stands as a declaration.
pub(super) fn declaration(p: &mut Parser) -> Done {
    let m = p.start();
    top_decl(p);
    p.complete(m, K::SourceFile)
}

fn top_decl(p: &mut Parser) {
    match p.current() {
        K::Ident if p.at_word("private") && p.nth(1) == K::ModuleKw => module_header(p),
        K::ModuleKw if p.nth(1) == K::Con && p.nth(2) == K::Eq => module_apply_decl(p),
        K::ModuleKw => module_header(p),
        K::ImportKw => import(p),
        K::RecKw => rec_group(p),
        K::KindKw => kind_decl(p),
        K::TypeKw if p.nth(1) == K::ErrorKw => error_decl(p),
        K::TypeKw if p.nth(1) == K::ModuleKw => module_type_decl(p),
        K::ServiceKw => service_decl(p),
        K::ClassKw => class_decl(p),
        K::InstanceKw => instance_decl(p),
        K::DerivingKw if p.nth(1) == K::InstanceKw => deriving_decl(p),
        K::ForeignKw => foreign_decl(p),
        // `effect Name`: the word is a name everywhere else
        K::Ident if p.at_word("effect") && p.nth(1) == K::Con => service_decl(p),
        K::Ident if super::plugin::at_declaration(p) => super::plugin::declaration(p),
        _ => member(p),
    }
}

/// Declarations allowed in `class`, `instance`, `service` and `rec` bodies,
/// and at the top level.
fn member(p: &mut Parser) {
    match p.current() {
        K::At => attribute(p),
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

include!("decl_modules.rs");
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
    let_rest(p, m, assert);
}

/// Whether the token `n` ahead is the identifier `word`.
pub(super) fn word_at(p: &Parser, n: usize, word: &str) -> bool {
    let t = p.toks[(p.pos + n).min(p.toks.len() - 1)];
    &p.text[t.start as usize..t.end as usize] == word
}

/// A `let` after its keywords: the name or pattern, the parameters, the
/// type and the body.
fn let_rest(p: &mut Parser, m: super::Marker, assert: bool) {
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
                    variants(p, true);
                }
            }
            deriving(p);
        }
    }
    p.complete(m, K::TypeDecl);
}

/// `type module Name =` followed by required declarations. `type T = U`
/// in this body is a manifest type equality, unlike an ordinary ADT.
fn module_type_decl(p: &mut Parser) {
    let m = p.start();
    p.bump();
    p.bump();
    if p.expect(K::Con) && p.expect(K::Eq) {
        if matches!(p.current(), K::Con | K::LParen) {
            module_signature(p);
        } else {
            p.body(K::Members, module_type_member);
        }
    }
    p.complete(m, K::ModuleTypeDecl);
}

fn module_type_member(p: &mut Parser) {
    match p.current() {
        K::ModuleKw => {
            let m = p.start();
            p.bump();
            if p.expect(K::Con) && p.expect(K::Colon) {
                module_signature(p);
            }
            p.complete(m, K::ModuleMember);
        }
        K::TypeKw => {
            let m = p.start();
            p.bump();
            if p.expect(K::Con) {
                type_params(p);
                if p.eat(K::Eq) {
                    types::type_(p);
                }
            }
            p.complete(m, K::ModuleTypeMember);
        }
        K::InstanceKw => {
            let m = p.start();
            p.bump();
            types::union(p);
            if p.expect(K::FromKw) {
                p.expect(K::Ident);
            }
            p.complete(m, K::ModuleInstanceReq);
        }
        K::Ident if p.nth(1) == K::Colon => signature(p),
        K::ClassKw | K::ServiceKw
            if p.nth(1) == K::Con && p.nth(2) == K::Eq && p.nth(3) == K::Con =>
        {
            let m = p.start();
            p.bump();
            p.bump();
            p.bump();
            module_path(p);
            p.complete(m, K::ModuleNominalMember);
        }
        K::ClassKw => class_decl(p),
        K::ServiceKw => service_decl(p),
        _ => {
            p.error_expected("a module requirement: `type`, a value signature, `class`, `service`, or `instance ... from name`");
            p.recover();
        }
    }
}

fn variants(p: &mut Parser, refinable: bool) {
    p.eat(K::Pipe);
    variant(p, refinable);
    while p.eat(K::Pipe) {
        variant(p, refinable);
    }
}

/// `Con field…`, `Con = literal`, or `[C =>] Con field… : T index…`, a
/// constructor of a `type` that builds a refined instance of it and may
/// hold dictionaries; a field is an atomic type, so `Some (List a)` needs the
/// parentheses.
fn variant(p: &mut Parser, refinable: bool) {
    let m = p.start();
    if p.at(K::At) {
        attribute(p);
    }
    // `Show b => Showable b : Showable`, `(Eq a, Show a) => …`
    if p.at(K::LParen) || variant_context_ahead(p) {
        if !refinable {
            let at = p.raw();
            p.error_at(
                codes::S015,
                p.span(at.start, at.end),
                "only a constructor of a `type` holds dictionaries".into(),
            );
        }
        let c = p.start();
        types::union(p);
        p.expect(K::FatArrow);
        p.complete(c, K::Context);
    }
    if p.expect(K::Con) {
        if p.at_word("of") {
            let of = p.raw();
            p.error_at(
                codes::S010,
                p.span(of.start, of.end),
                "Flick has no `of` before constructor fields: write `Circle Float`, `Rect Float Float`".into(),
            );
            p.bump();
        }
        while types::at_atom_start(p) {
            types::capture(p);
        }
        // `Get = "GET"`: the literal that is the value of an `untagged type`
        let literal = p.eat(K::Eq);
        if literal {
            let lit = p.start();
            if p.at(K::Minus) && p.nth(1) == K::Number {
                p.bump();
            }
            if matches!(p.current(), K::String | K::Number) {
                p.bump();
            } else {
                p.error_expected("the literal of the constructor, a string or a number");
            }
            p.complete(lit, K::LitPat);
        }
        // `IntLit Int : Expr Int`: the type a constructor of a GADT builds
        if p.at(K::Colon) {
            let colon = p.raw();
            if !refinable || literal {
                let why = if literal {
                    "a constructor with a literal value builds its own type: drop the `:` result"
                } else {
                    "only a constructor of a `type` names the type it builds after `:`"
                };
                p.error_at(codes::S015, p.span(colon.start, colon.end), why.into());
            }
            let result = p.start();
            p.bump();
            types::type_(p);
            p.complete(result, K::VariantResult);
        }
    }
    p.complete(m, K::Variant);
}

/// `@name argument…`: an attribute of the declaration on the next line,
/// or of the constructor after it. Its arguments are lowercase words, so a
/// keyword such as `none` reads as one.
fn attribute(p: &mut Parser) {
    let m = p.start();
    p.bump();
    p.expect(K::Ident);
    while p.at(K::Ident) || p.current().is_keyword() {
        p.bump_as(K::Ident);
    }
    p.complete(m, K::Attribute);
}

fn deriving(p: &mut Parser) {
    if !p.at(K::DerivingKw) {
        return;
    }
    let m = p.start();
    p.bump();
    derived(p);
    while p.eat(K::Comma) {
        derived(p);
    }
    p.complete(m, K::DerivingClause);
}

/// A class of `deriving`, maybe with the arguments of its derive:
/// `Schema { rename = "snake_case" }`.
fn derived(p: &mut Parser) {
    expr::con_path(p);
    if p.at(K::LBrace) {
        let m = p.start();
        super::plugin::raw_braces(p);
        p.complete(m, K::DerivingArgs);
    }
}

/// `deriving instance C (T a)`.
fn deriving_decl(p: &mut Parser) {
    let m = p.start();
    p.bump();
    p.bump();
    derived(p);
    super::types::type_(p);
    p.complete(m, K::DerivingDecl);
}

fn kind_decl(p: &mut Parser) {
    let m = p.start();
    p.bump();
    if p.expect(K::Con) && p.expect(K::Eq) {
        variants(p, false);
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
            variant(p, false);
        }
        deriving(p);
    }
    p.complete(m, K::ErrorDecl);
}

/// `service S params = …` or `effect E params = …`, one node: a service is
/// an effect whose operations are all `tail`.
fn service_decl(p: &mut Parser) {
    let m = p.start();
    if p.at(K::ServiceKw) {
        p.bump();
    } else {
        p.bump_as(K::EffectKw);
    }
    if p.expect(K::Con) {
        type_params(p);
        if p.eat(K::Eq) {
            p.body(K::Members, operation);
        }
    }
    p.complete(m, K::ServiceDecl);
}

/// A member of a service or an effect: a signature may start with the class
/// of its operation.
fn operation(p: &mut Parser) {
    let class = [
        ("tail", K::TailKw),
        ("abort", K::AbortKw),
        ("control", K::ControlKw),
    ]
    .into_iter()
    .find(|(word, _)| p.at_word(word));
    match class {
        Some((_, kind)) if p.nth(1) == K::Ident && p.nth(2) == K::Colon => {
            let m = p.start();
            p.bump_as(kind);
            p.bump();
            p.bump();
            types::type_(p);
            p.complete(m, K::Signature);
        }
        _ => member(p),
    }
}

/// Whether `=>` comes before the end of the constructor it would belong to.
fn variant_context_ahead(p: &Parser) -> bool {
    for i in p.pos..p.toks.len() {
        if p.sep_at(i).is_some() {
            break;
        }
        match p.toks[i].kind {
            K::FatArrow => return true,
            K::Eq | K::Pipe | K::Colon | K::DerivingKw => break,
            _ => {}
        }
    }
    false
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
    // `foreign name from …` has no mode: two words before `from` do
    let mode = (p.at(K::Ident) && p.nth(1) == K::Ident).then(|| p.text_of(p.raw()));
    if mode.is_some_and(|w| !FOREIGN_MODES.contains(&w)) {
        p.error_expected("a foreign mode: `pure`, `effect` or `blocked`, or none");
    }
    if mode.is_some() {
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
