//! JavaScript source spelling of numeric literals.

pub use flick_ir::{Arith, Num, NumKind};

/// The shorter of the plain and the exponent spelling of a finite `x`,
/// without its sign; both read back as exactly `x`.
pub fn float_text(x: f64) -> String {
    let plain = format!("{}", x.abs());
    let exponent = format!("{:e}", x.abs());
    if exponent.len() < plain.len() {
        exponent
    } else {
        plain
    }
}
