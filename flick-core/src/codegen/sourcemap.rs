//! Source positions for the JavaScript emitter's version 3 maps.

use super::Output;
use crate::source::{FileId, SourceMap};
use flick_backend_js::codegen::sourcemap;

/// The `.map` JSON for `output`, using the shared source table's UTF-16 positions.
pub fn source_map(
    file: &str,
    output: &Output,
    sources: &SourceMap,
    source_path: &dyn Fn(FileId) -> String,
) -> String {
    sourcemap::source_map(file, output, source_path, &|span| {
        let source = sources.get(span.file);
        let (line, byte_col) = source.line_col(span.start);
        let start = (span.start - byte_col) as usize;
        let col = source.text[start..span.start as usize]
            .encode_utf16()
            .count();
        (line, col)
    })
}
