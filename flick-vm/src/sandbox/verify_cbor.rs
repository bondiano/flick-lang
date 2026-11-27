//! Iterative CBOR depth and item-count pre-validator for untrusted `.fvm`
//! sections.
//!
//! Runs before `ciborium` deserialization so crafted nested CBOR containers
//! cannot overflow the Rust stack or trigger large allocations.

use crate::image::ImageError;

/// Maximum CBOR container nesting depth accepted before deserialization.
pub const MAX_CBOR_DEPTH: usize = 96;

/// Maximum CBOR data items accepted per section.
const MAX_CBOR_ITEMS: usize = 2_000_000;

#[derive(Clone, Copy)]
enum CborFrame {
    Fixed(u64),
    Indefinite { is_map: bool, odd: bool },
}

/// Validates CBOR nesting depth and total item budget iteratively before
/// `ciborium` deserialization.
///
/// # Errors
/// Returns [`ImageError::Corrupt`] if the CBOR stream is malformed, truncated,
/// or exceeds [`MAX_CBOR_DEPTH`] / item budgets.
pub fn verify_cbor_budget(slice: &[u8], label: &str) -> Result<(), ImageError> {
    let mut pos = 0usize;
    let mut items = 0usize;
    let mut stack: Vec<CborFrame> = Vec::new();

    if slice.is_empty() {
        return Err(ImageError::Corrupt(format!(
            "{label} section is empty CBOR"
        )));
    }

    while pos < slice.len() {
        if items >= MAX_CBOR_ITEMS {
            return Err(ImageError::Corrupt(format!(
                "{label} section exceeds CBOR item budget ({MAX_CBOR_ITEMS})"
            )));
        }
        items += 1;

        let ib = slice[pos];
        pos += 1;

        if ib == 0xff {
            step_cbor_break(&mut stack, label)?;
            if stack.is_empty() {
                break;
            }
            continue;
        }

        let major = ib >> 5;
        let ai = ib & 0x1f;
        let arg = read_cbor_arg(slice, &mut pos, major, ai, label)?;
        step_cbor_major(slice, &mut pos, major, arg, &mut stack, label)?;

        if stack.is_empty() {
            break;
        }
    }

    if !stack.is_empty() {
        return Err(ImageError::Corrupt(format!(
            "{label} section has unterminated CBOR container"
        )));
    }
    if pos != slice.len() {
        return Err(ImageError::Corrupt(format!(
            "{label} section has {} trailing CBOR bytes",
            slice.len() - pos
        )));
    }
    Ok(())
}

fn step_cbor_break(stack: &mut Vec<CborFrame>, label: &str) -> Result<(), ImageError> {
    let Some(frame) = stack.pop() else {
        return Err(ImageError::Corrupt(format!(
            "{label} section has stray CBOR break byte"
        )));
    };
    match frame {
        CborFrame::Indefinite { is_map, odd } => {
            if is_map && odd {
                return Err(ImageError::Corrupt(format!(
                    "{label} section has odd-length indefinite CBOR map"
                )));
            }
        }
        CborFrame::Fixed(_) => {
            return Err(ImageError::Corrupt(format!(
                "{label} section has CBOR break inside definite container"
            )));
        }
    }
    consume_cbor_item(stack);
    Ok(())
}

