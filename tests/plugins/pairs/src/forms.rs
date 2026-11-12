//! The heads of `pairs` for each form of a block:
//! - `named Shape`, `named App.Api.place`: the text of the name;
//! - `text_of r#"…"#`: the value of the string, raw text as it is;
//! - `define name { e }`, a declaration: `let name = e`;
//! - `table Point { x : Float = 1.0; … }`: `type alias Point = { x : Float }`
//!   and `let point : Point = { x = 1.0 }`;
//! - `traced let f x = …`: the user's declaration, and `let f_name = "f"`;
//! - `variants Color { Red; Green }`: `type Color = Red | Green deriving Show`;
//! - `shows Box`: `instance Show Box = let show _ = "Box"`;
//! - `wrapped Meters`: `unboxed type Meters = Meters$Make Float`, a type of
//!   its own whose values are `Float`s when the program runs, with
//!   `Meters.make` and `Meters.value`, the only ways in and out;
//!   `wrapped Meters = T deriving C` wraps the user's `T` and names classes;
//! - `forged Meters`: `Meters$Make 1.0`, the constructor of a wrapped type
//!   named outside the code that declared it;
//! - `capture { e }`: `(fun x -> { held = e; own = x; shown = show
//!   Pairs.version }) "shadow"`, whose `x`, `show` and `Pairs` are not the
//!   user's;
//! - `companion Box { e }`: `let Box.label = e`, a companion of `Box`;
//! - `signed name { e }` below `name : T`: `name : T`, `let name = e`, and
//!   `let name_module = "App.Main"`, the module the block is in;
//! - `typed { e }`: `(e : _)`, the `_` at `e`, whose type `check` reports;
//!   `linted { e }` the same, whose `check` warns with a fix;
//! - `twice p { e }`: a function of `p` whose block keeps `e` and gives a
//!   function of `p` again, a `match` and an `if` in it.

use flick_plugin::{
    Arena, Arm, Block, Builder, Case, Compiler, Decl, Diagnostic, Expr, Name, Pat, Place, Span,
    TokenKind, Ty, flick, string_value,
};

