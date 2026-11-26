//! Counted `Json`, `Formatter`, `Inspectable`, and `Version` primitives.

use flick_core::ir::Lit;
use flick_core::ir::memory::JsonIntrinsic;
use flick_core::opt::{js_number, json_string};

use super::json_patch_ops;
use super::json_pointer_ops::{
    escape_token_op, format_uri_fragment_op, indent_json, parse_uri_fragment_op, unescape_token_op,
};
use super::record_ops::put_js_key;
use super::{Data, Error, Fuel, Heap, Value, charge, show_utf16_string};

pub(super) fn run(
    heap: &mut Heap,
    op: JsonIntrinsic,
    args: &[Value],
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    charge(fuel, 1)?;
    match op {
        JsonIntrinsic::Parse => parse_json(heap, &args[0], fuel),
        JsonIntrinsic::Stringify => {
            let text = stringify_value(heap, &args[0], fuel)?;
            heap.literal(&Lit::String(text))
        }
        JsonIntrinsic::FormatJson => format_json_op(heap, &args[0], &args[1], fuel),
        JsonIntrinsic::InspectToString => inspect_to_string(heap, &args[0], &args[1], fuel),
        JsonIntrinsic::EscapeToken => escape_token_op(heap, &args[0], fuel),
        JsonIntrinsic::UnescapeToken => unescape_token_op(heap, &args[0], fuel),
        JsonIntrinsic::ParseUriFragment => parse_uri_fragment_op(heap, &args[0], fuel),
        JsonIntrinsic::FormatUriFragment => format_uri_fragment_op(heap, &args[0], fuel),
        JsonIntrinsic::PatchGet => json_patch_ops::patch_get(heap, &args[0], &args[1], fuel),
        JsonIntrinsic::PatchApply => json_patch_ops::patch_apply(heap, &args[0], &args[1], fuel),
        JsonIntrinsic::VersionCurrent => heap.literal(&Lit::String("4.1.2".into())),
    }
}

fn format_json_op(
    heap: &mut Heap,
    indent_val: &Value,
    data: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let Value::Int(indent) = *indent_val else {
        return Err(Error::ExpectedInt);
    };
    let compact = stringify_value(heap, data, fuel)?;
    let out = if indent <= 0 {
        compact
    } else {
        indent_json(&compact, usize::try_from(indent).unwrap_or(0))
    };
    heap.literal(&Lit::String(out))
}

fn inspect_to_string(
    heap: &mut Heap,
    indent_val: &Value,
    data: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    if let Value::Object(tok) = data
        && matches!(heap.object(tok.id)?.data, Data::String(_) | Data::Utf16(_))
    {
        return heap.dup(data);
    }
    format_json_op(heap, indent_val, data, fuel)
}

pub(super) fn stringify_value(heap: &Heap, value: &Value, fuel: Fuel<'_>) -> Result<String, Error> {
    let mut out = String::new();
    write_json(heap, value, &mut out, fuel)?;
    Ok(out)
}

fn write_json(heap: &Heap, value: &Value, out: &mut String, fuel: Fuel<'_>) -> Result<(), Error> {
    charge(fuel, 1)?;
    match value {
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(i) => out.push_str(&i.to_string()),
        Value::Float(f) => {
            if f.is_nan() || f.is_infinite() {
                out.push_str("null");
            } else if *f == 0.0 {
                out.push('0');
            } else {
                out.push_str(&js_number(*f));
            }
        }
        Value::Object(tok) => match &heap.object(tok.id)?.data {
            Data::String(s) => {
                charge(fuel, s.len())?;
                out.push_str(&json_string(s));
            }
            Data::Utf16(units) => {
                charge(fuel, units.len())?;
                out.push_str(&show_utf16_string(units));
            }
            Data::Tagged { tag, fields, .. } if tag == "$list" => {
                write_json_list(heap, fields, out, fuel)?;
            }
            Data::ListSlice { .. } => {
                let (backing, offset, len) = heap.list_view(value)?;
                let Data::Tagged { fields, .. } = &heap.object(backing)?.data else {
                    return Err(Error::PatternKind("list"));
                };
                write_json_list(heap, &fields[offset..offset + len], out, fuel)?;
            }
            Data::Record(fields) => {
                out.push('{');
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    charge(fuel, k.len())?;
                    out.push_str(&json_string(k));
                    out.push(':');
                    write_json(heap, v, out, fuel)?;
                }
                out.push('}');
            }
            _ => out.push_str("null"),
        },
        _ => out.push_str("null"),
    }
    Ok(())
}

