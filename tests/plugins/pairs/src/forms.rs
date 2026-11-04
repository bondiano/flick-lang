//! The heads of `pairs` for each form of a block:
//! - `named Shape`, `named App.Api.place`: the text of the name;
//! - `text_of r#"…"#`: the value of the string, raw text as it is;
//! - `define name { e }`, a declaration: `let name = e`;
//! - `table Point { x : Float = 1.0; … }`: `type alias Point = { x : Float }`
//!   and `let point : Point = { x = 1.0 }`;
//! - `traced let f x = …`: the user's declaration, and `let f_name = "f"`;
//! - `variants Color { Red; Green }`: `type Color = Red | Green deriving Show`;
//! - `shows Box`: `instance Show Box = let show _ = "Box"`.

use flick_plugin::{
    Arena, Block, Builder, Case, Compiler, Decl, Diagnostic, Expr, Field, Pat, Place, TokenKind,
    Ty, string_value,
};

/// The code of `block` when its head is one of these; `None` otherwise.
pub fn expand(
    block: &Block,
    compiler: &mut dyn Compiler,
) -> Option<Result<Arena, Vec<Diagnostic>>> {
    let expression = matches!(block.head.as_str(), "named" | "text_of");
    let declaration = matches!(
        block.head.as_str(),
        "define" | "table" | "traced" | "variants" | "shows"
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
        "define" => define(block, compiler, &mut b),
        "table" => table(block, compiler, &mut b),
        "traced" => traced(block, compiler, &mut b),
        "variants" => Ok(variants(block, &mut b)),
        _ => Ok(shows(block, &mut b)),
    };
    Some(made.map(|root| b.finish(root)))
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
    let value = b.expr(Expr::Hole(value));
    Ok(b.decl(Decl::Let {
        name: block.text_of(name.span).to_owned(),
        params: Vec::new(),
        ty: None,
        body: value,
    }))
}

/// The user's declaration, and `let f_name = "f"`.
fn traced(
    block: &Block,
    compiler: &mut dyn Compiler,
    b: &mut Builder,
) -> Result<u32, Vec<Diagnostic>> {
    let user = block.cursor().decl(compiler).map_err(|d| vec![d])?;
    let name = (block.tokens.iter())
        .filter(|t| t.kind == TokenKind::Ident)
        .map(|t| block.text_of(t.span))
        .next()
        .unwrap_or("it");
    let kept = b.decl(Decl::Hole(user));
    let text = b.expr(Expr::Text(name.to_owned()));
    let traced = b.decl(Decl::Let {
        name: format!("{name}_name"),
        params: Vec::new(),
        ty: None,
        body: text,
    });
    Ok(b.group(vec![kept, traced]))
}

/// `type Color = Red | Green deriving Show`.
fn variants(block: &Block, b: &mut Builder) -> u32 {
    let name = block
        .cursor()
        .next()
        .map(|t| block.text_of(t.span).to_owned());
    let cases = (block.body())
        .filter(|t| t.kind == TokenKind::Con)
        .map(|t| Case {
            name: block.text_of(t.span).to_owned(),
            fields: Vec::new(),
        })
        .collect();
    b.decl(Decl::Adt {
        name: name.unwrap_or_default(),
        params: Vec::new(),
        cases,
        deriving: vec!["Show".into()],
    })
}

/// `instance Show Box = let show _ = "Box"`.
fn shows(block: &Block, b: &mut Builder) -> u32 {
    let name = block.text.trim().to_owned();
    let class = b.ty(Ty::Name("Show".into()));
    let shown = b.ty(Ty::Name(name.clone()));
    let head = b.ty(Ty::Apply {
        function: class,
        arguments: vec![shown],
    });
    let ignored = b.pat(Pat::Wildcard);
    let text = b.expr(Expr::Text(name));
    let show = b.decl(Decl::Let {
        name: "show".into(),
        params: vec![ignored],
        ty: None,
        body: text,
    });
    b.decl(Decl::Instance {
        context: Vec::new(),
        head,
        members: vec![show],
    })
}

/// `type alias Point = { x : Float; … }` and `let point = { x = 1.0; … }`.
fn table(
    block: &Block,
    compiler: &mut dyn Compiler,
    b: &mut Builder,
) -> Result<u32, Vec<Diagnostic>> {
    let name = (block.arguments().first())
        .filter(|t| t.kind == TokenKind::Con)
        .map(|t| block.text_of(t.span).to_owned())
        .ok_or_else(|| {
            vec![Diagnostic::error(
                "expected the table's name",
                block.whole(),
            )]
        })?;
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
        types.push(Field {
            label: label.clone(),
            value: b.ty(Ty::Hole(ty)),
        });
        values.push(Field {
            label,
            value: b.expr(Expr::Hole(value)),
        });
    }
    let record = b.ty(Ty::Record(types));
    let alias = b.decl(Decl::Alias {
        name: name.clone(),
        params: Vec::new(),
        ty: record,
    });
    let row = b.expr(Expr::Record(values));
    let annotation = b.ty(Ty::Name(name.clone()));
    let mut lower = name;
    lower[..1].make_ascii_lowercase();
    let value = b.decl(Decl::Let {
        name: lower,
        params: Vec::new(),
        ty: Some(annotation),
        body: row,
    });
    Ok(b.group(vec![alias, value]))
}
