//! `ui { … }`: the body with the module `Ui` open, so that `text`, `button`
//! and the rest of the library need no `Ui.` there. Several expression
//! lines, after the `let`s, are the children of one `Ui.fragment`.

use flick_plugin::{
    Arena, Block, Builder, Compiler, Diagnostic, Expr, Hole, HoleKind, Hover, Piece, Span,
};

/// What an editor shows on the head.
const ABOUT: &str = "Describes a user interface; building it runs no handler.";

/// The code of the block: its body with `Ui` open.
pub fn expand(block: &Block, compiler: &mut dyn Compiler) -> Result<Arena, Vec<Diagnostic>> {
    let mut rest = block.body();
    let body = rest.lines(compiler).map_err(|d| vec![d])?;
    // the body is all of it: what follows would be lost
    if let Some(next) = rest.peek() {
        let span = Span {
            start: next.span.start,
            end: rest.rest().last().map_or(next.span.end, |t| t.span.end),
        };
        let message = "expected the end of `ui` after its body: `;` separates its lines";
        return Err(vec![Diagnostic::error(message, span).with_code("U010")]);
    }
    let mut b = Builder::new(block.whole());
    let user = b.at(body).expr(Expr::Lines(body));
    let root = b.at(block.whole()).expr(Expr::Open {
        module: "Ui".into(),
        child: Some("Ui".into()),
        // a foreign component takes its props through `Ui.component`
        through: Some("Ui.component".into()),
        // several lines are the children of one fragment
        siblings: Some("Ui.fragment".into()),
        body: user,
    });
    Ok(b.finish(root))
}

/// The layout of the block: its body, which the compiler lays out.
pub fn format(block: &Block, compiler: &mut dyn Compiler) -> Option<Vec<Piece>> {
    let span = block.body().lines(compiler).ok()?;
    let kind = HoleKind::Lines;
    Some(vec![Piece::Hole(Hole { kind, span })])
}

/// What an editor shows on the head, at an offset past the block.
pub fn hover(block: &Block, offset: u32) -> Option<Hover> {
    (offset > block.whole().end).then(|| Hover {
        markdown: ABOUT.into(),
        span: block.whole(),
    })
}
