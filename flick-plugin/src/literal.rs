//! The value of a string token of Flick, as a block's text holds it: what a
//! plugin reads raw text from, such as `r#"create table …"#`, and what the
//! compiler's own literals mean.

/// The value of the literal of an enum's constructor, as text: a string's
/// value, an integer with its sign, `-1` of `- 1`.
pub fn literal_text(text: &str) -> String {
    if text.ends_with('"') {
        string_value(text)
    } else {
        text.split_whitespace().collect()
    }
}

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
