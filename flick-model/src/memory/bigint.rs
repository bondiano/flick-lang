//! Exact signed decimal arithmetic; every digit pass is charged before work.
use std::cmp::Ordering;

use flick_core::numeric::Arith;

use super::{Error, normalize_bigint};

type Charge<'a> = dyn FnMut(usize) -> Result<(), Error> + 'a;
type Digits = Vec<u8>; // Little-endian base ten, canonical zero is [0].

struct Integer {
    negative: bool,
    digits: Digits,
}

impl Integer {
    fn parse(text: &str, charge: &mut Charge<'_>) -> Result<Self, Error> {
        let canonical = normalize_bigint(text, &mut *charge)?;
        let (negative, digits) = canonical
            .strip_prefix('-')
            .map_or((false, canonical.as_str()), |digits| (true, digits));
        charge(digits.len())?;
        Ok(Self {
            negative,
            digits: digits.bytes().rev().map(|digit| digit - b'0').collect(),
        })
    }

    fn render(mut self, charge: &mut Charge<'_>) -> Result<String, Error> {
        trim(&mut self.digits, charge)?;
        self.negative &= !zero(&self.digits);
        charge(self.digits.len().saturating_add(usize::from(self.negative)))?;
        let mut text = String::with_capacity(self.digits.len() + usize::from(self.negative));
        if self.negative {
            text.push('-');
        }
        text.extend(
            self.digits
                .into_iter()
                .rev()
                .map(|digit| char::from(b'0' + digit)),
        );
        Ok(text)
    }
}

pub(super) fn arithmetic(
    op: Arith,
    left: &str,
    right: &str,
    mut charge: impl FnMut(usize) -> Result<(), Error>,
) -> Result<String, Error> {
    let left = Integer::parse(left, &mut charge)?;
    let mut right = Integer::parse(right, &mut charge)?;
    if op == Arith::Sub && !zero(&right.digits) {
        right.negative = !right.negative;
    }
    let result = match op {
        Arith::Add | Arith::Sub => add(&left, &right, &mut charge)?,
        Arith::Mul => Integer {
            negative: left.negative != right.negative,
            digits: multiply(&left.digits, &right.digits, &mut charge)?,
        },
        Arith::Div | Arith::Rem => {
            let (quotient, remainder) = divide(&left.digits, &right.digits, &mut charge)?;
            Integer {
                negative: if op == Arith::Div {
                    left.negative != right.negative
                } else {
                    left.negative
                },
                digits: if op == Arith::Div {
                    quotient
                } else {
                    remainder
                },
            }
        }
        Arith::Pow => {
            if right.negative {
                return Err(Error::BigIntNegativeExponent);
            }
            Integer {
                negative: left.negative && !right.digits[0].is_multiple_of(2),
                digits: power(left.digits, right.digits, &mut charge)?,
            }
        }
    };
    result.render(&mut charge)
}

fn zero(digits: &[u8]) -> bool {
    digits == [0]
}

fn trim(digits: &mut Digits, charge: &mut Charge<'_>) -> Result<(), Error> {
    while digits.len() > 1 && digits.last() == Some(&0) {
        charge(1)?;
        digits.pop();
    }
    Ok(())
}

fn compare(left: &[u8], right: &[u8], charge: &mut Charge<'_>) -> Result<Ordering, Error> {
    charge(1)?;
    if left.len() != right.len() {
        return Ok(left.len().cmp(&right.len()));
    }
    charge(left.len())?;
    Ok(left.iter().rev().cmp(right.iter().rev()))
}

fn add(left: &Integer, right: &Integer, charge: &mut Charge<'_>) -> Result<Integer, Error> {
    if left.negative == right.negative {
        return Ok(Integer {
            negative: left.negative,
            digits: sum(&left.digits, &right.digits, charge)?,
        });
    }
    let order = compare(&left.digits, &right.digits, charge)?;
    let (larger, smaller) = if order == Ordering::Less {
        (right, left)
    } else {
        (left, right)
    };
    Ok(Integer {
        negative: larger.negative,
        digits: subtract(&larger.digits, &smaller.digits, charge)?,
    })
}

