//! Exact OS error mapping and shared value-conversion helpers for VM host
//! services, matching `runtime-go/flickrt/{filesystem,file}.go` and
//! `runtime/src/rt/`.

use std::io::{self, ErrorKind};

use flick_core::ir::Lit;

use crate::tier0::{Data, Error, Heap, Value};

pub(super) use super::error_format::record_field;

/// Maps a Rust `io::Error` (and its raw OS errno when present) to the exact
/// `PlatformError.SystemErrorTag` constructor name.
pub(super) fn system_error_tag(err: &io::Error) -> &'static str {
    if let Some(code) = err.raw_os_error()
        && let Some(tag) = errno_to_tag(code)
    {
        return tag;
    }
    match err.kind() {
        ErrorKind::NotFound => "NotFound",
        ErrorKind::PermissionDenied => "PermissionDenied",
        ErrorKind::AlreadyExists => "AlreadyExists",
        ErrorKind::InvalidData => "InvalidData",
        ErrorKind::TimedOut => "TimedOut",
        ErrorKind::UnexpectedEof => "UnexpectedEof",
        ErrorKind::WouldBlock => "WouldBlock",
        ErrorKind::WriteZero => "WriteZero",
        ErrorKind::IsADirectory | ErrorKind::NotADirectory => "BadResource",
        ErrorKind::ResourceBusy | ErrorKind::ExecutableFileBusy => "Busy",
        _ => "UnknownError",
    }
}

#[cfg(unix)]
fn errno_to_tag(code: i32) -> Option<&'static str> {
    // Standard POSIX/Linux/macOS errnos.
    #[cfg(target_os = "linux")]
    const ELOOP: i32 = 40;
    #[cfg(target_os = "macos")]
    const ELOOP: i32 = 62;
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    const ELOOP: i32 = 40;

    #[cfg(target_os = "linux")]
    const ETIMEDOUT: i32 = 110;
    #[cfg(target_os = "macos")]
    const ETIMEDOUT: i32 = 60;
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    const ETIMEDOUT: i32 = 110;

    #[cfg(target_os = "linux")]
    const EAGAIN: i32 = 11;
    #[cfg(target_os = "macos")]
    const EAGAIN: i32 = 35;
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    const EAGAIN: i32 = 11;

    match code {
        2 => Some("NotFound"),                   // ENOENT
        1 | 13 | 30 => Some("PermissionDenied"), // EPERM, EACCES, EROFS
        17 => Some("AlreadyExists"),             // EEXIST
        9 | 20 | 21 => Some("BadResource"),      // EBADF, ENOTDIR, EISDIR
        c if c == ELOOP => Some("BadResource"),  // ELOOP
        16 | 26 => Some("Busy"),                 // EBUSY, ETXTBSY
        c if c == ETIMEDOUT => Some("TimedOut"), // ETIMEDOUT
        c if c == EAGAIN => Some("WouldBlock"),  // EAGAIN/EWOULDBLOCK
        _ => None,
    }
}

#[cfg(not(unix))]
fn errno_to_tag(_code: i32) -> Option<&'static str> {
    None
}

/// Formats an OS error like Node's `fs` error message (`ENOENT: no such file or directory, open '/x'`).
pub(super) fn node_message(err: &io::Error, call: &str, path: &str) -> String {
    let pair = match system_error_tag(err) {
        "NotFound" => Some(("ENOENT", "no such file or directory")),
        "PermissionDenied" => {
            if err.raw_os_error() == Some(30) {
                Some(("EROFS", "read-only file system"))
            } else {
                Some(("EACCES", "permission denied"))
            }
        }
        "AlreadyExists" => Some(("EEXIST", "file already exists")),
        "BadResource" => match err.kind() {
            ErrorKind::IsADirectory => Some(("EISDIR", "illegal operation on a directory")),
            ErrorKind::NotADirectory => Some(("ENOTDIR", "not a directory")),
            _ => Some(("EBADF", "bad file descriptor")),
        },
        _ => None,
    };
    if let Some((code, text)) = pair {
        if path.is_empty() {
            format!("{code}: {text}, {call}")
        } else {
            format!("{code}: {text}, {call} '{path}'")
        }
    } else {
        err.to_string()
    }
}

