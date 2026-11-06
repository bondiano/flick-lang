//! Types, rows, capture qualifiers and kinds (kinds share the type grammar).

use super::{Ctx, Done, K, Parser, expr};

/// `forall a b. T`, `C => T` or an arrow type.
pub(super) fn type_(p: &mut Parser) -> Option<Done> {
    if !p.enter() {
        return None;
    }
    let done = if p.at(K::ForallKw) {
        let m = p.start();
        p.bump();
        if !p.at(K::Ident) {
            p.error_expected("a type variable");
        }
        while p.eat(K::Ident) {}
        if p.expect(K::Dot) {
            type_(p);
        }
        Some(p.complete(m, K::ForallType))
    } else {
        arrow(p).map(|lhs| {
            if p.at(K::FatArrow) {
                let m = p.precede(lhs);
                p.bump();
                type_(p);
                p.complete(m, K::ConstrainedType)
            } else {
                lhs
            }
        })
    };
    p.leave();
    done
}

/// `A -> B`, `A ->{r; e} B`, `A ->{} B`; right associative.
fn arrow(p: &mut Parser) -> Option<Done> {
    let lhs = union(p)?;
    if !p.at(K::Arrow) {
        return Some(lhs);
    }
    let m = p.precede(lhs);
    p.bump();
    // `->{` written without a space is the effect annotation; `-> {…}` is a type
    if p.raw().kind == K::LBrace && p.adjacent() {
        effect_annot(p);
    }
    type_(p);
    Some(p.complete(m, K::ArrowType))
}

fn effect_annot(p: &mut Parser) {
    let m = p.start();
    let opener = p.raw();
    p.bump();
    p.push(Ctx::Bracket);
    if p.raw().kind != K::RBrace {
        row(p);
        if p.expect(K::Semi) {
            row(p);
        }
    }
    p.pop();
    p.expect_closer(K::RBrace, opener);
    p.complete(m, K::EffectAnnot);
}

/// One side of an effect annotation: `none`, `r`, `A, B | r`, `r1 + r2`, or
/// nothing for an empty row, as in `->{; E}`.
fn row(p: &mut Parser) {
    let m = p.start();
    if !matches!(p.current(), K::Semi | K::RBrace) {
        row_items(p);
    }
    p.complete(m, K::Row);
}

fn row_items(p: &mut Parser) {
    if matches!(p.current(), K::Semi | K::RBrace | K::Pipe) {
        p.error_expected("a row (write `none` for an empty row)");
    } else {
        union(p);
        while p.eat(K::Comma) {
            union(p);
        }
    }
    if p.at(K::Pipe) {
        tail(p);
    }
}

fn tail(p: &mut Parser) {
    let m = p.start();
    p.bump();
    union(p);
    p.complete(m, K::RowTail);
}

/// `r1 + r2`: union of rows or capture sets.
pub(super) fn union(p: &mut Parser) -> Option<Done> {
    let mut lhs = app(p)?;
    while p.at(K::Plus) {
        let m = p.precede(lhs);
        p.bump();
        app(p);
        lhs = p.complete(m, K::UnionType);
    }
    Some(lhs)
}

pub(super) fn at_atom_start(p: &Parser) -> bool {
    matches!(
        p.current(),
        K::Ident | K::Con | K::NoneKw | K::String | K::LParen | K::LBrace
    )
}

fn app(p: &mut Parser) -> Option<Done> {
    let head = capture(p)?;
    // `List _`: a hole is an argument too, though not a constructor field
    let at_arg = |p: &Parser| at_atom_start(p) || p.at(K::Underscore);
    if !at_arg(p) {
        return Some(head);
    }
    let m = p.precede(head);
    while at_arg(p) {
        capture(p);
    }
    Some(p.complete(m, K::AppType))
}

/// `T^c`, `T^{s}`, `T^(c + d)`.
pub(super) fn capture(p: &mut Parser) -> Option<Done> {
    let mut base = atom(p)?;
    while p.at(K::Caret) {
        let m = p.precede(base);
        p.bump();
        match p.current() {
            K::Ident | K::LParen | K::LBrace => {
                atom(p);
            }
            _ => p.error_expected("a capture set after `^`"),
        }
        base = p.complete(m, K::CaptureType);
    }
    Some(base)
}

fn atom(p: &mut Parser) -> Option<Done> {
    if !p.enter() {
        return None;
    }
    let m = p.start();
    let kind = match p.current() {
        // a string is a label at the type level
        K::Ident | K::NoneKw | K::String => {
            p.bump();
            K::NameType
        }
        K::Con => {
            expr::con_path(p);
            K::NameType
        }
        K::LParen => {
            let opener = p.raw();
            p.bump();
            if p.raw().kind == K::RParen {
                p.bump();
                K::UnitType
            } else {
                p.push(Ctx::Bracket);
                type_(p);
                let kind = if p.at(K::Comma) {
                    while p.eat(K::Comma) && !p.at(K::RParen) {
                        type_(p);
                    }
                    K::TupleType
                } else {
                    K::ParenType
                };
                p.pop();
                p.expect_closer(K::RParen, opener);
                kind
            }
        }
        K::LBrace => brace_type(p),
        K::Underscore => {
            p.bump();
            K::HoleType
        }
        _ => {
            p.error_expected("a type");
            K::Error
        }
    };
    p.leave();
    Some(p.complete(m, kind))
}

/// `{ f : T; … | r }` record type, or `{A, B | r}` row.
fn brace_type(p: &mut Parser) -> K {
    let opener = p.raw();
    p.bump();
    let first = p.raw();
    let next = p.toks[(p.pos + 1).min(p.toks.len() - 1)];
    if first.kind == K::RBrace {
        p.bump();
        return K::RowType;
    }
    let record = first.kind == K::Ident && next.kind == K::Colon;
    if record {
        p.push(Ctx::Aligned { col: p.col() });
        p.items(record_type_field, true, |p| p.at(K::Pipe));
    } else {
        p.push(Ctx::Bracket);
        row_items(p);
    }
    p.pop();
    p.expect_closer(K::RBrace, opener);
    if record { K::RecordType } else { K::RowType }
}

fn record_type_field(p: &mut Parser) {
    if p.at(K::Pipe) {
        return tail(p);
    }
    let m = p.start();
    if p.expect(K::Ident) && p.expect(K::Colon) {
        type_(p);
    }
    p.complete(m, K::RecordTypeField);
}