fn step_cbor_major(
    slice: &[u8],
    pos: &mut usize,
    major: u8,
    arg: Option<u64>,
    stack: &mut Vec<CborFrame>,
    label: &str,
) -> Result<(), ImageError> {
    match major {
        0 | 1 | 7 => consume_cbor_item(stack),
        2 | 3 => match arg {
            Some(n) => {
                let len = usize::try_from(n).map_err(|_| {
                    ImageError::Corrupt(format!("{label} section CBOR string length overflow"))
                })?;
                let end = pos.checked_add(len).ok_or_else(|| {
                    ImageError::Corrupt(format!("{label} section CBOR string range overflow"))
                })?;
                if end > slice.len() {
                    return Err(ImageError::Corrupt(format!(
                        "{label} section truncated inside CBOR string"
                    )));
                }
                *pos = end;
                consume_cbor_item(stack);
            }
            None => push_cbor_frame(
                stack,
                CborFrame::Indefinite {
                    is_map: false,
                    odd: false,
                },
                label,
            )?,
        },
        4 => match arg {
            Some(0) => consume_cbor_item(stack),
            Some(n) => push_cbor_frame(stack, CborFrame::Fixed(n), label)?,
            None => push_cbor_frame(
                stack,
                CborFrame::Indefinite {
                    is_map: false,
                    odd: false,
                },
                label,
            )?,
        },
        5 => match arg {
            Some(0) => consume_cbor_item(stack),
            Some(n) => {
                let total = n.checked_mul(2).ok_or_else(|| {
                    ImageError::Corrupt(format!("{label} section CBOR map size overflow"))
                })?;
                push_cbor_frame(stack, CborFrame::Fixed(total), label)?;
            }
            None => push_cbor_frame(
                stack,
                CborFrame::Indefinite {
                    is_map: true,
                    odd: false,
                },
                label,
            )?,
        },
        6 => {
            if arg.is_none() {
                return Err(ImageError::Corrupt(format!(
                    "{label} section has indefinite CBOR tag"
                )));
            }
            push_cbor_frame(stack, CborFrame::Fixed(1), label)?;
        }
        _ => unreachable!("3-bit CBOR major type"),
    }
    Ok(())
}

fn push_cbor_frame(
    stack: &mut Vec<CborFrame>,
    frame: CborFrame,
    label: &str,
) -> Result<(), ImageError> {
    if stack.len() >= MAX_CBOR_DEPTH {
        return Err(ImageError::Corrupt(format!(
            "{label} section exceeds CBOR nesting limit ({MAX_CBOR_DEPTH})"
        )));
    }
    stack.push(frame);
    Ok(())
}

fn consume_cbor_item(stack: &mut Vec<CborFrame>) {
    while let Some(top) = stack.last_mut() {
        match top {
            CborFrame::Fixed(rem) => {
                *rem = rem.saturating_sub(1);
                if *rem == 0 {
                    stack.pop();
                } else {
                    break;
                }
            }
            CborFrame::Indefinite { is_map, odd } => {
                if *is_map {
                    *odd = !*odd;
                }
                break;
            }
        }
    }
}

fn read_cbor_arg(
    slice: &[u8],
    pos: &mut usize,
    major: u8,
    ai: u8,
    label: &str,
) -> Result<Option<u64>, ImageError> {
    match ai {
        0..=23 => Ok(Some(u64::from(ai))),
        24 => {
            let b = *slice.get(*pos).ok_or_else(|| {
                ImageError::Corrupt(format!("{label} section truncated in 1-byte CBOR arg"))
            })?;
            *pos += 1;
            Ok(Some(u64::from(b)))
        }
        25 => {
            let bytes: [u8; 2] = slice
                .get(*pos..*pos + 2)
                .ok_or_else(|| {
                    ImageError::Corrupt(format!("{label} section truncated in 2-byte CBOR arg"))
                })?
                .try_into()
                .map_err(|_| ImageError::Corrupt(format!("{label} section 2-byte CBOR arg")))?;
            *pos += 2;
            Ok(Some(u64::from(u16::from_be_bytes(bytes))))
        }
        26 => {
            let bytes: [u8; 4] = slice
                .get(*pos..*pos + 4)
                .ok_or_else(|| {
                    ImageError::Corrupt(format!("{label} section truncated in 4-byte CBOR arg"))
                })?
                .try_into()
                .map_err(|_| ImageError::Corrupt(format!("{label} section 4-byte CBOR arg")))?;
            *pos += 4;
            Ok(Some(u64::from(u32::from_be_bytes(bytes))))
        }
        27 => {
            let bytes: [u8; 8] = slice
                .get(*pos..*pos + 8)
                .ok_or_else(|| {
                    ImageError::Corrupt(format!("{label} section truncated in 8-byte CBOR arg"))
                })?
                .try_into()
                .map_err(|_| ImageError::Corrupt(format!("{label} section 8-byte CBOR arg")))?;
            *pos += 8;
            Ok(Some(u64::from_be_bytes(bytes)))
        }
        31 => {
            if matches!(major, 2..=5) {
                Ok(None)
            } else {
                Err(ImageError::Corrupt(format!(
                    "{label} section invalid indefinite CBOR major type {major}"
                )))
            }
        }
        _ => Err(ImageError::Corrupt(format!(
            "{label} section reserved CBOR additional info {ai}"
        ))),
    }
}
