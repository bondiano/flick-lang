//! Counted `Gen` mulberry32 PRNG primitives matching `runtime/src/core/testing.ts`
//! and `runtime-go/flickrt/testing.go` bit-for-bit.

use flick_core::ir::Lit;
use flick_core::ir::memory::GenIntrinsic;

use super::list_ops::slice_reps;
use super::{Error, Heap, Value, charge};

pub(super) fn run(
    heap: &mut Heap,
    op: GenIntrinsic,
    args: &[Value],
    fuel: &mut u64,
) -> Result<Value, Error> {
    charge(fuel, 1)?;
    match op {
        GenIntrinsic::Step => {
            let Value::Int(seed) = args[0] else {
                return Err(Error::ExpectedInt);
            };
            #[expect(
                clippy::cast_sign_loss,
                clippy::cast_possible_wrap,
                reason = "mulberry32 operates on 32-bit unsigned seed state"
            )]
            let (next, draw) = {
                let mut state = seed as u32;
                let r = mulberry_next(&mut state);
                (state as i32, r)
            };
            heap.construct("$tuple", vec![Value::Int(next), Value::Float(draw)])
        }
        GenIntrinsic::DrawString => draw_string(heap, &args[0], &args[1], fuel),
        GenIntrinsic::OneOfCheck => {
            let reps = slice_reps(heap, &args[0])?;
            if reps.is_empty() {
                return Err(Error::RangeError(
                    "Gen.one_of needs at least one value".into(),
                ));
            }
            heap.dup(&args[0])
        }
        GenIntrinsic::DrawOneOf => draw_one_of(heap, &args[0], &args[1]),
    }
}

#[expect(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    reason = "bounded index draw matching JS/Go mulberry32"
)]
fn draw_one_of(heap: &mut Heap, list_val: &Value, seed_val: &Value) -> Result<Value, Error> {
    let Value::Int(seed) = seed_val else {
        return Err(Error::ExpectedInt);
    };
    let reps = slice_reps(heap, list_val)?;
    if reps.is_empty() {
        return Err(Error::RangeError(
            "Gen.one_of needs at least one value".into(),
        ));
    }
    let mut state = *seed as u32;
    let r = mulberry_next(&mut state);
    let idx = ((r * reps.len() as f64).floor() as usize).min(reps.len() - 1);
    let chosen = heap.dup(&reps[idx])?;
    heap.construct("$tuple", vec![Value::Int(state as i32), chosen])
}

fn mulberry_next(state: &mut u32) -> f64 {
    *state = state.wrapping_add(0x6d2b_79f5);
    let mut t = *state;
    t = (t ^ (t >> 15)).wrapping_mul(t | 1);
    t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 0x3d));
    f64::from(t ^ (t >> 14)) / 4_294_967_296.0
}

#[expect(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "bounded printable ASCII draw matching JS/Go mulberry32"
)]
fn draw_string(
    heap: &mut Heap,
    max_val: &Value,
    seed_val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let (Value::Int(max), Value::Int(seed)) = (max_val, seed_val) else {
        return Err(Error::ExpectedInt);
    };
    let mut state = *seed as u32;
    let r0 = mulberry_next(&mut state);
    let len = (r0 * (f64::from(*max) + 1.0)).floor().max(0.0) as usize;
    charge(fuel, len)?;
    let mut out = String::with_capacity(len);
    for _ in 0..len {
        let r = mulberry_next(&mut state);
        let code = 32_u8.saturating_add((r * 95.0).floor() as u8);
        out.push(char::from(code));
    }
    let text = heap.literal(&Lit::String(out))?;
    heap.construct("$tuple", vec![Value::Int(state as i32), text])
}
