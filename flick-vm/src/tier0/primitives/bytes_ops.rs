//! Counted `Bytes` primitive implementations (`packages/std/src/Bytes.fl`).

use std::str;

use base64::Engine as _;
use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use flick_core::ir::Lit;
use flick_core::ir::memory::BytesIntrinsic;

use super::list_ops::{expect_int, slice_reps};
use super::{Data, Error, Heap, Value, charge};

pub(super) fn run(
    heap: &mut Heap,
    op: BytesIntrinsic,
    args: &[Value],
    fuel: &mut u64,
) -> Result<Value, Error> {
    charge(fuel, 1)?;
    match op {
        BytesIntrinsic::Empty => Ok(heap.allocate(Data::Bytes(Vec::new()))),
        BytesIntrinsic::Length => {
            let len = heap.bytes(&args[0])?.len();
            Ok(Value::Int(i32::try_from(len).unwrap_or(i32::MAX)))
        }
        BytesIntrinsic::Get => {
            let idx = expect_int(&args[0])?;
            let byte = usize::try_from(idx)
                .ok()
                .and_then(|i| heap.bytes(&args[1]).ok().and_then(|b| b.get(i).copied()));
            // Validate operand type even when index is negative.
            let _ = heap.bytes(&args[1])?;
            match byte {
                Some(b) => heap.construct("Prelude.Option.Some", vec![Value::Int(i32::from(b))]),
                None => heap.construct("Prelude.Option.None", vec![]),
            }
        }
        BytesIntrinsic::Slice => bytes_slice(heap, &args[0], &args[1], &args[2], fuel),
        BytesIntrinsic::FromList => bytes_from_list(heap, &args[0], fuel),
        BytesIntrinsic::ToList => {
            let buf = heap.bytes(&args[0])?.to_vec();
            charge(fuel, buf.len())?;
            byte_list(heap, &buf)
        }
        BytesIntrinsic::EncodeUtf8 => {
            let text = heap.string_lossy(&args[0])?;
            charge(fuel, text.len())?;
            Ok(heap.allocate(Data::Bytes(text.into_bytes())))
        }
        BytesIntrinsic::DecodeUtf8 => {
            let buf = heap.bytes(&args[0])?.to_vec();
            charge(fuel, buf.len())?;
            decode_utf8_opt(heap, &buf)
        }
        BytesIntrinsic::EncodeBase64Url => {
            let buf = heap.bytes(&args[0])?.to_vec();
            charge(fuel, buf.len())?;
            let encoded = URL_SAFE_NO_PAD.encode(&buf);
            heap.literal(&Lit::String(encoded))
        }
        BytesIntrinsic::DecodeBase64Url => {
            let text = heap.string(&args[0])?.to_owned();
            charge(fuel, text.len())?;
            match decode_base64url(&text) {
                Some(bytes) => {
                    let val = heap.allocate(Data::Bytes(bytes));
                    heap.construct("Prelude.Option.Some", vec![val])
                }
                None => heap.construct("Prelude.Option.None", vec![]),
            }
        }
        BytesIntrinsic::EqualConstantTime => {
            let a = heap.bytes(&args[0])?;
            let b = heap.bytes(&args[1])?;
            charge(fuel, a.len().max(b.len()))?;
            let eq =
                a.len() == b.len() && a.iter().zip(b).fold(0_u8, |acc, (x, y)| acc | (x ^ y)) == 0;
            Ok(Value::Bool(eq))
        }
        BytesIntrinsic::LegacyUtf8Encode => {
            let text = heap.string_lossy(&args[0])?;
            charge(fuel, text.len())?;
            byte_list(heap, text.as_bytes())
        }
        BytesIntrinsic::LegacyUtf8Decode => {
            let Some(buf) = list_to_bytes_unchecked(heap, &args[0], fuel)? else {
                return heap.construct("Prelude.Option.None", vec![]);
            };
            decode_utf8_opt(heap, &buf)
        }
        BytesIntrinsic::LegacyBase64UrlEncode => {
            let buf = list_to_bytes_unchecked(heap, &args[0], fuel)?.unwrap_or_default();
            let encoded = URL_SAFE_NO_PAD.encode(&buf);
            heap.literal(&Lit::String(encoded))
        }
        BytesIntrinsic::LegacyBase64UrlDecode => {
            let text = heap.string(&args[0])?.to_owned();
            charge(fuel, text.len())?;
            match decode_base64url(&text) {
                Some(bytes) => {
                    let list = byte_list(heap, &bytes)?;
                    heap.construct("Prelude.Option.Some", vec![list])
                }
                None => heap.construct("Prelude.Option.None", vec![]),
            }
        }
    }
}

