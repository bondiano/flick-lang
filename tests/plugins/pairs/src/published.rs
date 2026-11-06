//! The heads of `pairs` that tell other modules and groups about a name:
//! - `note greeting "hello"`, a declaration: `let greeting = "hello"`, and
//!   `greeting` published with the text `hello`;
//! - `heard greeting`, `heard Main.greeting`: the text the plugin published
//!   of that name as the module writes it, or `"unheard"`;
//! - `above`, the whole body of a `let`: the signature above it, or
//!   `"none"`.

use flick_plugin::{
    Arena, Block, Builder, Compiler, Diagnostic, Expr, Published, flick, string_value,
};

/// What `collect` adds to the state: a line `module name text` of each
/// `note`.
pub fn collect(blocks: &[Block]) -> Vec<u8> {
    let mut out = String::new();
    for block in blocks.iter().filter(|b| b.head == "note") {
        if let [name, text, ..] = block.arguments() {
            let name = block.text_of(name.span);
            let text = string_value(block.text_of(text.span));
            out += &[block.module.as_str(), name, &text].join("\t");
            out.push('\n');
        }
    }
    out.into_bytes()
}

/// Each `note` of the state, published.
pub fn publish(notes: &[u8]) -> Vec<Published> {
    (String::from_utf8_lossy(notes).lines())
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            Some(Published {
                module: parts.next()?.to_owned(),
                name: parts.next()?.to_owned(),
                data: parts.next()?.as_bytes().to_vec(),
            })
        })
        .collect()
}

/// The code of `block` when its head is one of these; `None` otherwise.
pub fn expand(
    block: &Block,
    compiler: &mut dyn Compiler,
) -> Option<Result<Arena, Vec<Diagnostic>>> {
    let mut b = Builder::new(block.whole());
    let made = match block.head.as_str() {
        "note" => {
            let [name, text, ..] = block.arguments() else {
                return Some(Err(vec![Diagnostic::error(
                    "expected a name and a text",
                    block.whole(),
                )]));
            };
            let text = string_value(block.text_of(text.span));
            flick!(b, let #(block.text_of(name.span)) = #(text.as_str()))
        }
        "heard" => {
            let name = block.text.trim();
            let heard = compiler.published(name);
            let text = heard.map_or_else(
                || "unheard".to_owned(),
                |d| String::from_utf8_lossy(&d).into_owned(),
            );
            b.expr(Expr::Text(text))
        }
        "above" => {
            let text = (block.signature.as_ref()).map_or("none", |s| s.text.as_str());
            b.expr(Expr::Text(text.to_owned()))
        }
        _ => return None,
    };
    Some(Ok(b.finish(made)))
}
