//! Lexical and surface syntax: lexer, layout-aware parser, lossless
//! concrete tree and its trivia-free AST view. Grammar: `docs/src/content/docs/reference/syntax.md`.

pub mod ast;
pub mod cst;
pub mod heads;
pub mod kind;
#[cfg(test)]
mod kind_tests;
pub mod lang;
pub mod lexer;
pub mod literal;
mod parser;
pub mod patterns;
pub mod structure;
pub mod ts_mark;
mod validate;
mod validate_attrs;
mod validate_defer;
mod validate_let_else;
mod validate_native;
pub mod when;

pub use parser::{
    Fragment, Heads, Parse, parse, parse_body_at, parse_decl_at, parse_decls_at, parse_expr_at,
    parse_lines_at, parse_pat_at, parse_type_at, parse_with_heads,
};
