//! Values of literal tokens. The lexer has already validated the text; on a
//! malformed token these functions still return something instead of panicking.

/// The value of a `String` token: `"…"`, `r#"…"#` or `"""…"""`.
/// Line breaks inside the literal are always `\n`, also for `\r\n` sources.
pub fn string_value(text: &str) -> String {
    if let Some(rest) = text.strip_prefix('r') {
        let hashes = rest.bytes().take_while(|&b| b == b'#').count();
        let close = "\"".to_owned() + &"#".repeat(hashes);
        let body = rest[hashes..].strip_prefix('"').unwrap_or_default();
        return body
            .strip_suffix(close.as_str())
            .unwrap_or(body)
            .replace("\r\n", "\n");
    }
    if let Some(body) = text.strip_prefix("\"\"\"") {
        let body = body.strip_suffix("\"\"\"").unwrap_or(body);
        return unescape(&dedent(&body.replace("\r\n", "\n")));
    }
    let body = text.strip_prefix('"').unwrap_or(text);
    unescape(body.strip_suffix('"').unwrap_or(body))
}

/// Multi-line string layout: drop the line break after the opening `"""`, drop
/// the closing line when it holds only spaces, then remove the indentation
/// common to the non-blank lines and the closing line. Escapes are decoded
/// afterwards, so `\u{20}` is content, not indentation.
fn dedent(body: &str) -> String {
    let body = body.strip_prefix('\n').unwrap_or(body);
    let mut lines: Vec<&str> = body.split('\n').collect();
    let blank = |line: &str| line.bytes().all(|b| b == b' ');
    let indent_of = |line: &str| line.bytes().take_while(|&b| b == b' ').count();
    let closing = match lines.last() {
        Some(last) if blank(last) => {
            let width = last.len();
            lines.pop();
            Some(width)
        }
        _ => None,
    };
    let indent = lines
        .iter()
        .filter(|l| !blank(l))
        .map(|l| indent_of(l))
        .chain(closing)
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|l| if blank(l) { "" } else { &l[indent..] })
        .collect::<Vec<_>>()
        .join("\n")
}

fn unescape(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('0') => out.push('\0'),
            Some('u') => {
                let rest = chars.as_str();
                let scalar = rest
                    .strip_prefix('{')
                    .and_then(|r| r.split_once('}'))
                    .and_then(|(hex, _)| u32::from_str_radix(hex, 16).ok().map(|v| (hex, v)));
                match scalar.and_then(|(hex, v)| char::from_u32(v).map(|c| (hex, c))) {
                    Some((hex, c)) => {
                        out.push(c);
                        chars = rest[hex.len() + 2..].chars();
                    }
                    None => out.push_str("\\u"),
                }
            }
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// The value of a `Number` token, rounded to binary64 like JS; `None` for a
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
    let digits: String = text
        .strip_suffix('n')?
        .chars()
        .filter(|&c| c != '_')
        .collect();
    let (base, rest) = radix(&digits).unwrap_or((10, &digits));
    i128::from_str_radix(rest, base).ok()
}

fn radix(digits: &str) -> Option<(u32, &str)> {
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
