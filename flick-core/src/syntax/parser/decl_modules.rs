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
    while p.at(K::LParen) {
        let group = p.start();
        p.delimited(K::RParen, module_param);
        p.complete(group, K::ModuleParamGroup);
    }
    if p.eat(K::Colon) {
        let result = p.start();
        module_signature(p);
        p.complete(result, K::ModuleResult);
    }
    if p.at(K::ExposingKw) {
        exposing(p);
    }
    if p.eat(K::Eq) {
        if at_module_expr(p) {
            module_expr(p);
            p.complete(m, K::ModuleApplyDecl);
        } else {
            p.body(K::Members, top_decl);
            p.complete(m, K::InlineModuleDecl);
        }
    } else {
        p.complete(m, K::ModuleHeader);
    }
}

fn module_param(p: &mut Parser) {
    let m = p.start();
    if p.expect(K::Con) && p.expect(K::Colon) {
        module_signature(p);
    }
    p.complete(m, K::ModuleParam);
}

fn module_apply_decl(p: &mut Parser) {
    let m = p.start();
    p.bump();
    p.bump();
    if p.expect(K::Eq) {
        module_expr(p);
    }
    p.complete(m, K::ModuleApplyDecl);
}

fn at_module_expr(p: &Parser) -> bool {
    matches!(p.current(), K::Con | K::LParen) || p.at_word("functor")
}

fn module_expr(p: &mut Parser) -> Option<Done> {
    if !p.enter() {
        return None;
    }
    let mut lhs = if p.at_word("functor") {
        let m = p.start();
        p.bump();
        while p.at(K::LParen) {
            let group = p.start();
            p.delimited(K::RParen, module_param);
            p.complete(group, K::ModuleParamGroup);
        }
        if p.expect(K::Arrow) {
            if at_module_expr(p) {
                module_expr(p);
            } else {
                p.body(K::Members, top_decl);
            }
        }
        p.complete(m, K::ModuleFunctorExpr)
    } else if p.at(K::LParen) {
        let m = p.start();
        p.bump();
        p.push(Ctx::Bracket);
        module_expr(p);
        let annotated = p.eat(K::Colon);
        if annotated {
            module_signature(p);
        }
        p.pop();
        p.expect(K::RParen);
        p.complete(
            m,
            if annotated {
                K::ModuleAnnotExpr
            } else {
                K::ModuleParenExpr
            },
        )
    } else {
        let m = p.start();
        if p.at(K::Con) {
            module_path(p);
        } else {
            p.error_expected("a named module, not a `Module Sig` value");
            p.recover();
        }
        // Keep paths directly visible in the expression tree.
        p.complete(m, K::ModuleParenExpr)
    };
    while p.at(K::LParen) {
        let m = p.precede(lhs);
        p.delimited(K::RParen, |p| {
            module_expr(p);
        });
        lhs = p.complete(m, K::ModuleApplyExpr);
    }
    p.leave();
    Some(lhs)
}

fn module_signature(p: &mut Parser) -> Option<Done> {
    if !p.enter() {
        return None;
    }
    let lhs = if p.at(K::LParen) {
        let m = p.start();
        p.bump();
        p.push(Ctx::Bracket);
        if p.at(K::Con) && p.nth(1) == K::Colon {
            loop {
                let binder = p.start();
                p.expect(K::Con);
                p.expect(K::Colon);
                module_signature(p);
                p.complete(binder, K::ModuleSignatureBinder);
                if !p.eat(K::Comma) { break; }
            }
        } else {
            module_signature(p);
        }
        p.pop();
        p.expect(K::RParen);
        p.complete(m, K::ModuleSignatureParen)
    } else {
        let m = p.start();
        if p.at(K::Con) {
            expr::con_path(p);
        } else {
            p.error_expected("a module signature");
            p.recover();
        }
        p.complete(m, K::NameType)
    };
    let mut lhs = lhs;
    if p.eat(K::WithKw) {
        let m = p.precede(lhs);
        loop {
            let equality = p.start();
            p.expect(K::TypeKw);
            module_path(p);
            if p.expect(K::Eq) {
                types::type_(p);
            }
            p.complete(equality, K::ModuleTypeEquality);
            if !p.at_word("and") {
                break;
            }
            p.bump();
        }
        lhs = p.complete(m, K::ModuleSignatureRefinement);
    }
    if p.eat(K::Arrow) {
        let m = p.precede(lhs);
        module_signature(p);
        lhs = p.complete(m, K::ModuleSignatureArrow);
    }
    p.leave();
    Some(lhs)
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