pub(super) fn ok(heap: &mut Heap, val: Value) -> Result<Value, Error> {
    heap.construct("Prelude.Result.Ok", vec![val])
}

pub(super) fn err(heap: &mut Heap, val: Value) -> Result<Value, Error> {
    heap.construct("Prelude.Result.Err", vec![val])
}

pub(super) fn opt_some(heap: &mut Heap, val: Value) -> Result<Value, Error> {
    heap.construct("Prelude.Option.Some", vec![val])
}

pub(super) fn opt_none(heap: &mut Heap) -> Result<Value, Error> {
    heap.construct("Prelude.Option.None", vec![])
}

pub(super) fn opt_str_val(heap: &mut Heap, s: Option<&str>) -> Result<Value, Error> {
    match s {
        Some(text) => {
            let v = heap.literal(&Lit::String(text.to_owned()))?;
            opt_some(heap, v)
        }
        None => opt_none(heap),
    }
}

pub(super) fn file_err(
    heap: &mut Heap,
    kind: &str,
    io_err: &io::Error,
    call: &str,
    path: &str,
) -> Result<Value, Error> {
    let msg = heap.literal(&Lit::String(node_message(io_err, call, path)))?;
    let payload = heap.construct(&format!("File.{kind}.{kind}"), vec![msg])?;
    err(heap, payload)
}

pub(super) fn platform_sys_err(
    heap: &mut Heap,
    io_err: &io::Error,
    module: &str,
    method: &str,
    syscall: Option<&str>,
    path: Option<&str>,
) -> Result<Value, Error> {
    platform_sys_err_tag(
        heap,
        system_error_tag(io_err),
        module,
        method,
        None,
        syscall,
        path,
    )
}

pub(super) fn platform_sys_err_tag(
    heap: &mut Heap,
    tag: &str,
    module: &str,
    method: &str,
    description: Option<&str>,
    syscall: Option<&str>,
    path: Option<&str>,
) -> Result<Value, Error> {
    let desc_v = opt_str_val(heap, description)?;
    let method_v = heap.literal(&Lit::String(method.to_owned()))?;
    let module_v = heap.literal(&Lit::String(module.to_owned()))?;
    let path_v = opt_str_val(heap, path)?;
    let syscall_v = opt_str_val(heap, syscall)?;
    let tag_v = heap.construct(&format!("PlatformError.SystemErrorTag.{tag}"), vec![])?;
    // Canonical alphabetical field order matching Flick's Data::Record invariant.
    let rec = heap.allocate(Data::Record(vec![
        ("description".into(), desc_v),
        ("method".into(), method_v),
        ("module_".into(), module_v),
        ("pathOrDescriptor".into(), path_v),
        ("syscall".into(), syscall_v),
        ("tag".into(), tag_v),
    ]));
    let reason = heap.construct("PlatformError.Reason.System", vec![rec])?;
    let pe = heap.construct("PlatformError.PlatformError.PlatformError", vec![reason])?;
    err(heap, pe)
}

pub(super) fn platform_bad_arg(
    heap: &mut Heap,
    module: &str,
    method: &str,
    description: &str,
) -> Result<Value, Error> {
    let desc_v = opt_str_val(heap, Some(description))?;
    let method_v = heap.literal(&Lit::String(method.to_owned()))?;
    let module_v = heap.literal(&Lit::String(module.to_owned()))?;
    let rec = heap.allocate(Data::Record(vec![
        ("description".into(), desc_v),
        ("method".into(), method_v),
        ("module_".into(), module_v),
    ]));
    let reason = heap.construct("PlatformError.Reason.Bad", vec![rec])?;
    let pe = heap.construct("PlatformError.PlatformError.PlatformError", vec![reason])?;
    err(heap, pe)
}

pub(super) fn check_no_nul(
    heap: &mut Heap,
    method: &str,
    paths: &[&str],
) -> Result<Option<Value>, Error> {
    for p in paths {
        if p.as_bytes().contains(&0) {
            return Ok(Some(platform_bad_arg(
                heap,
                "FileSystem",
                method,
                "path must not contain null bytes",
            )?));
        }
    }
    Ok(None)
}

