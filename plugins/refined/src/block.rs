//! `refined type T a = Base where predicate deriving C, …`: a type of its
//! own over `Base`, whose values are those the predicate holds for.
//!
//! ```text
//! unboxed type T a = T$Make Base deriving C, …
//! T.check : Base -> Bool
//! let T.check = predicate
//! T.parse : Base -> Result SchemaError (T a)
//! let T.parse value = if T.check value then Ok (T$Make value) else Err (…)
//! T.value : T a -> Base
//! let T.value (T$Make value) = value
//! ```
//!
//! The constructor is a name only generated code writes, and only this
//! plugin's: `T.parse` is the one way in. `Show` is written here over the
//! base, as a value shows the way its base does.

use std::mem;

use flick_plugin::{
    Arena, Block, Builder, Case, Compiler, Cursor, Decl, Diagnostic, Expr, Field, Hole, HoleKind,
    Hover, Name, NodeId, Pat, Piece, Span, Token, TokenKind, Ty, flick,
};

/// What an editor shows on the head.
const ABOUT: &str = "A type of its own over another, whose values pass a check: `T.parse` makes one, `T.value` reads it.";

/// The parts of the declaration, at spans of the block.
struct Parts {
    name: Token,
    params: Vec<Token>,
    base: Span,
    predicate: Span,
    /// The classes after `deriving`, as written, and where the word stands.
    classes: Vec<String>,
    deriving: Option<Span>,
}

fn read(block: &Block, compiler: &mut dyn Compiler) -> Result<Parts, Vec<Diagnostic>> {
    let refuse = |message: &str, at: Span| vec![Diagnostic::error(message, at)];
    let mut cursor = block.cursor();
    // where the declaration stops short: the next token, or the last one
    let here = |cursor: &Cursor| {
        let last = block.tokens.last().map_or(block.whole(), |t| t.span);
        cursor.peek().map_or(last, |t| t.span)
    };
    if cursor.eat("type").is_none() {
        return Err(refuse("expected `type` after `refined`", here(&cursor)));
    }
    let name = (cursor.peek())
        .filter(|t| t.kind == TokenKind::Con)
        .ok_or_else(|| refuse("expected the name of the type", here(&cursor)))?;
    cursor.next();
    let mut params = Vec::new();
    while let Some(param) = cursor.ident() {
        params.push(param);
    }
    if cursor.eat("=").is_none() {
        return Err(refuse("expected `=` and the base type", here(&cursor)));
    }
    let base = cursor.ty(compiler).map_err(|d| vec![d])?;
    if cursor.eat("where").is_none() {
        return Err(refuse(
            "expected `where` and the check of the type",
            here(&cursor),
        ));
    }
    let predicate = cursor.expr(compiler).map_err(|d| vec![d])?;
    let deriving = cursor.eat("deriving").map(|t| t.span);
    let mut classes = Vec::new();
    if deriving.is_some() {
        let mut class = String::new();
        for token in cursor.rest() {
            match block.text_of(token.span) {
                "," => classes.push(mem::take(&mut class)),
                // shortcut: a class of a refined type takes no arguments; pass them on when a derive needs them
                "{" => {
                    return Err(refuse(
                        "a class of a refined type takes no arguments",
                        token.span,
                    ));
                }
                part => class.push_str(part),
            }
        }
        classes.push(class);
        if classes.iter().any(String::is_empty) {
            return Err(refuse("expected a class after `deriving`", here(&cursor)));
        }
    } else if !cursor.is_done() {
        return Err(refuse("expected the end of the declaration", here(&cursor)));
    }
    Ok(Parts {
        name,
        params,
        base,
        predicate,
        classes,
        deriving,
    })
}

/// The type the block declares, as its code names it.
struct Code<'a> {
    name: &'a str,
    params: Vec<String>,
    /// The constructor, a name of generated code alone.
    make: String,
    base: Span,
}

