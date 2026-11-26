//! ISAAC CSPRNG (bit-for-bit JS/Go parity), inert `CauseReport` formatting,
//! and cause/duration helpers for Stage 4A cooperative concurrency.

use super::outcome_ops::{describe_defect, expect_cause_reasons};
use super::{Data, DurationData, Error, Heap, Value, charge};
use crate::tier0::causes::{CauseToken, Reason, StoredReason};

/// Bit-for-bit port of `runtime-go/flickrt/random.go` `isaac` (`ISAAC_CSPRNG`).
#[derive(Clone, Debug)]
pub(in crate::tier0) struct IsaacRng {
    memory: [u32; 256],
    result: [u32; 256],
    accumulator: u32,
    last_result: u32,
    count: u32,
    generation: usize,
}

impl IsaacRng {
    pub(in crate::tier0) fn from_seed(seed: f64) -> Self {
        let mut r = Self {
            memory: [0; 256],
            result: [0; 256],
            accumulator: 0,
            last_result: 0,
            count: 0,
            generation: 0,
        };
        r.result[0] = js_uint32(seed);
        let mut s = [0x9e37_79b9_u32; 8];
        for _ in 0..4 {
            mix(&mut s);
        }
        let seed_words = r.result;
        for pass in 0..2 {
            let src = if pass == 0 { seed_words } else { r.memory };
            for i in (0..256).step_by(8) {
                for j in 0..8 {
                    s[j] = s[j].wrapping_add(src[i + j]);
                }
                mix(&mut s);
                r.memory[i..i + 8].copy_from_slice(&s);
            }
        }
        r.prng();
        r.generation = 256;
        r
    }

    fn prng(&mut self) {
        self.count = self.count.wrapping_add(1);
        self.last_result = self.last_result.wrapping_add(self.count);
        for i in 0..256 {
            match i & 3 {
                0 => self.accumulator ^= self.accumulator << 13,
                1 => self.accumulator ^= self.accumulator >> 6,
                2 => self.accumulator ^= self.accumulator << 2,
                _ => self.accumulator ^= self.accumulator >> 16,
            }
            self.accumulator = self.accumulator.wrapping_add(self.memory[(i + 128) & 0xff]);
            let x = self.memory[i];
            let m = self.memory[((x >> 2) & 0xff) as usize]
                .wrapping_add(self.accumulator)
                .wrapping_add(self.last_result);
            self.memory[i] = m;
            let res = self.memory[((m >> 10) & 0xff) as usize].wrapping_add(x);
            self.result[i] = res;
            self.last_result = res;
        }
    }

    fn next_u32(&mut self) -> u32 {
        if self.generation == 0 {
            self.prng();
            self.generation = 256;
        }
        self.generation -= 1;
        self.result[self.generation]
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "exact IEEE-754 53-bit double draw"
    )]
    pub(in crate::tier0) fn next_f64(&mut self) -> f64 {
        let hi = self.next_u32() & 0x1f_ffff;
        let lo = self.next_u32();
        (f64::from(hi) * 4_294_967_296.0 + f64::from(lo)) / ((1_u64 << 53) as f64)
    }
}

fn mix(s: &mut [u32; 8]) {
    s[0] ^= s[1] << 11;
    s[3] = s[3].wrapping_add(s[0]);
    s[1] = s[1].wrapping_add(s[2]);
    s[1] ^= s[2] >> 2;
    s[4] = s[4].wrapping_add(s[1]);
    s[2] = s[2].wrapping_add(s[3]);
    s[2] ^= s[3] << 8;
    s[5] = s[5].wrapping_add(s[2]);
    s[3] = s[3].wrapping_add(s[4]);
    s[3] ^= s[4] >> 16;
    s[6] = s[6].wrapping_add(s[3]);
    s[4] = s[4].wrapping_add(s[5]);
    s[4] ^= s[5] << 10;
    s[7] = s[7].wrapping_add(s[4]);
    s[5] = s[5].wrapping_add(s[6]);
    s[5] ^= s[6] >> 4;
    s[0] = s[0].wrapping_add(s[5]);
    s[6] = s[6].wrapping_add(s[7]);
    s[6] ^= s[7] << 8;
    s[1] = s[1].wrapping_add(s[6]);
    s[7] = s[7].wrapping_add(s[0]);
    s[7] ^= s[0] >> 9;
    s[2] = s[2].wrapping_add(s[7]);
    s[0] = s[0].wrapping_add(s[1]);
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "exact ECMAScript ToUint32 modulo 2^32"
)]
fn js_uint32(x: f64) -> u32 {
    if !x.is_finite() {
        return 0;
    }
    let m = x.trunc().rem_euclid(4_294_967_296.0);
    m as u64 as u32
}

/// Inert report matching `runtime-go/flickrt/failure.go:describeCause`.
pub(in crate::tier0) fn describe_cause_report(heap: &Heap, cause: &Value) -> Result<String, Error> {
    let reasons = expect_cause_reasons(heap, cause)?;
    let mut parts = Vec::with_capacity(reasons.len());
    for r in reasons {
        match r {
            StoredReason::Interrupt { .. } => parts.push("interrupt".to_owned()),
            StoredReason::Fail(val) => {
                let shown = ctor_name(heap, val).unwrap_or_else(|| "<object>".to_owned());
                parts.push(format!("fail {shown}"));
            }
            StoredReason::Die(val) => {
                if let Some(name) = ctor_name(heap, val) {
                    parts.push(format!("die {name}"));
                } else if let Some(raw) = raw_defect_string(heap, val)? {
                    let trimmed = raw.strip_suffix('\n').unwrap_or(&raw);
                    if let Some(msg) = strip_error_prefix(trimmed) {
                        parts.push(format!("die error {}", inert_text(msg)));
                    } else {
                        parts.push(format!("die {}", inert_text(trimmed)));
                    }
                } else {
                    let msg = describe_defect(heap, val)?;
                    parts.push(format!("die {}", inert_text(&msg)));
                }
            }
        }
    }
    Ok(parts.join("; "))
}

