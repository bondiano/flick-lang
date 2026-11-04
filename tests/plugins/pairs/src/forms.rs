//! The heads of `pairs` for each form of a block:
//! - `named Shape`, `named App.Api.place`: the text of the name;
//! - `text_of r#"…"#`: the value of the string, raw text as it is;
//! - `define name { e }`, a declaration: `let name = e`;
//! - `table Point { x : Float = 1.0; … }`: `type alias Point = { x : Float }`
//!   and `let point : Point = { x = 1.0 }`;
//! - `traced let f x = …`: the user's declaration, and `let f_name = "f"`;
//! - `variants Color { Red; Green }`: `type Color = Red | Green deriving Show`;
//! - `shows Box`: `instance Show Box = let show _ = "Box"`;
//! - `capture { e }`: `(fun x -> { held = e; own = x; shown = show
//!   Pairs.version }) "shadow"`, whose `x`, `show` and `Pairs` are not the
//!   user's;
//! - `companion Box { e }`: `let Box.label = e`, a companion of `Box`.

use flick_plugin::{
    Arena, Block, Builder, Case, Compiler, Decl, Diagnostic, Expr, Name, Place, TokenKind, flick,
    string_value,
};

/// The code of `block` when its head is one of these; `None` otherwise.
pub fn expand(
    block: &Block,
    compiler: &mut dyn Compiler,
) -> Option<Result<Arena, Vec<Diagnostic>>> {
    let expression = matches!(block.head.as_str(), "named" | "text_of" | "capture");
    let declaration = matches!(
        block.head.as_str(),
        "define" | "table" | "traced" | "variants" | "shows" | "companion"
    );
    if !expression && !declaration {
        return None;
    }
    let wanted = if expression {
        Place::Expression
    } else {
        Place::Declaration
    };
    if block.place != wanted {
        let message = format!("`{}` stands elsewhere", block.head);
        return Some(Err(vec![Diagnostic::error(message, block.whole())]));
    }
    let mut b = Builder::new(block.whole());
    let made = match block.head.as_str() {
        "named" => Ok(b.expr(Expr::Text(block.text.trim().to_owned()))),
        "text_of" => {
            let raw = block.arguments().first().map(|t| block.text_of(t.span));
            Ok(b.expr(Expr::Text(string_value(raw.unwrap_or("")))))
        }
        "capture" => capture(block, compiler, &mut b),
        "define" => define(block, compiler, &mut b),
        "companion" => companion(block, compiler, &mut b),
        "table" => table(block, compiler, &mut b),
        "traced" => traced(block, compiler, &mut b),
        "variants" => variants(block, &mut b),
        _ => Ok(shows(block, &mut b)),
    };
    Some(made.map(|root| b.finish(root)))
}

/// A generated local around the user's code, and generated names the
/// user's locals could hide.
fn capture(
    block: &Block,
    compiler: &mut dyn Compiler,
    b: &mut Builder,
) -> Result<u32, Vec<Diagnostic>> {
    let held = block.body().expr(compiler).map_err(|d| vec![d])?;
    Ok(flick!(b, (fun x -> { held = #held; own = x; shown = show Pairs.version }) "shadow"))
}

/// `let Owner.label = e`.
fn companion(
    block: &Block,
    compiler: &mut dyn Compiler,
    b: &mut Builder,
) -> Result<u32, Vec<Diagnostic>> {
    let owner = (block.cursor().next())
        .filter(|t| t.kind == TokenKind::Con)
        .ok_or_else(|| vec![Diagnostic::error("expected a type", block.whole())])?;
    let value = block.body().expr(compiler).map_err(|d| vec![d])?;
    let name = format!("{}.label", block.text_of(owner.span));
    Ok(flick!(b @ owner.span, let #(name) = #value))
}

/// `let name = e`.
fn define(
    block: &Block,
    compiler: &mut dyn Compiler,
    b: &mut Builder,
) -> Result<u32, Vec<Diagnostic>> {
    let name = (block.cursor().ident())
        .ok_or_else(|| vec![Diagnostic::error("expected a name", block.whole())])?;
    let value = block.body().expr(compiler).map_err(|d| vec![d])?;
    Ok(flick!(b, let #(block.text_of(name.span)) = #value))
}

/// The user's declaration, and `let f_name = "f"`.
fn traced(
    block: &Block,
    compiler: &mut dyn Compiler,
    b: &mut Builder,
) -> Result<u32, Vec<Diagnostic>> {
    let user = block.cursor().decl(compiler).map_err(|d| vec![d])?;
    let name = (block.tokens.iter())
        .find(|t| t.kind == TokenKind::Ident)
        .map(|t| block.text_of(t.span))
        .ok_or_else(|| vec![Diagnostic::error("expected a name", block.whole())])?;
    let kept = b.decl(Decl::Hole(user));
    let traced = flick!(b, let #(format!("{name}_name")) = #name);
    Ok(b.group(vec![kept, traced]))
}

/// `type Color = Red | Green deriving Show`.
fn variants(block: &Block, b: &mut Builder) -> Result<u32, Vec<Diagnostic>> {
    let name = (block.cursor().next())
        .filter(|t| t.kind == TokenKind::Con)
        .map(|t| block.text_of(t.span).to_owned())
        .ok_or_else(|| vec![Diagnostic::error("expected the type's name", block.whole())])?;
    let cases = (block.body())
        .filter(|t| t.kind == TokenKind::Con)
        .map(|t| Case {
            name: block.text_of(t.span).to_owned(),
            fields: Vec::new(),
        })
        .collect();
    Ok(b.decl(Decl::Adt {
        name,
        params: Vec::new(),
        cases,
        deriving: vec!["Show".into()],
    }))
}

/// `instance Show Box = let show _ = "Box"`.
fn shows(block: &Block, b: &mut Builder) -> u32 {
    let name = block.text.trim();
    flick!(b, instance Show #(Name(name)) { let show _ = #name })
}

/// `type alias Point = { x : Float; … }`, with the block's documentation,
/// and `let point = { x = 1.0; … }`, both declared at the name `Point` of
/// the block.
fn table(
    block: &Block,
    compiler: &mut dyn Compiler,
    b: &mut Builder,
) -> Result<u32, Vec<Diagnostic>> {
    let token = (block.arguments().first())
        .filter(|t| t.kind == TokenKind::Con)
        .ok_or_else(|| {
            vec![Diagnostic::error(
                "expected the table's name",
                block.whole(),
            )]
        })?;
    let (at, name) = (token.span, block.text_of(token.span).to_owned());
    let mut c = block.body();
    let (mut types, mut values) = (Vec::new(), Vec::new());
    while let Some(field) = c.ident() {
        let label = c.text(field).to_owned();
        let at = |what: &str| vec![Diagnostic::error(what, field.span)];
        c.eat(":").ok_or_else(|| at("expected `:`"))?;
        let ty = c.ty(compiler).map_err(|d| vec![d])?;
        c.eat("=").ok_or_else(|| at("expected `=`"))?;
        let value = c.expr(compiler).map_err(|d| vec![d])?;
        c.eat(";");
        types.push((label.clone(), ty));
        values.push((label, value));
    }
    let alias = flick!(b @ at, type alias #(name) = { #(..types) });
    let alias = b.doc(&block.docs, alias);
    let mut lower = name.clone();
    lower[..1].make_ascii_lowercase();
    let value = flick!(b @ at, let #(lower) : #(Name(&name)) = { #(..values) });
    Ok(b.group(vec![alias, value]))
}