fn write_json_list(
    heap: &Heap,
    elems: &[Value],
    out: &mut String,
    fuel: Fuel<'_>,
) -> Result<(), Error> {
    out.push('[');
    for (i, elem) in elems.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        write_json(heap, elem, out, fuel)?;
    }
    out.push(']');
    Ok(())
}

pub(super) fn schema_error_single(
    heap: &mut Heap,
    path: &str,
    message: &str,
) -> Result<Value, Error> {
    let msg_val = heap.literal(&Lit::String(message.to_owned()))?;
    let path_str = if path.is_empty() { "(root)" } else { path };
    let path_val = match heap.literal(&Lit::String(path_str.to_owned())) {
        Ok(v) => v,
        Err(err) => {
            heap.drop_value(msg_val)?;
            return Err(err);
        }
    };
    let issue = heap.record(vec![("message".into(), msg_val), ("path".into(), path_val)])?;
    let issues = heap.construct("$list", vec![issue])?;
    let err = heap.construct("Prelude.SchemaError.SchemaError", vec![issues])?;
    heap.construct("Prelude.Result.Err", vec![err])
}

fn parse_json(heap: &mut Heap, text_val: &Value, fuel: Fuel<'_>) -> Result<Value, Error> {
    let text = heap.string_lossy(text_val)?;
    charge(fuel, text.len())?;
    let mut parser = JsonParser {
        s: &text,
        i: 0,
        problem: None,
    };
    parser.space();
    if let Some(val) = parser.value(heap, fuel)? {
        parser.space();
        if parser.i < parser.s.len() {
            heap.drop_value(val)?;
            let at = parser.at();
            let msg = format!("not JSON: Unexpected non-whitespace character after JSON {at}");
            schema_error_single(heap, "", &msg)
        } else {
            heap.construct("Prelude.Result.Ok", vec![val])
        }
    } else {
        let msg = format!("not JSON: {}", parser.reason());
        schema_error_single(heap, "", &msg)
    }
}

struct JsonParser<'a> {
    s: &'a str,
    i: usize,
    problem: Option<String>,
}

