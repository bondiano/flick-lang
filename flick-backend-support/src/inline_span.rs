//! Maps byte offsets in decoded inline foreign code back to exact UTF-8
//! source [`Span`]s inside the enclosing `.fl` string literal token.
//!
//! Decoding delegates to [`fln_literals::decode_string_mapped`] (`"..."`,
//! `r"..."`, `r#"..."#`, and `"""..."""` with `\r\n` normalization,
//! common-indent dedent, and escape decoding) so backend diagnostics reported
//! against decoded inline host code land on the exact line and column of the
//! `.fl` source file.

use flick_ir::Span;
use fln_literals::decode_string_mapped;

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
    let map = decode_string_mapped(raw);
    let (raw_start, raw_end) = map.map_byte_range(decoded_start, decoded_end);
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
