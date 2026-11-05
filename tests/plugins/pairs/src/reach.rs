//! The heads of `pairs` that reach outside their blocks, as the project
//! grants: `read_file "p"`, `env_var "N"` and `fetch_text "url"` stand for
//! the text of the file, the variable (`unset` without one) and the status
//! and body of the answer; refused, for `refused: <why>`, which swallows
//! the error. `stated {}` stands for the text of `stated.txt`, which
//! `collect` reads once for the group.

use flick_plugin::{Arena, Block, Builder, Compiler, Diagnostic, Expr, Request, string_value};

/// The file `collect` reads when a block of the group is `stated`.
const STATED: &str = "stated.txt";

/// What `collect` adds to the state: the text of `stated.txt`, when a
/// block asks for it.
pub fn collect(blocks: &[Block], compiler: &mut dyn Compiler) -> Vec<u8> {
    if !blocks.iter().any(|b| b.head == "stated") {
        return Vec::new();
    }
    (compiler.read_file(STATED)).unwrap_or_else(|e| format!("refused: {e}").into_bytes())
}

/// The code of `block` when its head is one of these; `None` otherwise.
pub fn expand(
    block: &Block,
    state: &[u8],
    compiler: &mut dyn Compiler,
) -> Option<Result<Arena, Vec<Diagnostic>>> {
    let argument = || {
        let raw = block.arguments().first().map(|t| block.text_of(t.span));
        string_value(raw.unwrap_or(""))
    };
    let text = match block.head.as_str() {
        "read_file" => compiler
            .read_file(&argument())
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned()),
        "env_var" => compiler
            .env(&argument())
            .map(|v| v.unwrap_or_else(|| "unset".to_owned())),
        "fetch_text" => {
            let request = Request {
                method: "GET".into(),
                url: argument(),
                ..Request::default()
            };
            (compiler.fetch(&request))
                .map(|r| format!("{} {}", r.status, String::from_utf8_lossy(&r.body)))
        }
        "stated" => Ok(String::from_utf8_lossy(state.get(4..).unwrap_or_default()).into_owned()),
        _ => return None,
    };
    let text = text.unwrap_or_else(|e| format!("refused: {e}"));
    let mut b = Builder::new(block.whole());
    let made = b.expr(Expr::Text(text));
    Some(Ok(b.finish(made)))
}