impl JsonParser<'_> {
    fn at(&self) -> String {
        let before = &self.s[..self.i.min(self.s.len())];
        let line = before.bytes().filter(|&b| b == b'\n').count() + 1;
        let last_nl = before.rfind('\n').map_or(0, |p| p + 1);
        let col = before[last_nl..].encode_utf16().count() + 1;
        let pos = before.encode_utf16().count();
        format!("at position {pos} (line {line} column {col})")
    }

    fn expect(&mut self, what: &str) {
        self.problem = Some(format!("{what} in JSON {}", self.at()));
    }

    fn reason(&self) -> String {
        if let Some(prob) = &self.problem {
            return prob.clone();
        }
        if self.i >= self.s.len() {
            return "Unexpected end of JSON input".into();
        }
        if matches!(self.s, "NaN" | "Infinity" | "undefined" | "[object Object]") {
            return format!("\"{}\" is not valid JSON", self.s);
        }
        let units: Vec<u16> = self.s.encode_utf16().collect();
        let pos = self.s[..self.i].encode_utf16().count();
        let slice_str = |a: usize, b: usize| String::from_utf16_lossy(&units[a..b]);
        let ctx = 10_usize;
        let source = if units.len() < 2 * ctx + 1 {
            format!("\"{}\"", self.s)
        } else if pos < ctx {
            format!("\"{}\"...", slice_str(0, pos + ctx))
        } else if pos < units.len() - ctx {
            format!("...\"{}\"...", slice_str(pos - ctx, pos + ctx))
        } else {
            format!("...\"{}\"", slice_str(pos - ctx, units.len()))
        };
        let tok = slice_str(pos, (pos + 1).min(units.len()));
        format!("Unexpected token '{tok}', {source} is not valid JSON")
    }

    fn space(&mut self) {
        let bytes = self.s.as_bytes();
        while self.i < bytes.len() && matches!(bytes[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }

    fn next_byte(&self) -> u8 {
        self.s.as_bytes().get(self.i).copied().unwrap_or(0)
    }

    fn value(&mut self, heap: &mut Heap, fuel: Fuel<'_>) -> Result<Option<Value>, Error> {
        charge(fuel, 1)?;
        if self.i >= self.s.len() {
            return Ok(None);
        }
        match self.next_byte() {
            b'{' => self.object(heap, fuel),
            b'[' => self.array(heap, fuel),
            b'"' => match self.string_units(fuel)? {
                Some(units) => Ok(Some(heap.alloc_utf16(units))),
                None => Ok(None),
            },
            b't' => Ok(self.literal("true").then_some(Value::Bool(true))),
            b'f' => Ok(self.literal("false").then_some(Value::Bool(false))),
            b'n' => Ok(self.literal("null").then(|| heap.allocate(Data::JsonNull))),
            b'-' | b'0'..=b'9' => Ok(self.number().map(Value::Float)),
            _ => Ok(None),
        }
    }

    fn literal(&mut self, word: &str) -> bool {
        let bytes = word.as_bytes();
        for &want in &bytes[1..] {
            self.i += 1;
            if self.i >= self.s.len() {
                return false;
            }
            let c = self.next_byte();
            if c == want {
                continue;
            }
            if c == b'"' {
                self.expect("Unexpected string");
            } else if c == b'-' || c.is_ascii_digit() {
                self.expect("Unexpected number");
            }
            return false;
        }
        self.i += 1;
        true
    }

    fn number(&mut self) -> Option<f64> {
        let start = self.i;
        let bytes = self.s.as_bytes();
        if self.next_byte() == b'-' {
            self.i += 1;
        }
        if self.next_byte() == b'0' {
            self.i += 1;
            if self.next_byte().is_ascii_digit() {
                self.expect("Unexpected number");
                return None;
            }
        } else if self.eat_digits() == 0 {
            self.expect("No number after minus sign");
            return None;
        }
        if self.i < bytes.len() && bytes[self.i] == b'.' {
            self.i += 1;
            if self.eat_digits() == 0 {
                self.expect("Unterminated fractional number");
                return None;
            }
        }
        if self.i < bytes.len() && matches!(bytes[self.i], b'e' | b'E') {
            self.i += 1;
            if self.i < bytes.len() && matches!(bytes[self.i], b'+' | b'-') {
                self.i += 1;
            }
            if self.eat_digits() == 0 {
                self.expect("Exponent part is missing a number");
                return None;
            }
        }
        Some(self.s[start..self.i].parse::<f64>().unwrap_or(f64::NAN))
    }

    fn eat_digits(&mut self) -> usize {
        let bytes = self.s.as_bytes();
        let start = self.i;
        while self.i < bytes.len() && bytes[self.i].is_ascii_digit() {
            self.i += 1;
        }
        self.i - start
    }

    fn string_units(&mut self, fuel: Fuel<'_>) -> Result<Option<Vec<u16>>, Error> {
        let out = self.quoted_units(fuel)?;
        if out.is_none() && self.problem.is_none() {
            self.expect("Unterminated string");
        }
        Ok(out)
    }

    fn quoted_units(&mut self, fuel: Fuel<'_>) -> Result<Option<Vec<u16>>, Error> {
        self.i += 1;
        let mut units = Vec::new();
        let bytes = self.s.as_bytes();
        while self.i < bytes.len() {
            charge(fuel, 1)?;
            let c = bytes[self.i];
            if c == b'"' {
                self.i += 1;
                return Ok(Some(units));
            }
            if c < 0x20 {
                self.expect("Bad control character in string literal");
                return Ok(None);
            }
            if c == b'\\' {
                if self.i + 1 >= bytes.len() {
                    self.problem = Some("Unexpected end of JSON input".into());
                    return Ok(None);
                }
                let esc = bytes[self.i + 1];
                self.i += 2;
                match esc {
                    b'"' | b'\\' | b'/' => units.push(u16::from(esc)),
                    b'b' => units.push(0x08),
                    b'f' => units.push(0x0c),
                    b'n' => units.push(u16::from(b'\n')),
                    b'r' => units.push(u16::from(b'\r')),
                    b't' => units.push(u16::from(b'\t')),
                    b'u' => {
                        let mut code = 0_u16;
                        for _ in 0..4 {
                            let Some(d) = hex_digit(bytes.get(self.i).copied()) else {
                                self.expect("Bad Unicode escape");
                                return Ok(None);
                            };
                            code = (code << 4) | u16::from(d);
                            self.i += 1;
                        }
                        units.push(code);
                    }
                    _ => {
                        self.i -= 1;
                        self.expect("Bad escaped character");
                        return Ok(None);
                    }
                }
            } else {
                let ch = self.s[self.i..].chars().next().expect("valid char");
                let mut buf = [0_u16; 2];
                units.extend_from_slice(ch.encode_utf16(&mut buf));
                self.i += ch.len_utf8();
            }
        }
        Ok(None)
    }

    fn array(&mut self, heap: &mut Heap, fuel: Fuel<'_>) -> Result<Option<Value>, Error> {
        self.i += 1;
        self.space();
        if self.next_byte() == b']' {
            self.i += 1;
            return Ok(Some(heap.construct("$list", Vec::new())?));
        }
        let mut items = Vec::new();
        loop {
            self.space();
            match self.value(heap, fuel) {
                Ok(Some(v)) => items.push(v),
                Ok(None) => {
                    heap.drop_values(items)?;
                    return Ok(None);
                }
                Err(err) => {
                    heap.drop_values(items)?;
                    return Err(err);
                }
            }
            self.space();
            match self.next_byte() {
                b',' => self.i += 1,
                b']' => {
                    self.i += 1;
                    return Ok(Some(heap.construct("$list", items)?));
                }
                _ => {
                    heap.drop_values(items)?;
                    self.expect("Expected ',' or ']' after array element");
                    return Ok(None);
                }
            }
        }
    }

    fn object(&mut self, heap: &mut Heap, fuel: Fuel<'_>) -> Result<Option<Value>, Error> {
        self.i += 1;
        self.space();
        if self.next_byte() == b'}' {
            self.i += 1;
            return Ok(Some(heap.record(Vec::new())?));
        }
        let mut fields: Vec<(String, Value)> = Vec::new();
        let mut first = true;
        loop {
            self.space();
            if self.next_byte() != b'"' {
                drop_pairs(heap, fields)?;
                self.expect(if first {
                    "Expected property name or '}'"
                } else {
                    "Expected double-quoted property name"
                });
                return Ok(None);
            }
            first = false;
            let key = match self.string_units(fuel) {
                Ok(Some(u)) => String::from_utf16_lossy(&u),
                Ok(None) => {
                    drop_pairs(heap, fields)?;
                    return Ok(None);
                }
                Err(err) => {
                    drop_pairs(heap, fields)?;
                    return Err(err);
                }
            };
            self.space();
            if self.next_byte() != b':' {
                drop_pairs(heap, fields)?;
                self.expect("Expected ':' after property name");
                return Ok(None);
            }
            self.i += 1;
            self.space();
            let val = match self.value(heap, fuel) {
                Ok(Some(v)) => v,
                Ok(None) => {
                    drop_pairs(heap, fields)?;
                    return Ok(None);
                }
                Err(err) => {
                    drop_pairs(heap, fields)?;
                    return Err(err);
                }
            };
            if let Some(old) = put_js_key(&mut fields, key, val) {
                heap.drop_value(old)?;
            }
            self.space();
            match self.next_byte() {
                b',' => self.i += 1,
                b'}' => {
                    self.i += 1;
                    return Ok(Some(heap.record(fields)?));
                }
                _ => {
                    drop_pairs(heap, fields)?;
                    self.expect("Expected ',' or '}' after property value");
                    return Ok(None);
                }
            }
        }
    }
}

fn drop_pairs(heap: &mut Heap, pairs: Vec<(String, Value)>) -> Result<(), Error> {
    heap.drop_values(pairs.into_iter().map(|(_, v)| v).collect())
}

fn hex_digit(byte: Option<u8>) -> Option<u8> {
    match byte? {
        b'0'..=b'9' => Some(byte? - b'0'),
        b'a'..=b'f' => Some(byte? - b'a' + 10),
        b'A'..=b'F' => Some(byte? - b'A' + 10),
        _ => None,
    }
}