fn sum(left: &[u8], right: &[u8], charge: &mut Charge<'_>) -> Result<Digits, Error> {
    let places = left.len().max(right.len());
    charge(places.saturating_add(1))?;
    let mut digits = Vec::with_capacity(places + 1);
    let mut carry = 0;
    for index in 0..places {
        charge(1)?;
        let next =
            left.get(index).copied().unwrap_or(0) + right.get(index).copied().unwrap_or(0) + carry;
        digits.push(next % 10);
        carry = next / 10;
    }
    if carry != 0 {
        charge(1)?;
        digits.push(carry);
    }
    Ok(digits)
}

/// The first magnitude is at least the second.
fn subtract(left: &[u8], right: &[u8], charge: &mut Charge<'_>) -> Result<Digits, Error> {
    charge(left.len())?;
    let mut digits = Vec::with_capacity(left.len());
    let mut borrow = 0;
    for (index, digit) in left.iter().copied().enumerate() {
        charge(1)?;
        let needed = right.get(index).copied().unwrap_or(0) + borrow;
        borrow = u8::from(digit < needed);
        digits.push(digit + borrow * 10 - needed);
    }
    debug_assert_eq!(borrow, 0);
    trim(&mut digits, charge)?;
    Ok(digits)
}

fn multiply(left: &[u8], right: &[u8], charge: &mut Charge<'_>) -> Result<Digits, Error> {
    if zero(left) || zero(right) {
        charge(1)?;
        return Ok(vec![0]);
    }
    charge(left.len().saturating_add(right.len()))?;
    let mut digits = vec![0; left.len() + right.len()];
    for (i, a) in left.iter().copied().enumerate() {
        let mut carry = 0;
        for (j, b) in right.iter().copied().enumerate() {
            charge(1)?;
            let next = digits[i + j] + a * b + carry;
            digits[i + j] = next % 10;
            carry = next / 10;
        }
        charge(1)?;
        digits[i + right.len()] = carry;
    }
    trim(&mut digits, charge)?;
    Ok(digits)
}

fn divide(left: &[u8], right: &[u8], charge: &mut Charge<'_>) -> Result<(Digits, Digits), Error> {
    if zero(right) {
        charge(2)?;
        return Ok((vec![0], vec![0]));
    }
    charge(left.len().saturating_add(1))?;
    let mut quotient = Vec::with_capacity(left.len());
    let mut remainder = vec![0];
    for digit in left.iter().rev().copied() {
        // Inserting a decimal place shifts the whole current remainder.
        charge(remainder.len().saturating_mul(2).saturating_add(1))?;
        remainder.insert(0, digit);
        trim(&mut remainder, charge)?;
        let mut next = 0;
        while compare(&remainder, right, charge)? != Ordering::Less {
            remainder = subtract(&remainder, right, charge)?;
            next += 1;
        }
        debug_assert!(next < 10);
        charge(1)?;
        quotient.push(next);
    }
    charge(quotient.len())?;
    quotient.reverse();
    trim(&mut quotient, charge)?;
    Ok((quotient, remainder))
}

fn halve(digits: &mut Digits, charge: &mut Charge<'_>) -> Result<(), Error> {
    let mut carry = 0;
    for digit in digits.iter_mut().rev() {
        charge(1)?;
        let next = carry * 10 + *digit;
        *digit = next / 2;
        carry = next % 2;
    }
    trim(digits, charge)
}

fn power(mut base: Digits, mut exponent: Digits, charge: &mut Charge<'_>) -> Result<Digits, Error> {
    charge(1)?;
    let mut result = vec![1];
    while !zero(&exponent) {
        charge(1)?;
        if !exponent[0].is_multiple_of(2) {
            result = multiply(&result, &base, charge)?;
        }
        halve(&mut exponent, charge)?;
        if !zero(&exponent) {
            base = multiply(&base, &base, charge)?;
        }
    }
    Ok(result)
}