fn ctor_name(heap: &Heap, val: &Value) -> Option<String> {
    let Value::Object(tok) = val else {
        return None;
    };
    let Data::Tagged { tag, .. } = &heap.object(tok.id).ok()?.data else {
        return None;
    };
    if tag.starts_with('$') {
        return None;
    }
    Some(tag.rsplit('.').next().unwrap_or(tag).to_owned())
}

fn raw_defect_string(heap: &Heap, val: &Value) -> Result<Option<String>, Error> {
    let Value::Object(tok) = val else {
        return Ok(None);
    };
    Ok(match &heap.object(tok.id)?.data {
        Data::Defect(spec) => Some(spec.message()),
        Data::String(s) => Some(s.clone()),
        Data::Utf16(u) => Some(String::from_utf16_lossy(u)),
        _ => None,
    })
}

fn strip_error_prefix(s: &str) -> Option<&str> {
    for prefix in [
        "Error: ",
        "RangeError: ",
        "TypeError: ",
        "SyntaxError: ",
        "URIError: ",
        "EvalError: ",
        "ReferenceError: ",
    ] {
        if let Some(rest) = s.strip_prefix(prefix) {
            return Some(rest);
        }
    }
    None
}

fn inert_text(s: &str) -> String {
    let mut chars = s.chars();
    let prefix: String = chars.by_ref().take(200).collect();
    if chars.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
}

/// Strips all `Interrupt` reasons from a cause; returns `None` when nothing remains.
pub(in crate::tier0) fn without_interrupts_cause(
    heap: &mut Heap,
    cause: &CauseToken,
    fuel: &mut u64,
) -> Result<Option<CauseToken>, Error> {
    let reasons = heap.cause_data(cause)?;
    charge(fuel, reasons.len().saturating_add(1))?;
    let mut kept = Vec::new();
    for r in reasons {
        match r {
            StoredReason::Fail(v) => kept.push(StoredReason::Fail(v.representation())),
            StoredReason::Die(v) => kept.push(StoredReason::Die(v.representation())),
            StoredReason::Interrupt { .. } => {}
        }
    }
    if kept.is_empty() {
        return Ok(None);
    }
    let borrowed: Vec<Reason<'_>> = kept.iter().map(StoredReason::borrowed).collect();
    heap.cause(&borrowed, fuel).map(Some)
}

/// Turns every `Fail(v)` reason in `cause` into `Die(v)` (`Layer.or_die` / `Effect.or_die`).
pub(in crate::tier0) fn fails_to_dies_cause(
    heap: &mut Heap,
    cause: &CauseToken,
    fuel: &mut u64,
) -> Result<CauseToken, Error> {
    let reasons = heap.cause_data(cause)?;
    charge(fuel, reasons.len().saturating_mul(2).saturating_add(1))?;
    let mut converted = Vec::with_capacity(reasons.len());
    for r in reasons {
        match r {
            StoredReason::Fail(v) | StoredReason::Die(v) => {
                converted.push(StoredReason::Die(v.representation()));
            }
            StoredReason::Interrupt { fiber_id } => {
                converted.push(StoredReason::Interrupt {
                    fiber_id: *fiber_id,
                });
            }
        }
    }
    let borrowed: Vec<Reason<'_>> = converted.iter().map(StoredReason::borrowed).collect();
    heap.cause(&borrowed, fuel)
}

/// Converts a `Duration` value to virtual sleep nanoseconds:
/// - `Some(0)` -> non-positive (yield only)
/// - `Some(ns)` -> positive finite delay in nanoseconds
/// - `None` -> infinite (sleep forever until interrupted)
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "clamped finite positive millisecond duration to u64 nanoseconds"
)]
pub(in crate::tier0) fn duration_sleep_nanos(
    heap: &Heap,
    val: &Value,
) -> Result<Option<u64>, Error> {
    let Value::Object(tok) = val else {
        return Err(Error::UnsupportedPrimitive("expected Duration".into()));
    };
    let Data::Duration(d) = &heap.object(tok.id)?.data else {
        return Err(Error::UnsupportedPrimitive("expected Duration".into()));
    };
    Ok(match d {
        DurationData::Infinite => None,
        DurationData::NegativeInfinite => Some(0),
        DurationData::Millis(ms) => {
            if !ms.is_finite() {
                if *ms > 0.0 { None } else { Some(0) }
            } else if *ms <= 0.0 {
                Some(0)
            } else {
                let ns = (*ms * 1_000_000.0).round();
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "rounds u64::MAX / 2 to exact 2^63 f64 clamp"
                )]
                let max_ns = (u64::MAX / 2) as f64;
                if ns >= max_ns {
                    Some(u64::MAX / 2)
                } else {
                    Some((ns as u64).max(1))
                }
            }
        }
        DurationData::Nanos(bi) => {
            if bi <= &num_bigint::BigInt::ZERO {
                Some(0)
            } else {
                let ns = u64::try_from(bi).unwrap_or(u64::MAX / 2);
                Some(ns.min(u64::MAX / 2))
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isaac_seed_42_matches_js_and_go_first_draw() {
        let mut rng = IsaacRng::from_seed(42.0);
        assert_eq!(
            rng.next_f64().to_bits(),
            0.227_224_855_090_131_9_f64.to_bits()
        );
    }
}
