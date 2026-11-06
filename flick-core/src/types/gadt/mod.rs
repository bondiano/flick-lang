//! Constructors that build an instance of their type of their own (GADTs):
//! `| IntLit Int : Expr Int`. Their schemes, and what matching one tells
//! about the type of the matched value.

pub mod branch;
pub mod columns;
pub mod derive;
pub mod refine;
pub mod scheme;
