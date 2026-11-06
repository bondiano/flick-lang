//! Lexical and surface syntax: lexer, layout-aware parser, lossless
//! concrete tree and its trivia-free AST view. Grammar: `docs/src/content/docs/reference/grammar.md`.

pub mod ast;
pub mod cst;
pub mod kind;
pub mod lexer;
pub mod literal;
mod parser;
mod validate;

pub use parser::{
    Fragment, Heads, Parse, parse, parse_body_at, parse_decl_at, parse_decls_at, parse_expr_at,
    parse_pat_at, parse_type_at, parse_with_heads,
};
