//! `ui { … }`: the body with the module `Ui` open, so that `text`, `button`
//! and the rest of the library need no `Ui.` there.

use flick_plugin::{
    Arena, Block, Builder, Compiler, Diagnostic, Expr, Hole, HoleKind, Hover, Piece,
};

/// What an editor shows on the head.
const ABOUT: &str = "Describes a user interface; building it runs no handler.";

/// The code of the block: its body with `Ui` open.
pub fn expand(block: &Block, compiler: &mut dyn Compiler) -> Result<Arena, Vec<Diagnostic>> {
    let body = block.body().lines(compiler).map_err(|d| vec![d])?;
    let mut b = Builder::new(block.whole());
    let user = b.at(body).expr(Expr::Lines(body));
    let root = b.at(block.whole()).expr(Expr::Open {
        module: "Ui".into(),
        child: Some("Ui".into()),
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
