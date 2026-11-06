//! Source files, UTF-8 byte spans and line index: diagnostics carry UTF-8 byte
//! ranges; editors convert them to their own position encoding.

use std::iter;

pub use flick_ir::{FileId, Span};

use crate::diagnostics::{Diagnostic, codes};

#[derive(Debug)]
pub struct SourceFile {
    /// Display path; a string rather than `PathBuf` because the core also runs in wasm.
    pub path: String,
    pub text: String,
    line_starts: Vec<u32>,
}

impl SourceFile {
    fn new(path: String, text: String) -> Self {
        let line_starts = iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| offset(i + 1)))
            .collect();
        SourceFile {
            path,
            text,
            line_starts,
        }
    }

    /// 0-based line and 0-based byte column of a byte offset.
    pub fn line_col(&self, pos: u32) -> (u32, u32) {
        let line = self.line_starts.partition_point(|&s| s <= pos) - 1;
        (offset(line), pos - self.line_starts[line])
    }

    /// Byte offset of the start of a 0-based line, if the line exists.
    pub fn line_start(&self, line: u32) -> Option<u32> {
        self.line_starts.get(line as usize).copied()
    }
}

#[derive(Debug, Default)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

impl SourceMap {
    pub fn add(&mut self, path: impl Into<String>, text: impl Into<String>) -> FileId {
        let id = FileId::new(offset(self.files.len()));
        self.files.push(SourceFile::new(path.into(), text.into()));
        id
    }

    /// Adds raw file contents. Invalid UTF-8 is stored lossily so the snippet can
    /// still be shown, and reported as `FLICK-S001` at the first bad sequence.
    pub fn add_bytes(
        &mut self,
        path: impl Into<String>,
        bytes: Vec<u8>,
    ) -> (FileId, Option<Diagnostic>) {
        match String::from_utf8(bytes) {
            Ok(text) => (self.add(path, text), None),
            Err(err) => {
                let utf8 = err.utf8_error();
                let start = utf8.valid_up_to();
                let len = utf8.error_len().unwrap_or(err.as_bytes().len() - start);
                let text = String::from_utf8_lossy(err.as_bytes()).into_owned();
                let file = self.add(path, text);
                let diagnostic = Diagnostic::error(
                    codes::S001,
                    "source file is not valid UTF-8",
                    Span::new(file, offset(start), offset(start + len)),
                );
                (file, Some(diagnostic))
            }
        }
    }

    pub fn get(&self, id: FileId) -> &SourceFile {
        &self.files[id.index() as usize]
    }

    /// The files in the order they were added.
    pub fn files(&self) -> impl Iterator<Item = &SourceFile> {
        self.files.iter()
    }
}

fn offset(n: usize) -> u32 {
    u32::try_from(n).expect("source offsets fit in u32")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(text: &str) -> SourceFile {
        SourceFile::new("t.fl".into(), text.into())
    }

    #[test]
    fn line_col_handles_empty_crlf_tail_and_non_ascii() {
        assert_eq!(file("").line_col(0), (0, 0));
        let f = file("ab\r\nцd\nx");
        assert_eq!(f.line_col(1), (0, 1));
        assert_eq!(f.line_col(4), (1, 0));
        assert_eq!(f.line_col(6), (1, 2), "column counts bytes: 'ц' is two");
        assert_eq!(f.line_col(8), (2, 0));
        assert_eq!(
            f.line_col(9),
            (2, 1),
            "end of file without trailing newline"
        );
        assert_eq!(f.line_start(2), Some(8));
        assert_eq!(f.line_start(3), None);
    }

    #[test]
    fn invalid_utf8_reports_first_bad_sequence() {
        let mut map = SourceMap::default();
        let (id, diag) = map.add_bytes("bad.fl", b"ok\n\xffz\xfe".to_vec());
        let diag = diag.expect("invalid UTF-8 is diagnosed");
        assert_eq!(diag.code, codes::S001);
        assert_eq!(diag.span, Some(Span::new(id, 3, 4)));
        assert_eq!(map.get(id).text, "ok\n\u{fffd}z\u{fffd}");

        let (_, diag) = map.add_bytes("ok.fl", b"fine".to_vec());
        assert!(diag.is_none());
    }
}
