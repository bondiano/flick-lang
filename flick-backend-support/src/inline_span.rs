//! Maps byte offsets in decoded inline foreign code back to exact UTF-8
//! source [`Span`]s inside the enclosing `.fl` string literal token.
//!
//! Decoding mirrors `flick_plugin::string_value` (`"..."`, `r"..."`, `r#"..."#`,
//! and `"""..."""` with `\r\n` normalization, common-indent dedent, and escape
//! decoding) so backend diagnostics reported against decoded inline host code
//! land on the exact line and column of the `.fl` source file.

use flick_ir::Span;

#[derive(Debug, Clone, Copy)]
struct Unit {
    dec_start: usize,
    dec_end: usize,
    raw_start: usize,
    raw_end: usize,
}

struct Line<'a> {
    text: &'a str,
    raw_start: usize,
    sep_start: usize,
    sep_end: usize,
}

/// Maps `[decoded_start, decoded_end)` (byte offsets in the decoded inline host
/// code) back to a valid UTF-8 [`Span`] within `source` inside `token_span`.
pub fn map_byte_range(
    source: &str,
    token_span: Span,
    decoded_start: usize,
    decoded_end: usize,
) -> Span {
    let start_abs = token_span.start as usize;
    let end_abs = token_span.end as usize;
    let Some(raw) = source.get(start_abs..end_abs) else {
        return token_span;
    };
    let (units, dec_len, content_start, content_end) = decode_map(raw);
    if units.is_empty() {
        return clamp_span(source, token_span, content_start, content_end);
    }
    let ds = decoded_start.min(dec_len);
    let de = decoded_end.min(dec_len).max(ds);
    let (raw_start, raw_end) = if ds >= dec_len {
        let last = units[units.len() - 1];
        (last.raw_start, last.raw_end)
    } else {
        let first_idx = units
            .partition_point(|u| u.dec_end <= ds)
            .min(units.len() - 1);
        let raw_start = units[first_idx].raw_start;
        let raw_end = if de <= ds {
            units[first_idx].raw_end
        } else {
            let last_idx = units
                .partition_point(|u| u.dec_start < de)
                .saturating_sub(1)
                .max(first_idx);
            units[last_idx].raw_end
        };
        (raw_start, raw_end)
    };
    clamp_span(source, token_span, raw_start, raw_end)
}

/// Maps a single byte offset in decoded inline code to a [`Span`] in `source`.
pub fn map_offset(source: &str, token_span: Span, decoded_offset: usize) -> Span {
    map_byte_range(source, token_span, decoded_offset, decoded_offset)
}

/// Returns the 1-based line number in `source` corresponding to
/// `decoded_offset` inside `token_span`, using `line_starts`.
pub fn line_for_offset(
    source: &str,
    token_span: Span,
    decoded_offset: usize,
    line_starts: &[u32],
) -> u32 {
    let mapped = map_offset(source, token_span, decoded_offset);
    let line = line_starts.partition_point(|&start| start <= mapped.start);
    u32::try_from(line.max(1)).expect("fewer than 2^32 lines")
}

fn clamp_span(source: &str, token_span: Span, rel_start: usize, rel_end: usize) -> Span {
    let base = token_span.start as usize;
    let abs_start = base.saturating_add(rel_start);
    let abs_end = base.saturating_add(rel_end).max(abs_start);
    if abs_end <= source.len()
        && source.is_char_boundary(abs_start)
        && source.is_char_boundary(abs_end)
        && let (Ok(s), Ok(e)) = (u32::try_from(abs_start), u32::try_from(abs_end))
    {
        Span::new(token_span.file, s, e)
    } else {
        token_span
    }
}

