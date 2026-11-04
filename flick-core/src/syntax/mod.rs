//! Lexical and surface syntax: lexer, layout-aware parser, lossless
//! concrete tree and its trivia-free AST view. Grammar: `docs/src/content/docs/reference/grammar.md`.

pub mod ast;
pub mod cst;
pub mod ddl;
pub mod kind;
pub mod lexer;
pub mod literal;
mod parser;
mod validate;

pub use parser::{Fragment, Parse, parse, parse_expr_at, parse_type_at};
