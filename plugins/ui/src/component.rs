//! `component name (props : P) = body`: what it declares, the code it
//! stands for, its layout and what an editor shows on its head.

use flick_plugin::{
    Arena, Block, Builder, Compiler, Decl, Diagnostic, Expr, Hole, HoleKind, Hover, NodeId, Pat,
    Piece, Span, Token, TokenKind,
};

/// What an editor shows on the head.
const ABOUT: &str = "A view of its props. The statements of its body set up each instance \
                     once, in the instance's region `s`; its last expression renders it, \
                     again when an atom it read changes.";

/// A component as its block writes it: its name, its props, and its body,
/// either one expression on the line of `=` or statements on lines of their
/// own, the last the render.
struct Parts {
    name: Token,
    props: Span,
    inline: bool,
    setup: Vec<Hole>,
    render: Span,
}

/// The code of the block `component …`: `let name = Ui.view (fun props ->
/// body)`, or with a setup `let name = Ui.stateful (fun props -> setup; fun
/// props -> render)`, the props bound in both.
pub fn expand(block: &Block, compiler: &mut dyn Compiler) -> Result<Arena, Vec<Diagnostic>> {
    let parts = read(block, compiler)?;
    let mut b = Builder::new(block.whole());
    let props = |b: &mut Builder| b.at(parts.props).pat(Pat::Hole(parts.props));
    let render = b.at(parts.render).expr(Expr::Hole(parts.render));
    let (maker, instance) = if parts.setup.is_empty() {
        let params = vec![props(&mut b)];
        ("Ui.view", lambda(&mut b, block.whole(), params, render))
    } else {
        let again = vec![props(&mut b)];
        let rendered = lambda(&mut b, parts.render, again, render);
        let statements: Vec<NodeId> = (parts.setup.iter())
            .map(|h| match h.kind {
                HoleKind::Decl => b.at(h.span).decl(Decl::Hole(h.span)),
                _ => b.at(h.span).expr(Expr::Hole(h.span)),
            })
            .collect();
        let setup = b.at(block.whole()).expr(Expr::Block {
            statements,
            result: rendered,
        });
        let params = vec![props(&mut b)];
        ("Ui.stateful", lambda(&mut b, block.whole(), params, setup))
    };
    let function = b.at(block.whole()).expr(Expr::Name(maker.into()));
    let made = b.expr(Expr::Apply {
        function,
        arguments: vec![instance],
    });
    let name = block.text_of(parts.name.span).to_owned();
    let root = b.at(parts.name.span).decl(Decl::Let {
        name,
        params: Vec::new(),
        ty: None,
        body: made,
        rec: false,
    });
    Ok(b.finish(root))
}

/// `fun params -> body`, its origin `at`: the instance, which stands for
/// the whole block, its type what an editor shows on the head.
fn lambda(b: &mut Builder, at: Span, params: Vec<NodeId>, body: NodeId) -> NodeId {
    b.at(at).expr(Expr::Lambda { params, body })
}

fn refuse(block: &Block, message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::error(message, block.whole()).with_code("U010")]
}

/// The parts of `component name (props : P) = body`.
fn read(block: &Block, compiler: &mut dyn Compiler) -> Result<Parts, Vec<Diagnostic>> {
    let mut c = block.cursor();
    let name = (c.ident()).ok_or_else(|| refuse(block, "expected the name of the component"))?;
    let props = c.pat(compiler).map_err(|d| vec![d])?;
    let eq = (c.eat("="))
        .ok_or_else(|| refuse(block, "expected `=` after the props of the component"))?;
    let body: Vec<Token> = (block.tokens.iter())
        .filter(|t| t.span.start >= eq.span.end)
        .copied()
        .collect();
    let Some(first) = body.first() else {
        return Err(refuse(
            block,
            "expected the body of the component after `=`",
        ));
    };
    let inline = !block
        .text_of(Span {
            start: eq.span.end,
            end: first.span.start,
        })
        .contains('\n');
    if inline {
        let render = compiler.parse_body(first.span.start, block.whole().end);
        let render = render.map_err(|d| vec![d])?;
        return Ok(Parts {
            name,
            props,
            inline,
            setup: Vec::new(),
            render: render.span,
        });
    }
    // a statement starts a line at the column of the first, unless the line
    // goes on with the one before: an operator, a closer, `then`, `else`
    let column = column_of(block, first.span.start);
    let starts: Vec<u32> = (body.iter())
        .filter(|t| starts_line(block, t.span.start) && column_of(block, t.span.start) == column)
        .filter(|t| !continues(block, **t))
        .map(|t| t.span.start)
        .collect();
    let end = block.whole().end;
    let mut setup = Vec::new();
    for (i, &start) in starts.iter().enumerate() {
        let until = starts.get(i + 1).copied().unwrap_or(end);
        if i + 1 == starts.len() {
            let render = compiler.parse_expr(start, until).map_err(|d| vec![d])?;
            return Ok(Parts {
                name,
                props,
                inline,
                setup,
                render: render.span,
            });
        }
        let token = body.iter().find(|t| t.span.start == start).copied();
        let next = body.iter().find(|t| t.span.start > start).copied();
        let declared = token.is_some_and(|t| block.text_of(t.span) == "let")
            || token.is_some_and(|t| t.kind == TokenKind::Ident)
                && next.is_some_and(|t| block.text_of(t.span) == ":");
        let hole = if declared {
            compiler.parse_decl(start, until)
        } else {
            compiler.parse_expr(start, until)
        };
        setup.push(hole.map_err(|d| vec![d])?);
    }
    Err(refuse(
        block,
        "expected the body of the component after `=`",
    ))
}