pub(super) fn option_some<'a>(heap: &'a Heap, val: &'a Value) -> Result<Option<&'a Value>, Error> {
    let tag = heap.tag(val)?;
    if tag == "Prelude.Option.None" {
        Ok(None)
    } else if tag == "Prelude.Option.Some" {
        Ok(Some(heap.field(val, 0)?))
    } else {
        Err(Error::ExpectedOption)
    }
}

pub(super) fn expect_bool(val: &Value) -> Result<bool, Error> {
    match val {
        Value::Bool(b) => Ok(*b),
        _ => Err(Error::ExpectedBoolean),
    }
}

pub(super) fn expect_int(val: &Value) -> Result<i32, Error> {
    match val {
        Value::Int(n) => Ok(*n),
        _ => Err(Error::ExpectedInt),
    }
}

pub(super) fn expect_float(val: &Value) -> Result<f64, Error> {
    match val {
        Value::Float(f) => Ok(*f),
        Value::Int(n) => Ok(f64::from(*n)),
        _ => Err(Error::ExpectedFloat),
    }
}

pub(super) fn expect_bigint_i64(heap: &Heap, val: &Value) -> Result<i64, Error> {
    let text = heap.bigint(val)?;
    text.parse::<i64>()
        .map_err(|_| Error::InvalidBigInt(text.to_owned()))
}

pub(super) fn list_slice<'a>(heap: &'a Heap, val: &Value) -> Result<&'a [Value], Error> {
    let Value::Object(tok) = val else {
        return Err(Error::ExpectedObject);
    };
    match &heap.object(tok.id)?.data {
        Data::Tagged { tag, fields, .. } if tag == "$list" => Ok(fields.as_slice()),
        Data::ListSlice {
            backing,
            offset,
            len,
        } => {
            let Value::Object(btok) = backing else {
                return Err(Error::ExpectedObject);
            };
            let Data::Tagged { fields, .. } = &heap.object(btok.id)?.data else {
                return Err(Error::ExpectedObject);
            };
            Ok(&fields[*offset..*offset + *len])
        }
        _ => Err(Error::ExpectedObject),
    }
}

pub(super) fn bytes_from_int_list(heap: &Heap, val: &Value) -> Result<Vec<u8>, Error> {
    let items = list_slice(heap, val)?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let n = expect_int(item)?;
        out.push(u8::try_from(n & 0xff).unwrap_or(0));
    }
    Ok(out)
}

pub(super) fn int_list_from_bytes(heap: &mut Heap, bytes: &[u8]) -> Result<Value, Error> {
    let items: Vec<Value> = bytes.iter().map(|&b| Value::Int(i32::from(b))).collect();
    heap.construct("$list", items)
}

pub(super) fn string_list_to_vec(heap: &Heap, val: &Value) -> Result<Vec<String>, Error> {
    let items = list_slice(heap, val)?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        out.push(heap.string_lossy(item)?);
    }
    Ok(out)
}

pub(super) fn vec_to_string_list(heap: &mut Heap, items: Vec<String>) -> Result<Value, Error> {
    let mut vals = Vec::with_capacity(items.len());
    for s in items {
        match heap.literal(&Lit::String(s)) {
            Ok(v) => vals.push(v),
            Err(err) => {
                heap.drop_values(vals)?;
                return Err(err);
            }
        }
    }
    heap.construct("$list", vals)
}

/// Decodes UTF-8 bytes like JS `TextDecoder` / Go `decodeText`: strips leading
/// UTF-8 BOM (`EF BB BF`) and replaces invalid bytes with U+FFFD.
pub(super) fn decode_utf8_bom(bytes: &[u8]) -> String {
    let stripped = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    String::from_utf8_lossy(stripped).into_owned()
}

pub(super) fn handle_id(heap: &Heap, val: &Value, expected_tag: &str) -> Result<i64, Error> {
    let tag = heap.tag(val)?;
    if tag != expected_tag {
        return Err(Error::UnsupportedPrimitive(format!(
            "expected {expected_tag} handle, got {tag}"
        )));
    }
    match heap.field(val, 0)? {
        Value::Int64(id) => Ok(*id),
        _ => Err(Error::UnsupportedPrimitive(format!(
            "malformed {expected_tag} handle"
        ))),
    }
}
