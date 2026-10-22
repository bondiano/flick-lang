//! Lexical and surface syntax (§3–§12): lexer, layout-aware parser, lossless
//! concrete tree and its trivia-free AST view. Grammar: `docs/reference/grammar.md`.

pub mod ast;
pub mod cst;
pub mod kind;
pub mod lexer;
pub mod literal;
mod parser;
mod validate;

pub use parser::{Parse, parse};