/// The code of `block` when its head is one of these; `None` otherwise.
pub fn expand(
    block: &Block,
    compiler: &mut dyn Compiler,
) -> Option<Result<Arena, Vec<Diagnostic>>> {
    let expression = matches!(
        block.head.as_str(),
        "named"
            | "text_of"
            | "capture"
            | "typed"
            | "linted"
            | "twice"
            | "opens"
            | "forged"
            | "runs"
    );
    let declaration = matches!(
        block.head.as_str(),
        "define" | "table" | "traced" | "variants" | "shows" | "companion" | "signed" | "wrapped"
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
        "twice" => twice(block, compiler, &mut b),
        "typed" | "linted" => typed(block, compiler, &mut b),
        "opens" | "runs" => opens(block, compiler, &mut b),
        "forged" => {
            let make = format!("{}$Make", block.text.trim());
            Ok(flick!(b, #(Name(&make)) 1.0))
        }
        "define" => define(block, compiler, &mut b),
        "companion" => companion(block, compiler, &mut b),
        "table" => table(block, compiler, &mut b),
        "traced" => traced(block, compiler, &mut b),
        "signed" => signed(block, compiler, &mut b),
        "variants" => variants(block, &mut b),
        "wrapped" => wrapped(block, compiler, &mut b),
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

/// `twice p { e }`: a function of `p` whose block keeps `e` and gives a
/// function of `p` again, as the setup and the render of a component bind
/// its props: `fun p -> let seen = e; fun p -> match seen with | x when (if
/// true then true else false) -> x | _ -> 0.0`.
fn twice(
    block: &Block,
    compiler: &mut dyn Compiler,
    b: &mut Builder,
) -> Result<u32, Vec<Diagnostic>> {
    let props = block.cursor().pat(compiler).map_err(|d| vec![d])?;
    let value = block.body().expr(compiler).map_err(|d| vec![d])?;
    let outer = b.pat(Pat::Hole(props));
    let body = b.expr(Expr::Hole(value));
    let seen = b.decl(Decl::Let {
        name: "seen".into(),
        params: Vec::new(),
        ty: None,
        body,
        rec: false,
    });
    let inner = b.pat(Pat::Hole(props));
    let scrutinee = b.expr(Expr::Name("seen".into()));
    let x = b.pat(Pat::Var("x".into()));
    let (yes, no) = (b.expr(Expr::Boolean(true)), b.expr(Expr::Boolean(false)));
    let condition = b.expr(Expr::Boolean(true));
    let guard = b.expr(Expr::If {
        condition,
        then: yes,
        otherwise: no,
    });
    let found = b.expr(Expr::Name("x".into()));
    let other = b.pat(Pat::Wildcard);
    let zero = b.expr(Expr::Number(0.0));
    let arms = vec![
        Arm {
            pattern: x,
            guard: Some(guard),
            body: found,
        },
        Arm {
            pattern: other,
            guard: None,
            body: zero,
        },
    ];
    let chosen = b.expr(Expr::Match { scrutinee, arms });
    let render = b.expr(Expr::Lambda {
        params: vec![inner],
        body: chosen,
    });
    let setup = b.expr(Expr::Block {
        statements: vec![seen],
        result: render,
    });
    Ok(b.expr(Expr::Lambda {
        params: vec![outer],
        body: setup,
    }))
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

/// `name : T` from the signature above, `let name = e`, and
/// `let name_module = "…"`.
fn signed(
    block: &Block,
    compiler: &mut dyn Compiler,
    b: &mut Builder,
) -> Result<u32, Vec<Diagnostic>> {
    let name = (block.cursor().ident())
        .map(|t| block.text_of(t.span).to_owned())
        .ok_or_else(|| vec![Diagnostic::error("expected a name", block.whole())])?;
    if block.signature.is_none() {
        let message = format!("`{name}` needs a signature right above it");
        return Err(vec![Diagnostic::error(message, block.whole())]);
    }
    let value = block.body().expr(compiler).map_err(|d| vec![d])?;
    // the type, after `name :`
    let signature = block.signature.as_ref().expect("checked above");
    let start = signature.tokens.get(2).map_or(0, |t| t.span.start);
    let end = u32::try_from(signature.text.len()).unwrap_or(u32::MAX);
    let ty = b.ty(Ty::Signed(Span { start, end }));
    let signature = b.decl(Decl::Signature {
        name: name.clone(),
        ty,
    });
    let made = flick!(b, let #(name.clone()) = #value);
    let module = b.expr(Expr::Text(block.module.clone()));
    let module = flick!(b, let #(format!("{name}_module")) = #module);
    Ok(b.group(vec![signature, made, module]))
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
        unboxed: false,
    }))
}

/// `unboxed type Meters = Meters$Make Float` and its two companions. The
/// constructor is a name only generated code can write. After `=` the
/// field is the user's type, and the classes after `deriving` are the type's.
fn wrapped(
    block: &Block,
    compiler: &mut dyn Compiler,
    b: &mut Builder,
) -> Result<u32, Vec<Diagnostic>> {
    let mut cursor = block.cursor();
    let name = (cursor.next())
        .filter(|t| t.kind == TokenKind::Con)
        .map(|t| block.text_of(t.span).to_owned())
        .ok_or_else(|| vec![Diagnostic::error("expected the type's name", block.whole())])?;
    let name = name.as_str();
    let make = format!("{name}$Make");
    let (base, deriving) = if cursor.eat("=").is_some() {
        let base = cursor.ty(compiler).map_err(|d| vec![d])?;
        let classes: String = (cursor.rest().iter())
            .map(|t| block.text_of(t.span))
            .skip_while(|t| *t != "deriving")
            .skip(1)
            .collect();
        let classes = classes.split(',').filter(|c| !c.is_empty());
        (b.ty(Ty::Hole(base)), classes.map(str::to_owned).collect())
    } else {
        (flick!(b, type Float), Vec::new())
    };
    let ty = b.decl(Decl::Adt {
        name: name.to_owned(),
        params: Vec::new(),
        cases: vec![Case {
            name: make.clone(),
            fields: vec![base],
        }],
        deriving,
        unboxed: true,
    });
    let wrap = flick!(b, let #(format!("{name}.make")) value = #(Name(&make)) value);
    let field = b.pat(Pat::Var("value".into()));
    let taken = b.pat(Pat::Con {
        name: make,
        arguments: vec![field],
    });
    let body = flick!(b, value);
    let unwrap = b.decl(Decl::Let {
        name: format!("{name}.value"),
        params: vec![taken],
        ty: None,
        body,
        rec: false,
    });
    Ok(b.group(vec![ty, wrap, unwrap]))
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

/// The user's code with the module `Pairs` open: its names need no import.
fn opens(
    block: &Block,
    compiler: &mut dyn Compiler,
    b: &mut Builder,
) -> Result<u32, Vec<Diagnostic>> {
    let e = block.body().expr(compiler).map_err(|d| vec![d])?;
    let body = b.at(e).expr(Expr::Hole(e));
    // `runs { e }` applies a value of the type `Pairs.run` takes through it
    let through = (block.head == "runs").then(|| "Pairs.run".to_owned());
    Ok(b.at(block.whole()).expr(Expr::Open {
        module: "Pairs".into(),
        child: None,
        through,
        body,
    }))
}

/// `(e : _)`, the `_` at `e`.
fn typed(
    block: &Block,
    compiler: &mut dyn Compiler,
    b: &mut Builder,
) -> Result<u32, Vec<Diagnostic>> {
    let e = block.body().expr(compiler).map_err(|d| vec![d])?;
    let wildcard = b.at(e).ty(Ty::Wildcard);
    let expr = b.at(block.whole()).expr(Expr::Hole(e));
    Ok(b.expr(Expr::Annot { expr, ty: wildcard }))
}