/// Whether only spaces come before `offset` on its line.
fn starts_line(block: &Block, offset: u32) -> bool {
    let before = block.text.get(..offset as usize).unwrap_or("");
    let line = before.rfind('\n').map_or(0, |i| i + 1);
    before[line..].trim().is_empty()
}

/// The column of `offset` on its line.
fn column_of(block: &Block, offset: u32) -> usize {
    let before = block.text.get(..offset as usize).unwrap_or("");
    before.len() - before.rfind('\n').map_or(0, |i| i + 1)
}

/// Whether a line that starts with `token` goes on with the statement
/// before: an operator or a closer, `then`, `else` or `with`. A `-` starts a
/// statement, a negative number, as the parser reads a block.
fn continues(block: &Block, token: Token) -> bool {
    let text = block.text_of(token.span);
    match token.kind {
        TokenKind::Punct => !matches!(text, "(" | "[" | "{" | "-"),
        TokenKind::Keyword => matches!(text, "then" | "else" | "with"),
        _ => false,
    }
}

/// The layout of the block: `component name props =` then the body on that
/// line, or a line a statement one level in.
pub fn format(block: &Block, compiler: &mut dyn Compiler) -> Option<Vec<Piece>> {
    let parts = read(block, compiler).ok()?;
    // the compiler writes the head
    let mut pieces = vec![
        Piece::Text(format!("{} ", block.text_of(parts.name.span))),
        Piece::Hole(Hole {
            kind: HoleKind::Pat,
            span: parts.props,
        }),
        Piece::Text(" =".into()),
    ];
    let render = Hole {
        kind: HoleKind::Expr,
        span: parts.render,
    };
    if parts.inline {
        pieces.extend([Piece::Text(" ".into()), Piece::Hole(render)]);
        return Some(pieces);
    }
    pieces.push(Piece::Indent);
    for hole in parts.setup.into_iter().chain([render]) {
        pieces.extend([Piece::Line, Piece::Hole(hole)]);
    }
    pieces.push(Piece::Dedent);
    Some(pieces)
}

/// What an editor shows on the head, at an offset past the block: what a
/// component is.
pub fn hover(block: &Block, offset: u32) -> Option<Hover> {
    (offset > block.whole().end).then(|| Hover {
        markdown: ABOUT.into(),
        span: block.whole(),
    })
}

/// Structural warnings on the original setup and render, before expansion.
pub fn check(block: &Block, s: &flick_plugin::Structure) -> Vec<Diagnostic> {
    let Some(name) = block.tokens.first() else {
        return Vec::new();
    };
    let name_text = block.text_of(name.span);
    let mut out = Vec::new();
    {
        if s.lines > s.max_lines {
            out.push(
                Diagnostic::warning(
                    format!(
                        "`{name_text}` is {} lines long, more than {}",
                        s.lines, s.max_lines
                    ),
                    name.span,
                )
                .with_code("T036")
                .with_fix("split it into smaller functions", Vec::new(), false),
            );
        }
        if s.complexity > s.max_complexity {
            out.push(
                Diagnostic::warning(
                    format!(
                        "`{name_text}` has a complexity of {}, more than {}",
                        s.complexity, s.max_complexity
                    ),
                    name.span,
                )
                .with_code("T038")
                .with_fix(
                    "move some of its branches into functions of their own",
                    Vec::new(),
                    false,
                ),
            );
        }
        for n in &s.nesting {
            out.push(
                Diagnostic::warning(
                    format!(
                        "nested {} levels deep, more than {}",
                        n.depth, s.max_nesting
                    ),
                    n.span,
                )
                .with_code("T037")
                .with_fix(
                    "move the inner part into a function of its own",
                    Vec::new(),
                    false,
                ),
            );
        }
    }
    out
}