fn bytes_slice(
    heap: &mut Heap,
    start_val: &Value,
    end_val: &Value,
    bytes_val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let start = expect_int(start_val)?;
    let end = expect_int(end_val)?;
    let buf = heap.bytes(bytes_val)?;
    let (Ok(lo), Ok(hi)) = (usize::try_from(start), usize::try_from(end)) else {
        return heap.construct("Prelude.Option.None", vec![]);
    };
    if hi < lo || hi > buf.len() {
        return heap.construct("Prelude.Option.None", vec![]);
    }
    let slice = buf[lo..hi].to_vec();
    charge(fuel, slice.len())?;
    let val = heap.allocate(Data::Bytes(slice));
    heap.construct("Prelude.Option.Some", vec![val])
}

fn bytes_from_list(heap: &mut Heap, list_val: &Value, fuel: &mut u64) -> Result<Value, Error> {
    let reps = slice_reps(heap, list_val)?;
    charge(fuel, reps.len())?;
    let mut out = Vec::with_capacity(reps.len());
    for (idx, item) in reps.iter().enumerate() {
        let n = expect_int(item)?;
        if !(0..=255).contains(&n) {
            let pair = heap.construct(
                "$tuple",
                vec![
                    Value::Int(i32::try_from(idx).unwrap_or(i32::MAX)),
                    Value::Int(n),
                ],
            )?;
            return heap.construct("Prelude.Result.Err", vec![pair]);
        }
        out.push(u8::try_from(n).expect("0..=255"));
    }
    let val = heap.allocate(Data::Bytes(out));
    heap.construct("Prelude.Result.Ok", vec![val])
}

fn list_to_bytes_unchecked(
    heap: &Heap,
    list_val: &Value,
    fuel: &mut u64,
) -> Result<Option<Vec<u8>>, Error> {
    let reps = slice_reps(heap, list_val)?;
    charge(fuel, reps.len())?;
    let mut out = Vec::with_capacity(reps.len());
    for item in &reps {
        let n = expect_int(item)?;
        if !(0..=255).contains(&n) {
            return Ok(None);
        }
        out.push(u8::try_from(n).expect("0..=255"));
    }
    Ok(Some(out))
}

fn byte_list(heap: &mut Heap, bytes: &[u8]) -> Result<Value, Error> {
    let fields = bytes.iter().map(|&b| Value::Int(i32::from(b))).collect();
    heap.construct("$list", fields)
}

fn decode_utf8_opt(heap: &mut Heap, bytes: &[u8]) -> Result<Value, Error> {
    match str::from_utf8(bytes) {
        Ok(text) => {
            let s = heap.literal(&Lit::String(text.to_owned()))?;
            heap.construct("Prelude.Option.Some", vec![s])
        }
        Err(_) => heap.construct("Prelude.Option.None", vec![]),
    }
}

fn decode_base64url(input: &str) -> Option<Vec<u8>> {
    if input
        .bytes()
        .any(|b| b == b'\r' || b == b'\n' || b == b' ' || b == b'\t')
    {
        return None;
    }
    if input.ends_with('=') {
        URL_SAFE.decode(input).ok()
    } else {
        URL_SAFE_NO_PAD.decode(input).ok()
    }
}
