//! Source maps, version 3: each mapped position of the generated code
//! points at the start of the Flick node it comes from.

use std::collections::BTreeMap;

use super::Output;
use crate::source::{FileId, SourceMap};

/// The `.map` JSON for `output`, written next to `file`; `source_path`
/// gives the path of a source as the map refers to it.
pub fn source_map(
    file: &str,
    output: &Output,
    sources: &SourceMap,
    source_path: &dyn Fn(FileId) -> String,
) -> String {
    let mut files: BTreeMap<FileId, usize> = BTreeMap::new();
    let mut names: Vec<String> = Vec::new();
    for (_, _, span) in &output.mappings {
        files.entry(span.file).or_insert_with(|| {
            names.push(source_path(span.file));
            names.len() - 1
        });
    }
    let mut mappings = String::new();
    let mut line = 0;
    let (mut prev_col, mut prev_src, mut prev_line, mut prev_src_col) = (0i64, 0i64, 0i64, 0i64);
    let mut first = true;
    for (gen_line, gen_col, span) in &output.mappings {
        while line < *gen_line {
            mappings.push(';');
            line += 1;
            prev_col = 0;
            first = true;
        }
        if !first {
            mappings.push(',');
        }
        first = false;
        let source = sources.get(span.file);
        let (src_line, byte_col) = source.line_col(span.start);
        let line_start = (span.start - byte_col) as usize;
        let src_col = source.text[line_start..span.start as usize]
            .encode_utf16()
            .count();
        let src = i64::try_from(files[&span.file]).unwrap_or(i64::MAX);
        let (gen_col, src_line, src_col) = (
            i64::from(*gen_col),
            i64::from(src_line),
            i64::try_from(src_col).unwrap_or(i64::MAX),
        );
        for delta in [
            gen_col - prev_col,
            src - prev_src,
            src_line - prev_line,
            src_col - prev_src_col,
        ] {
            vlq(delta, &mut mappings);
        }
        (prev_col, prev_src, prev_line, prev_src_col) = (gen_col, src, src_line, src_col);
    }
    let map = serde_json::json!({
        "version": 3,
        "file": file,
        "sources": names,
        "names": [],
        "mappings": mappings,
    });
    map.to_string() + "\n"
}

fn vlq(value: i64, out: &mut String) {
    const DIGITS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut v = if value < 0 {
        ((-value) << 1) | 1
    } else {
        value << 1
    };
    loop {
        let mut digit = usize::try_from(v & 31).unwrap_or(0);
        v >>= 5;
        if v > 0 {
            digit |= 32;
        }
        out.push(DIGITS[digit] as char);
        if v == 0 {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn vlq_matches_the_reference_encoding() {
        let mut out = String::new();
        for v in [0, 1, -1, 15, 16, -17, 1000] {
            super::vlq(v, &mut out);
            out.push(' ');
        }
        assert_eq!(out, "A C D e gB jB w+B ");
    }
}