fn decode_map(raw: &str) -> (Vec<Unit>, usize, usize, usize) {
    let mut units = Vec::new();
    let mut dec = 0usize;
    if let Some(rest) = raw.strip_prefix('r') {
        let hashes = rest.bytes().take_while(|&b| b == b'#').count();
        let open_len = 1 + hashes + usize::from(rest[hashes..].starts_with('"'));
        let close = "\"".to_owned() + &"#".repeat(hashes);
        let body_end = if raw.len() >= open_len + close.len() && raw.ends_with(close.as_str()) {
            raw.len() - close.len()
        } else {
            raw.len()
        };
        let body = &raw[open_len..body_end];
        let bytes = body.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
                units.push(Unit {
                    dec_start: dec,
                    dec_end: dec + 1,
                    raw_start: open_len + i,
                    raw_end: open_len + i + 2,
                });
                dec += 1;
                i += 2;
            } else {
                let ch = body[i..].chars().next().expect("valid UTF-8");
                let len = ch.len_utf8();
                units.push(Unit {
                    dec_start: dec,
                    dec_end: dec + len,
                    raw_start: open_len + i,
                    raw_end: open_len + i + len,
                });
                dec += len;
                i += len;
            }
        }
        return (units, dec, open_len, body_end);
    }
    if raw.starts_with("\"\"\"") {
        let mut body_start = 3;
        let body_end = if raw.len() >= 6 && raw.ends_with("\"\"\"") {
            raw.len() - 3
        } else {
            raw.len()
        };
        let mut body = &raw[body_start..body_end];
        if let Some(rest) = body.strip_prefix("\r\n") {
            body_start += 2;
            body = rest;
        } else if let Some(rest) = body.strip_prefix('\n') {
            body_start += 1;
            body = rest;
        }
        let mut lines = split_raw_lines(body, body_start);
        let blank = |s: &str| s.bytes().all(|b| b == b' ');
        let indent_of = |s: &str| s.bytes().take_while(|&b| b == b' ').count();
        let closing = match lines.last() {
            Some(last) if blank(last.text) => {
                let width = last.text.len();
                lines.pop();
                Some(width)
            }
            _ => None,
        };
        let indent = lines
            .iter()
            .filter(|l| !blank(l.text))
            .map(|l| indent_of(l.text))
            .chain(closing)
            .min()
            .unwrap_or(0);
        for (idx, line) in lines.iter().enumerate() {
            if idx > 0 {
                let prev = &lines[idx - 1];
                units.push(Unit {
                    dec_start: dec,
                    dec_end: dec + 1,
                    raw_start: prev.sep_start,
                    raw_end: prev.sep_end,
                });
                dec += 1;
            }
            if !blank(line.text) {
                let trimmed = &line.text[indent..];
                push_unescaped(trimmed, line.raw_start + indent, &mut dec, &mut units);
            }
        }
        return (units, dec, body_start, body_end);
    }
    let body_start = usize::from(raw.starts_with('"'));
    let body_end = if raw.len() > body_start && raw.ends_with('"') {
        raw.len() - 1
    } else {
        raw.len()
    };
    push_unescaped(&raw[body_start..body_end], body_start, &mut dec, &mut units);
    (units, dec, body_start, body_end)
}

fn split_raw_lines(body: &str, base: usize) -> Vec<Line<'_>> {
    let bytes = body.as_bytes();
    let mut lines = Vec::new();
    let mut line_start = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
            lines.push(Line {
                text: &body[line_start..i],
                raw_start: base + line_start,
                sep_start: base + i,
                sep_end: base + i + 2,
            });
            i += 2;
            line_start = i;
        } else if bytes[i] == b'\n' {
            lines.push(Line {
                text: &body[line_start..i],
                raw_start: base + line_start,
                sep_start: base + i,
                sep_end: base + i + 1,
            });
            i += 1;
            line_start = i;
        } else {
            i += 1;
        }
    }
    lines.push(Line {
        text: &body[line_start..],
        raw_start: base + line_start,
        sep_start: base + body.len(),
        sep_end: base + body.len(),
    });
    lines
}

