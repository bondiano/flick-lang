//! The layout of a block: the command's name and `{`, then a clause a
//! line, one level in, each after its `///` lines, the user's code in it
//! laid out by the compiler.

use flick_plugin::{Block, Compiler, Hole, HoleKind, Piece, Span};

use crate::read::{self, Clause};

/// The pieces of `block`, or none when it is not clauses of a command.
pub fn format(block: &Block, compiler: &mut dyn Compiler) -> Option<Vec<Piece>> {
    let command = read::command(block, compiler).ok()?;
    let text = |span: Span| Piece::Text(block.text_of(span).to_owned());
    let hole = |kind: HoleKind, span: Span| Piece::Hole(Hole { kind, span });
    let word = |w: &str| Piece::Text(format!("{w} "));
    let mut out = vec![text(command.name), Piece::Text(" {".into()), Piece::Indent];
    for clause in &command.clauses {
        out.push(Piece::Line);
        let docs = block.docs_above(clause.word().start, "\n");
        for line in docs.lines().filter(|_| !docs.is_empty()) {
            out.push(Piece::Text(format!("/// {line}").trim_end().to_owned()));
            out.push(Piece::Line);
        }
        match clause {
            Clause::Description { value, .. } => {
                out.extend([word("description"), hole(HoleKind::Expr, *value)]);
            }
            Clause::Subcommand { value, .. } => {
                out.extend([word("subcommand"), hole(HoleKind::Expr, *value)]);
            }
            Clause::Handle { value, .. } => {
                out.extend([word("handle"), hole(HoleKind::Expr, *value)]);
            }
            Clause::Param(p) => {
                out.extend([word(if p.flag { "flag" } else { "arg" }), text(p.name)]);
                for alias in &p.aliases {
                    out.extend([Piece::Text(" ".into()), text(*alias)]);
                }
                out.extend([Piece::Text(" : ".into()), hole(HoleKind::Ty, p.ty)]);
                if let Some(default) = p.default {
                    out.extend([Piece::Text(" = ".into()), hole(HoleKind::Expr, default)]);
                }
            }
        }
    }
    out.extend([Piece::Dedent, Piece::Line]);
    Some(out)
}
