//! JSON-RPC over stdio with `Content-Length` framing, and positions: Flick
//! counts UTF-8 bytes, LSP counts UTF-16 code units per line.

use std::fmt::Write as _;
use std::io::{self, BufRead, Write};
use std::iter;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// The next message, or `None` at the end of input.
pub fn read(input: &mut impl BufRead) -> io::Result<Option<Value>> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(n) = line.strip_prefix("Content-Length:") {
            length = n.trim().parse::<usize>().ok();
        }
    }
    let Some(length) = length else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "a message without Content-Length",
        ));
    };
    let mut body = vec![0; length];
    input.read_exact(&mut body)?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(io::Error::other)
}

pub fn write(output: &mut impl Write, message: &Value) -> io::Result<()> {
    let body = message.to_string();
    write!(output, "Content-Length: {}\r\n\r\n{body}", body.len())?;
    output.flush()
}

pub fn response(id: &Value, result: &Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

pub fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

pub fn notification(method: &str, params: &Value) -> Value {
    json!({ "jsonrpc": "2.0", "method": method, "params": params })
}

/// Where the lines of a text start, for the positions of many offsets in
/// it: the text is read once, and each position reads only its line.
pub struct Lines<'t> {
    text: &'t str,
    starts: Vec<usize>,
}

impl<'t> Lines<'t> {
    pub fn of(text: &'t str) -> Self {
        let starts = iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        Lines { text, starts }
    }

    /// The line and the UTF-16 column of a byte offset.
    pub fn line_column(&self, offset: u32) -> (u64, u64) {
        let offset = (offset as usize).min(self.text.len());
        let line = self.starts.partition_point(|&start| start <= offset) - 1;
        let units: usize = (self.text[self.starts[line]..offset].chars())
            .map(char::len_utf16)
            .sum();
        (line as u64, units as u64)
    }

    /// The LSP position of a byte offset.
    pub fn position(&self, offset: u32) -> Value {
        let (line, character) = self.line_column(offset);
        json!({ "line": line, "character": character })
    }

    pub fn range(&self, start: u32, end: u32) -> Value {
        json!({ "start": self.position(start), "end": self.position(end) })
    }
}

pub fn range(text: &str, start: u32, end: u32) -> Value {
    Lines::of(text).range(start, end)
}

/// The byte offset of an LSP position.
pub fn offset(text: &str, position: &Value) -> u32 {
    let number =
        |field: &str| usize::try_from(position[field].as_u64().unwrap_or(0)).unwrap_or(usize::MAX);
    let (line, character) = (number("line"), number("character"));
    let start = if line == 0 {
        0
    } else {
        text.match_indices('\n')
            .nth(line - 1)
            .map_or(text.len(), |(i, _)| i + 1)
    };
    let mut units = 0;
    let mut at = start;
    for c in text[start..].chars() {
        if units >= character || c == '\n' {
            break;
        }
        units += c.len_utf16();
        at += c.len_utf8();
    }
    u32::try_from(at).unwrap_or(u32::MAX)
}

/// The path of a `file://` URI.
pub fn path_of(uri: &str) -> Option<PathBuf> {
    let path = uri.strip_prefix("file://")?;
    let mut bytes = Vec::new();
    let mut it = path.bytes();
    while let Some(b) = it.next() {
        if b == b'%' {
            let hex: String = [it.next()?, it.next()?]
                .iter()
                .map(|&c| c as char)
                .collect();
            bytes.push(u8::from_str_radix(&hex, 16).ok()?);
        } else {
            bytes.push(b);
        }
    }
    Some(PathBuf::from(String::from_utf8(bytes).ok()?))
}

/// The `file://` URI of a path.
pub fn uri_of(path: &Path) -> String {
    let mut out = String::from("file://");
    for b in path.to_string_lossy().bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.~@".contains(&b) {
            out.push(b as char);
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_count_utf16_units() {
        let text = "a\nπ𝄞x\n";
        let x = u32::try_from(text.find('x').expect("x")).expect("small");
        assert_eq!(
            Lines::of(text).position(x),
            json!({ "line": 1, "character": 3 })
        );
        assert_eq!(offset(text, &json!({ "line": 1, "character": 3 })), x);
        assert_eq!(
            path_of(&uri_of(Path::new("/a b/ü.fl"))),
            Some(PathBuf::from("/a b/ü.fl"))
        );
    }
}
