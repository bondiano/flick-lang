//! JSON-RPC over stdio with `Content-Length` framing, and positions: Flick
//! counts UTF-8 bytes, LSP counts UTF-16 code units per line.

use std::fmt::Write as _;
use std::io::{self, BufRead, Write};
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

/// The LSP position of a byte offset.
pub fn position(text: &str, offset: u32) -> Value {
    let offset = (offset as usize).min(text.len());
    let before = &text[..offset];
    let line = before.matches('\n').count();
    let start = before.rfind('\n').map_or(0, |i| i + 1);
    let character: usize = text[start..offset].chars().map(char::len_utf16).sum();
    json!({ "line": line, "character": character })
}

pub fn range(text: &str, start: u32, end: u32) -> Value {
    json!({ "start": position(text, start), "end": position(text, end) })
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

/// The URI of a built-in module's source, a read-only document.
pub fn builtin_uri(module: &str) -> String {
    format!("flick-builtin:///{module}.fl")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_count_utf16_units() {
        let text = "a\nπ𝄞x\n";
        let x = u32::try_from(text.find('x').expect("x")).expect("small");
        assert_eq!(position(text, x), json!({ "line": 1, "character": 3 }));
        assert_eq!(offset(text, &json!({ "line": 1, "character": 3 })), x);
        assert_eq!(
            path_of(&uri_of(Path::new("/a b/ü.fl"))),
            Some(PathBuf::from("/a b/ü.fl"))
        );
    }
}