fn push_unescaped(body: &str, base: usize, dec: &mut usize, units: &mut Vec<Unit>) {
    let mut i = 0;
    while i < body.len() {
        let ch = body[i..].chars().next().expect("valid UTF-8");
        if ch != '\\' {
            let len = ch.len_utf8();
            units.push(Unit {
                dec_start: *dec,
                dec_end: *dec + len,
                raw_start: base + i,
                raw_end: base + i + len,
            });
            *dec += len;
            i += len;
            continue;
        }
        let esc_start = i;
        i += 1;
        let Some(next) = body[i..].chars().next() else {
            units.push(Unit {
                dec_start: *dec,
                dec_end: *dec + 1,
                raw_start: base + esc_start,
                raw_end: base + i,
            });
            *dec += 1;
            break;
        };
        match next {
            'n' | 't' | 'r' | '0' => {
                i += 1;
                units.push(Unit {
                    dec_start: *dec,
                    dec_end: *dec + 1,
                    raw_start: base + esc_start,
                    raw_end: base + i,
                });
                *dec += 1;
            }
            'u' => {
                i += 1;
                let rest = &body[i..];
                let scalar = rest
                    .strip_prefix('{')
                    .and_then(|r| r.split_once('}'))
                    .and_then(|(hex, _)| u32::from_str_radix(hex, 16).ok().map(|v| (hex, v)))
                    .and_then(|(hex, v)| char::from_u32(v).map(|c| (hex, c)));
                if let Some((hex, c)) = scalar {
                    i += hex.len() + 2;
                    let len = c.len_utf8();
                    units.push(Unit {
                        dec_start: *dec,
                        dec_end: *dec + len,
                        raw_start: base + esc_start,
                        raw_end: base + i,
                    });
                    *dec += len;
                } else {
                    units.push(Unit {
                        dec_start: *dec,
                        dec_end: *dec + 2,
                        raw_start: base + esc_start,
                        raw_end: base + i,
                    });
                    *dec += 2;
                }
            }
            other => {
                let len = other.len_utf8();
                i += len;
                units.push(Unit {
                    dec_start: *dec,
                    dec_end: *dec + len,
                    raw_start: base + esc_start,
                    raw_end: base + i,
                });
                *dec += len;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::iter::once;

    use flick_ir::{FileId, Span};

    use super::{line_for_offset, map_byte_range};

    fn span(start: usize, end: usize) -> Span {
        Span::new(
            FileId::new(0),
            u32::try_from(start).expect("start fits u32"),
            u32::try_from(end).expect("end fits u32"),
        )
    }

    fn line_starts(s: &str) -> Vec<u32> {
        once(0)
            .chain(
                s.match_indices('\n')
                    .map(|(i, _)| u32::try_from(i + 1).expect("offset fits u32")),
            )
            .collect()
    }

    #[test]
    fn maps_plain_escaped_raw_and_multiline_tokens() {
        let src = r#"foreign f from inline "(x) => \"hi\"\n + @bad""#;
        let tok_start = src.find('"').expect("quote");
        let token_span = span(tok_start, src.len());
        let decoded = "(x) => \"hi\"\n + @bad";
        let bad_off = decoded.find("@bad").expect("bad marker");
        let mapped = map_byte_range(src, token_span, bad_off, bad_off + 4);
        assert_eq!(&src[mapped.start as usize..mapped.end as usize], "@bad");

        let raw_src = "foreign f from inline r#\"a\r\n  bad\"#";
        let tok_start = raw_src.find("r#\"").expect("raw quote");
        let token_span = span(tok_start, raw_src.len());
        let decoded_raw = "a\n  bad";
        let bad_off = decoded_raw.find("bad").expect("bad marker");
        let mapped = map_byte_range(raw_src, token_span, bad_off, bad_off + 3);
        assert_eq!(&raw_src[mapped.start as usize..mapped.end as usize], "bad");
    }

    #[test]
    fn maps_multiline_dedented_unicode_and_crlf_lines() {
        let src =
            "// π\nforeign module inline \"\"\"\n  const a = 1;\n\n  const b = @;\n  \"\"\"\n";
        let tok_start = src.find("\"\"\"").expect("open triple quote");
        let tok_end = src.rfind("\"\"\"").expect("close triple quote") + 3;
        let token_span = span(tok_start, tok_end);
        let decoded = "const a = 1;\n\nconst b = @;";
        let at_off = decoded.find('@').expect("@ marker");
        let mapped = map_byte_range(src, token_span, at_off, at_off + 1);
        assert_eq!(&src[mapped.start as usize..mapped.end as usize], "@");
        let starts = line_starts(src);
        assert_eq!(line_for_offset(src, token_span, 0, &starts), 3);
        assert_eq!(line_for_offset(src, token_span, at_off, &starts), 5);

        let crlf = "foreign f from inline \"\"\"\r\n    a\r\n    😀 + ?\r\n  \"\"\"";
        let tok_start = crlf.find("\"\"\"").expect("open triple quote");
        let token_span = span(tok_start, crlf.len());
        let decoded = "  a\n  😀 + ?";
        let q_off = decoded.find('?').expect("? marker");
        let mapped = map_byte_range(crlf, token_span, q_off, q_off + 1);
        assert_eq!(&crlf[mapped.start as usize..mapped.end as usize], "?");
    }

    #[test]
    fn maps_unicode_escape_and_eof_safely() {
        let src = r#"foreign f from inline "\u{1F600} + ""#;
        let tok_start = src.find('"').expect("quote");
        let token_span = span(tok_start, src.len());
        let decoded = "😀 + ";
        let emoji_mapped = map_byte_range(src, token_span, 0, "😀".len());
        assert_eq!(
            &src[emoji_mapped.start as usize..emoji_mapped.end as usize],
            r"\u{1F600}"
        );
        let eof_mapped = map_byte_range(src, token_span, decoded.len(), decoded.len());
        assert!(src.is_char_boundary(eof_mapped.start as usize));
        assert!(src.is_char_boundary(eof_mapped.end as usize));
        assert!(eof_mapped.start <= eof_mapped.end);
    }
}
