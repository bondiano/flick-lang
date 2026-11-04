//! Values of literal tokens. The lexer has already validated the text; on a
//! malformed token these functions still return something instead of panicking.

pub use flick_plugin::string_value;

/// The value of a `Float` token, rounded to binary64 like JS; `None` for a
/// radix literal longer than 128 bits.
pub fn number_value(text: &str) -> Option<f64> {
    let digits: String = text.chars().filter(|&c| c != '_').collect();
    match radix(&digits) {
        // `u128 as f64` rounds to nearest, ties to even, as JS does
        #[allow(clippy::cast_precision_loss, reason = "rounding is the JS semantics")]
        Some((base, rest)) => u128::from_str_radix(rest, base).ok().map(|v| v as f64),
        None => digits.parse().ok(),
    }
}

/// The value of a `BigInt` token (`…n`), if it fits in 128 bits.
pub fn bigint_value(text: &str) -> Option<i128> {
    text.strip_suffix('n')?;
    bigint_digits_value(&bigint_digits(text))
}

/// The digits of an integer token, with no `_` and no `n` suffix.
pub fn bigint_digits(text: &str) -> String {
    let text = text.strip_suffix('n').unwrap_or(text);
    text.chars().filter(|&c| c != '_').collect()
}

/// The value of [`bigint_digits`], if it fits in 128 bits.
pub fn bigint_digits_value(digits: &str) -> Option<i128> {
    let (base, rest) = radix(digits).unwrap_or((10, digits));
    i128::from_str_radix(rest, base).ok()
}

pub(crate) fn radix(digits: &str) -> Option<(u32, &str)> {
    let lower = digits.get(..2)?.to_ascii_lowercase();
    let base = match lower.as_str() {
        "0x" => 16,
        "0o" => 8,
        "0b" => 2,
        _ => return None,
    };
    Some((base, &digits[2..]))
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
