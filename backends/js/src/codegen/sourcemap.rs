//! Source maps, version 3: each mapped position of the generated code
//! points at the start of the Flick node it comes from.

use std::collections::BTreeMap;

use super::Output;
use flick_ir::{FileId, Span};

/// The `.map` JSON for `output`, written next to `file`; `source_path`
/// gives the path of a source as the map refers to it.
pub fn source_map(
    file: &str,
    output: &Output,
    source_path: &dyn Fn(FileId) -> String,
    source_position: &dyn Fn(Span) -> (u32, usize),
) -> String {
    let mut files: BTreeMap<FileId, usize> = BTreeMap::new();
    let mut paths: Vec<String> = Vec::new();
    let mut names: Vec<&str> = Vec::new();
    for (_, _, span, name) in &output.mappings {
        files.entry(span.file).or_insert_with(|| {
            paths.push(source_path(span.file));
            paths.len() - 1
        });
        if let Some(name) = name
            && !names.contains(&name.as_str())
        {
            names.push(name);
        }
    }
    let mut mappings = String::new();
    let mut line = 0;
    let (mut prev_col, mut prev_src, mut prev_line, mut prev_src_col) = (0i64, 0i64, 0i64, 0i64);
    let mut prev_name = 0i64;
    let mut first = true;
    for (gen_line, gen_col, span, name) in &output.mappings {
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
        let (src_line, src_col) = source_position(*span);
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
        if let Some(name) = name {
            let index = names.iter().position(|n| n == name).unwrap_or(0);
            let index = i64::try_from(index).unwrap_or(i64::MAX);
            vlq(index - prev_name, &mut mappings);
            prev_name = index;
        }
    }
    let map = serde_json::json!({
        "version": 3,
        "file": file,
        "sources": paths,
        "names": names,
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
