//! Diagnostic model: stable `FLICK-` code, severity, message, UTF-8 byte
//! span, related spans and fix suggestions.

pub mod codes;
pub mod render;
pub mod suggest;

pub use codes::Code;

mod model;

pub use model::Fix;
pub use model::{Diagnostic, Edit, Related, Severity};