impl Code<'_> {
    fn companion(&self, member: &str) -> String {
        format!("{}.{member}", self.name)
    }

    /// `T a`.
    fn own(&self, b: &mut Builder) -> NodeId {
        let head = b.ty(Ty::Name(self.name.to_owned()));
        if self.params.is_empty() {
            return head;
        }
        let arguments = (self.params.iter())
            .map(|p| b.ty(Ty::Name(p.clone())))
            .collect();
        b.ty(Ty::Apply {
            function: head,
            arguments,
        })
    }

    /// The base, the user's type.
    fn base(&self, b: &mut Builder) -> NodeId {
        b.at(self.base).ty(Ty::Hole(self.base))
    }

    /// `T.check : Base -> Bool` and the user's predicate, which has to be
    /// one. The other companions have the types their code gives them.
    fn check(&self, b: &mut Builder, predicate: Span) -> [NodeId; 2] {
        let (input, output) = (self.base(b), flick!(b, type Bool));
        let ty = b.ty(Ty::Function { input, output });
        let name = self.companion("check");
        let signature = b.decl(Decl::Signature {
            name: name.clone(),
            ty,
        });
        let held = flick!(b @ predicate, let #(name) = #predicate);
        let about = "Whether a value passes the check of the type.";
        [signature, b.doc(about, held)]
    }

    /// `T.parse`: the value as a `T` when its check holds, the error a
    /// schema gives at the root otherwise.
    fn parse(&self, b: &mut Builder) -> NodeId {
        let condition = flick!(b, #(Name(&self.companion("check"))) value);
        let then = flick!(b, Ok (#(Name(&self.make)) value));
        let field = |b: &mut Builder, label: &str, text: String| Field {
            label: label.into(),
            value: b.expr(Expr::Text(text)),
        };
        let fields = vec![
            field(b, "message", format!("not a valid {}", self.name)),
            field(b, "path", "(root)".into()),
        ];
        let issue = b.expr(Expr::Record(fields));
        let otherwise = flick!(b, Err (SchemaError [#issue]));
        let body = b.expr(Expr::If {
            condition,
            then,
            otherwise,
        });
        let value = b.pat(Pat::Var("value".into()));
        let about = "Checks a value: the value as one of the type, or why it is none.";
        self.defined(b, "parse", value, body, about)
    }

    /// `T.value`: what the constructor holds.
    fn value(&self, b: &mut Builder) -> NodeId {
        let inner = b.pat(Pat::Var("value".into()));
        let taken = b.pat(Pat::Con {
            name: self.make.clone(),
            arguments: vec![inner],
        });
        let body = flick!(b, value);
        self.defined(
            b,
            "value",
            taken,
            body,
            "The value a value of the type holds.",
        )
    }

    /// `let T.member param = body`, documented.
    fn defined(
        &self,
        b: &mut Builder,
        member: &str,
        param: NodeId,
        body: NodeId,
        about: &str,
    ) -> NodeId {
        let held = b.decl(Decl::Let {
            name: self.companion(member),
            params: vec![param],
            ty: None,
            body,
            rec: false,
        });
        b.doc(about, held)
    }
}

/// The declarations of the block.
pub fn expand(block: &Block, compiler: &mut dyn Compiler) -> Result<Arena, Vec<Diagnostic>> {
    let parts = read(block, compiler)?;
    let name = block.text_of(parts.name.span);
    let code = Code {
        name,
        params: (parts.params.iter())
            .map(|p| block.text_of(p.span).to_owned())
            .collect(),
        make: format!("{name}$Make"),
        base: parts.base,
    };
    let mut b = Builder::new(block.whole());
    // `Show` is over the base; the others are the type's own, structural
    // over its one field or derived by their plugins
    let mut deriving = parts.classes;
    let shows = deriving.iter().any(|c| c == "Show");
    deriving.retain(|c| c != "Show");
    let field = code.base(&mut b);
    let ty = b.at(parts.name.span).decl(Decl::Adt {
        name: name.to_owned(),
        params: code.params.clone(),
        cases: vec![Case {
            name: code.make.clone(),
            fields: vec![field],
        }],
        deriving,
        unboxed: true,
    });
    let ty = if block.docs.is_empty() {
        ty
    } else {
        b.doc(&block.docs, ty)
    };
    let mut decls = vec![ty];
    let [signature, check] = code.check(b.at(block.whole()), parts.predicate);
    decls.push(signature);
    decls.push(check);
    decls.push(code.parse(&mut b));
    decls.push(code.value(&mut b));
    if shows {
        let head = code.own(&mut b);
        let read = code.companion("value");
        decls.push(flick!(b, instance Show #head {
            let show shown = show (#(Name(&read)) shown)
        }));
    }
    let root = b.group(decls);
    Ok(b.finish(root))
}

/// The layout of the declaration: its words, the base and the check, which
/// the compiler lays out, and `deriving` on a line of its own.
// shortcut: `where` never moves to a line of its own; give the layout a soft break when a declaration outgrows its line
pub fn format(block: &Block, compiler: &mut dyn Compiler) -> Option<Vec<Piece>> {
    let parts = read(block, compiler).ok()?;
    let mut head = format!("type {}", block.text_of(parts.name.span));
    for param in &parts.params {
        head.push(' ');
        head.push_str(block.text_of(param.span));
    }
    head.push_str(" = ");
    let hole = |kind, span| Piece::Hole(Hole { kind, span });
    let mut pieces = vec![
        Piece::Text(head),
        hole(HoleKind::Ty, parts.base),
        Piece::Text(" where ".into()),
        hole(HoleKind::Expr, parts.predicate),
    ];
    if parts.deriving.is_some() {
        let mut deriving = String::from("deriving");
        for (i, class) in parts.classes.iter().enumerate() {
            deriving.push_str(if i == 0 { " " } else { ", " });
            deriving.push_str(class);
        }
        pieces.extend([
            Piece::Indent,
            Piece::Line,
            Piece::Text(deriving),
            Piece::Dedent,
        ]);
    }
    Some(pieces)
}

/// What an editor shows on the head, at an offset past the block.
pub fn hover(block: &Block, offset: u32) -> Option<Hover> {
    (offset > block.whole().end).then(|| Hover {
        markdown: ABOUT.into(),
        span: block.whole(),
    })
}
