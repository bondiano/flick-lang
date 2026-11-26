//! `BigInt` lexical normalization, ordering, and primitive display tests.

use std::cmp::Ordering;

use super::super::{Error, charge};

/// Converts a checked radix literal without imposing a machine-word bound.
pub(super) fn bigint_text(text: &str) -> Result<String, Error> {
    normalize_bigint(text, |_| Ok(()))
}

/// Pattern literals use numerical equality, with every normalization step
/// charged before it runs (including the quadratic radix conversion).
pub(in super::super) fn bigint_pattern_eq(
    left: &str,
    right: &str,
    fuel: &mut u64,
) -> Result<bool, Error> {
    let left = normalize_bigint(left, |work| charge(fuel, work))?;
    let right = normalize_bigint(right, |work| charge(fuel, work))?;
    charge(fuel, left.len().saturating_add(right.len()))?;
    Ok(left == right)
}

pub(super) fn normalize_bigint(
    text: &str,
    mut charge: impl FnMut(usize) -> Result<(), Error>,
) -> Result<String, Error> {
    charge(text.len())?;
    let (negative, unsigned) = text
        .strip_prefix('-')
        .map_or((false, text), |value| (true, value));
    let (base, digits) = if let Some(digits) = unsigned
        .strip_prefix("0x")
        .or_else(|| unsigned.strip_prefix("0X"))
    {
        (16, digits)
    } else if let Some(digits) = unsigned
        .strip_prefix("0o")
        .or_else(|| unsigned.strip_prefix("0O"))
    {
        (8, digits)
    } else if let Some(digits) = unsigned
        .strip_prefix("0b")
        .or_else(|| unsigned.strip_prefix("0B"))
    {
        (2, digits)
    } else {
        (10, unsigned)
    };
    if digits.is_empty() {
        return Err(Error::InvalidBigInt(text.into()));
    }
    let mut decimal = vec![0_u32];
    for digit in digits.chars() {
        charge(1)?;
        let digit = digit
            .to_digit(base)
            .ok_or_else(|| Error::InvalidBigInt(text.into()))?;
        let mut carry = digit;
        for place in &mut decimal {
            charge(1)?;
            let next = *place * base + carry;
            *place = next % 10;
            carry = next / 10;
        }
        while carry != 0 {
            charge(1)?;
            decimal.push(carry % 10);
            carry /= 10;
        }
    }
    while decimal.len() > 1 && decimal.last() == Some(&0) {
        charge(1)?;
        decimal.pop();
    }
    charge(decimal.len())?;
    let mut result = String::new();
    if negative && decimal != [0] {
        result.push('-');
    }
    result.extend(
        decimal
            .into_iter()
            .rev()
            .map(|digit| char::from_digit(digit, 10).expect("decimal digit")),
    );
    Ok(result)
}

pub(super) fn bigint_order(left: &str, right: &str) -> Ordering {
    match (left.strip_prefix('-'), right.strip_prefix('-')) {
        (Some(a), Some(b)) => b.len().cmp(&a.len()).then_with(|| b.cmp(a)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => left.len().cmp(&right.len()).then_with(|| left.cmp(right)),
    }
}

#[cfg(test)]
mod tests {
    use flick_core::ir::Lit;
    use flick_core::ir::memory::Intrinsic;

    use super::super::{Heap, intrinsic};
    use super::bigint_text;

    #[test]
    fn bigint_show_adds_n_without_changing_numeric_normalization_or_input_storage() {
        for (spelling, numeric, shown) in [
            ("0", "0", "0n"),
            ("-0", "0", "0n"),
            ("0x00", "0", "0n"),
            ("-0b0", "0", "0n"),
            ("123", "123", "123n"),
            ("-123", "-123", "-123n"),
            ("0xff", "255", "255n"),
            ("-0xff", "-255", "-255n"),
            ("0o17", "15", "15n"),
            ("-0o17", "-15", "-15n"),
            ("0b1010", "10", "10n"),
            ("-0b1010", "-10", "-10n"),
            (
                "0x100000000000000000000000000000000",
                "340282366920938463463374607431768211456",
                "340282366920938463463374607431768211456n",
            ),
        ] {
            assert_eq!(
                bigint_text(spelling),
                Ok(numeric.into()),
                "numeric normalization remains suffix-free"
            );
            let mut heap = Heap::new();
            let input = heap
                .literal(&Lit::BigInt(spelling.into()))
                .expect("BigInt input");
            let result = intrinsic(&mut heap, Intrinsic::Show, &input, &mut String::new())
                .expect("primitive Show BigInt");
            assert_eq!(heap.string(&result), Ok(shown));
            assert_eq!(
                heap.bigint(&input),
                Ok(spelling),
                "display must not mutate its borrowed input"
            );
            assert_eq!(
                heap.stats().live_objects,
                2,
                "input and displayed String have independent owned storage"
            );
            heap.drop_values(vec![input, result])
                .expect("release both owned roots");
            heap.finish().expect("BigInt and output String released");
        }
    }

    #[test]
    fn bigint_show_does_not_change_other_primitive_display_forms() {
        for (literal, expected) in [
            (Lit::Unit, "undefined"),
            (Lit::Bool(false), "false"),
            (Lit::Int(-7), "-7"),
            (Lit::Float(1.5), "1.5"),
            (Lit::String("quoted".into()), "\"quoted\""),
        ] {
            let mut heap = Heap::new();
            let input = heap.literal(&literal).expect("primitive input");
            let result = intrinsic(&mut heap, Intrinsic::Show, &input, &mut String::new())
                .expect("primitive Show");
            assert_eq!(heap.string(&result), Ok(expected));
            heap.drop_values(vec![input, result])
                .expect("primitive roots release");
            heap.finish().expect("ordinary primitive display balanced");
        }
    }
}
