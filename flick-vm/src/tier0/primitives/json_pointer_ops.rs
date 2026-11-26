//! Counted `JsonPointer` (RFC 6901) and JSON indentation helpers.

use std::str;

use flick_core::ir::Lit;

use super::list_ops::slice_reps;
use super::{Data, Error, Heap, Value, charge};

pub(super) fn escape_token(s: &str) -> String {
    s.replace('~', "~0").replace('/', "~1")
}

pub(super) fn unescape_token(s: &str) -> String {
    s.replace("~1", "/").replace("~0", "~")
}

pub(super) fn escape_token_op(
    heap: &mut Heap,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let s = heap.string_lossy(val)?;
    charge(fuel, s.len())?;
    heap.literal(&Lit::String(escape_token(&s)))
}

pub(super) fn unescape_token_op(
    heap: &mut Heap,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let s = heap.string_lossy(val)?;
    charge(fuel, s.len())?;
    heap.literal(&Lit::String(unescape_token(&s)))
}

fn is_uri_unescaped(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b";,/?:@&=+$-_.!~*'()#".contains(&b)
}

pub(super) fn encode_uri(s: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        if is_uri_unescaped(b) {
            out.push(char::from(b));
        } else {
            out.push('%');
            out.push(char::from(HEX[usize::from(b >> 4)]));
            out.push(char::from(HEX[usize::from(b & 0xf)]));
        }
    }
    out
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

pub(super) fn decode_uri_component(s: &str) -> Option<String> {
    if !s.contains('%') {
        return Some(s.to_owned());
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        let mut run = Vec::new();
        while i < bytes.len() && bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return None;
            }
            let hi = hex_val(bytes[i + 1])?;
            let lo = hex_val(bytes[i + 2])?;
            run.push((hi << 4) | lo);
            i += 3;
        }
        if str::from_utf8(&run).is_err() {
            return None;
        }
        out.extend_from_slice(&run);
    }
    String::from_utf8(out).ok()
}

pub(super) fn parse_uri_fragment(fragment: &str) -> Option<Vec<String>> {
    if fragment.is_empty() || fragment == "#" {
        return Some(Vec::new());
    }
    let encoded = fragment.strip_prefix('#')?;
    let round = encode_uri(encoded).replace("%25", "%").replace('#', "%23");
    if round != encoded {
        return None;
    }
    let pointer = decode_uri_component(encoded)?;
    let rest = pointer.strip_prefix('/')?;
    let bytes = pointer.as_bytes();
    for (idx, &b) in bytes.iter().enumerate() {
        if b == b'~' && !matches!(bytes.get(idx + 1), Some(b'0' | b'1')) {
            return None;
        }
    }
    Some(rest.split('/').map(unescape_token).collect())
}

pub(super) fn parse_uri_fragment_op(
    heap: &mut Heap,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    if let Value::Object(tok) = val
        && matches!(heap.object(tok.id)?.data, Data::Utf16(_))
    {
        return heap.construct("Prelude.Option.None", vec![]);
    }
    let text = heap.string(val)?;
    charge(fuel, text.len())?;
    let Some(tokens) = parse_uri_fragment(text) else {
        return heap.construct("Prelude.Option.None", vec![]);
    };
    let mut vals = Vec::with_capacity(tokens.len());
    for tok in tokens {
        match heap.literal(&Lit::String(tok)) {
            Ok(v) => vals.push(v),
            Err(err) => {
                heap.drop_values(vals)?;
                return Err(err);
            }
        }
    }
    let list = heap.construct("$list", vals)?;
    heap.construct("Prelude.Option.Some", vec![list])
}

pub(super) fn format_uri_fragment(tokens: &[String]) -> String {
    let mut out = String::from("#");
    for tok in tokens {
        out.push('/');
        out.push_str(&encode_uri(&escape_token(tok)).replace('#', "%23"));
    }
    out
}

pub(super) fn format_uri_fragment_op(
    heap: &mut Heap,
    list_val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let reps = slice_reps(heap, list_val)?;
    charge(fuel, reps.len())?;
    let mut tokens = Vec::with_capacity(reps.len());
    for rep in &reps {
        let tok = heap.string_lossy(rep)?;
        charge(fuel, tok.len())?;
        tokens.push(tok);
    }
    heap.literal(&Lit::String(format_uri_fragment(&tokens)))
}

/// Indents a compact JSON string by `indent` spaces per nesting level (`encoding/json.Indent`).
pub(super) fn indent_json(compact: &str, indent: usize) -> String {
    let mut out = String::with_capacity(compact.len() * 2);
    let bytes = compact.as_bytes();
    let mut depth = 0_usize;
    let mut in_str = false;
    let mut escaped = false;
    let mut i = 0;
    while i < bytes.len() {
        let ch = compact[i..].chars().next().expect("valid char");
        let ch_len = ch.len_utf8();
        if in_str {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_str = false;
            }
            i += ch_len;
            continue;
        }
        match ch {
            '"' => {
                in_str = true;
                out.push('"');
            }
            '{' | '[' => {
                out.push(ch);
                let next_byte = bytes.get(i + 1).copied();
                if (ch == '{' && next_byte == Some(b'}')) || (ch == '[' && next_byte == Some(b']'))
                {
                    out.push(if ch == '{' { '}' } else { ']' });
                    i += 2;
                    continue;
                }
                depth += 1;
                out.push('\n');
                for _ in 0..depth * indent {
                    out.push(' ');
                }
            }
            '}' | ']' => {
                depth = depth.saturating_sub(1);
                out.push('\n');
                for _ in 0..depth * indent {
                    out.push(' ');
                }
                out.push(ch);
            }
            ',' => {
                out.push(',');
                out.push('\n');
                for _ in 0..depth * indent {
                    out.push(' ');
                }
            }
            ':' => out.push_str(": "),
            _ => out.push(ch),
        }
        i += ch_len;
    }
    out
}
