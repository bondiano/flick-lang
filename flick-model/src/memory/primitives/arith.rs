#[expect(
    clippy::float_cmp,
    reason = "language primitive equality uses exact IEEE comparisons"
)]
fn arith_binary(
    heap: &mut Heap,
    op: Arith,
    num: Num,
    left: &Value,
    right: &Value,
) -> Result<Value, Error> {
    Ok(match num {
        Num::Int => {
            let (Value::Int(a), Value::Int(b)) = (left, right) else {
                return Err(unsupported("Int arithmetic type mismatch"));
            };
            Value::Int(int_arith(op, *a, *b))
        }
        Num::Int8 => {
            let (Value::Int8(a), Value::Int8(b)) = (left, right) else {
                return Err(unsupported("Int8 arithmetic type mismatch"));
            };
            Value::Int8(int8_arith(op, *a, *b))
        }
        Num::Int16 => {
            let (Value::Int16(a), Value::Int16(b)) = (left, right) else {
                return Err(unsupported("Int16 arithmetic type mismatch"));
            };
            Value::Int16(int16_arith(op, *a, *b))
        }
        Num::Int64 => {
            let (Value::Int64(a), Value::Int64(b)) = (left, right) else {
                return Err(unsupported("Int64 arithmetic type mismatch"));
            };
            Value::Int64(int64_arith(op, *a, *b))
        }
        Num::UInt8 => {
            let (Value::UInt8(a), Value::UInt8(b)) = (left, right) else {
                return Err(unsupported("UInt8 arithmetic type mismatch"));
            };
            Value::UInt8(uint8_arith(op, *a, *b))
        }
        Num::UInt16 => {
            let (Value::UInt16(a), Value::UInt16(b)) = (left, right) else {
                return Err(unsupported("UInt16 arithmetic type mismatch"));
            };
            Value::UInt16(uint16_arith(op, *a, *b))
        }
        Num::UInt32 => {
            let (Value::UInt32(a), Value::UInt32(b)) = (left, right) else {
                return Err(unsupported("UInt32 arithmetic type mismatch"));
            };
            Value::UInt32(uint32_arith(op, *a, *b))
        }
        Num::UInt64 => {
            let (Value::UInt64(a), Value::UInt64(b)) = (left, right) else {
                return Err(unsupported("UInt64 arithmetic type mismatch"));
            };
            Value::UInt64(uint64_arith(op, *a, *b))
        }
        Num::Float => {
            let (Value::Float(a), Value::Float(b)) = (left, right) else {
                return Err(unsupported("Float arithmetic type mismatch"));
            };
            Value::Float(match op {
                Arith::Add => a + b,
                Arith::Sub => a - b,
                Arith::Mul => a * b,
                Arith::Div => a / b,
                Arith::Rem => a % b,
                Arith::Pow if b.is_nan() || a.abs() == 1.0 && b.is_infinite() => f64::NAN,
                Arith::Pow => a.powf(*b),
            })
        }
        Num::Float32 => {
            let (Value::Float32(a), Value::Float32(b)) = (left, right) else {
                return Err(unsupported("Float32 arithmetic type mismatch"));
            };
            #[expect(
                clippy::cast_possible_truncation,
                reason = "binary32 pow rounds f64::powf once to f32, matching JS Math.fround and Go"
            )]
            let out = match op {
                Arith::Add => a + b,
                Arith::Sub => a - b,
                Arith::Mul => a * b,
                Arith::Div => a / b,
                Arith::Rem => a % b,
                Arith::Pow if b.is_nan() || a.abs() == 1.0 && b.is_infinite() => f32::NAN,
                Arith::Pow => f64::from(*a).powf(f64::from(*b)) as f32,
            };
            Value::Float32(out)
        }
        Num::BigInt => {
            // Standalone execute deliberately has no fuel or automatic cleanup.
            return bigint_binary(heap, op, left, right, |_| Ok(()));
        }
    })
}
