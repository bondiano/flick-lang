pub(super) fn module_path(p: &mut Parser) {
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
    if p.at_word("private") {
        p.bump();
    }
    p.bump();
    module_path(p);
    if p.at(K::LParen) {
        p.delimited(K::RParen, functor_param);
    }
    if p.at(K::ExposingKw) {
        exposing(p);
    }
    if p.eat(K::Eq) {
        p.body(K::Members, top_decl);
        p.complete(m, K::InlineModuleDecl);
    } else {
        p.complete(m, K::ModuleHeader);
    }
}

fn functor_param(p: &mut Parser) {
    let m = p.start();
    if p.expect(K::Con) && p.expect(K::Colon) {
        types::type_(p);
    }
    p.complete(m, K::FunctorParam);
}

fn module_apply_decl(p: &mut Parser) {
    let m = p.start();
    p.bump();
    p.bump();
    if p.expect(K::Eq) {
        module_path(p);
        if p.at(K::LParen) {
            p.delimited(K::RParen, static_module_argument);
        } else {
            p.error_expected("`(` with static module arguments");
        }
    }
    p.complete(m, K::ModuleApplyDecl);
}

fn static_module_argument(p: &mut Parser) {
    if p.at(K::Con) {
        module_path(p);
    } else {
        p.error_expected("a named module, not a `Module Sig` value");
        p.recover();
    }
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

/// `name`, `Type`, `Type(..)`, `..` or `plugin head`.
fn expose_item(p: &mut Parser) {
    let m = p.start();
    match p.current() {
        K::Ident if super::plugin::at_head(p) => super::plugin::head(p),
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
