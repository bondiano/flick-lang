//! Values of literal tokens. The lexer has already validated the text; on a
//! malformed token these functions still return something instead of panicking.

use flick_ir::Num;
use fln_literals::{NumSuffix, split_number_suffix as split_fln_suffix};

pub use fln_literals::{
    bigint_digits, bigint_digits_value, bigint_value, decode_string as string_value, float32_value,
    integer_value, number_value, radix,
};

/// Splits an optional fixed-width numeric suffix (`i8`, `i16`, `i32`, `i64`,
/// `u8`, `u16`, `u32`, `u64`, `f32`) from a `Number` token.
///
/// For hexadecimal literals (`0x…`), `f` is a hex digit, so `0x1f32` has no
/// `f32` suffix.
pub fn split_number_suffix(text: &str) -> (&str, Option<Num>) {
    let (stem, suffix) = split_fln_suffix(text);
    (stem, suffix.map(num_of_suffix))
}

const fn num_of_suffix(suffix: NumSuffix) -> Num {
    match suffix {
        NumSuffix::Int8 => Num::Int8,
        NumSuffix::Int16 => Num::Int16,
        NumSuffix::Int32 => Num::Int,
        NumSuffix::Int64 => Num::Int64,
        NumSuffix::UInt8 => Num::UInt8,
        NumSuffix::UInt16 => Num::UInt16,
        NumSuffix::UInt32 => Num::UInt32,
        NumSuffix::UInt64 => Num::UInt64,
        NumSuffix::Float32 => Num::Float32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_decode_escapes_raw_and_multiline() {
        assert_eq!(string_value(r#""a\n\t\"\\\0\u{1F600}""#), "a\n\t\"\\\0😀");
        assert_eq!(string_value(r#"r"C:\temp""#), "C:\\temp");
        assert_eq!(string_value("r#\"say \"hi\"\"#"), "say \"hi\"");
        let multi = "\"\"\"\n    first\n      second\n\n    \\u{20}third\n    \"\"\"";
        assert_eq!(string_value(multi), "first\n  second\n\n third");
        let closing_left = "\"\"\"\r\n    a\r\n  \"\"\"";
        assert_eq!(string_value(closing_left), "  a");
        let content_last = "\"\"\"\n  a\n  b\"\"\"";
        assert_eq!(string_value(content_last), "a\nb");
    }

    #[test]
    fn numbers_round_like_js() {
        assert_eq!(number_value("1_000.5"), Some(1000.5));
        assert_eq!(number_value("1e-3"), Some(0.001));
        assert_eq!(number_value("0xFF"), Some(255.0));
        assert_eq!(number_value("0b1010"), Some(10.0));
        assert_eq!(number_value("1e400"), Some(f64::INFINITY));
        // 2^53 + 1 rounds to even
        assert_eq!(
            number_value("0x20000000000001"),
            Some(9_007_199_254_740_992.0)
        );
        assert_eq!(bigint_value("0x1F_FFn"), Some(0x1fff));
        assert_eq!(bigint_value("10"), None);
    }
}
