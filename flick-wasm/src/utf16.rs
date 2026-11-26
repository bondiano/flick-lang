//! UTF-8 to UTF-16 offset conversion for browser editor diagnostics and queries.

use serde_json::{Value, json};

use crate::DISPLAYED;

/// Rewrites each UTF-8 `range` in the module `text` of the diagnostic events
/// `events`, theirs, their related notes' and their fixes' edits, to UTF-16
/// offsets `from` and `to`; an event of the module also gets `at`, its
/// `line:column` from 1. Ranges in the built-in modules stay as they are.
pub fn positions(text: &str, events: &mut Value) {
    let Some(events) = events.as_array_mut() else {
        return;
    };
    for event in events {
        let start = convert(text, event);
        if let Some(start) = start {
            event["at"] = Value::String(line_column(text, start));
        }
        for key in ["related", "fixes"] {
            for item in event[key].as_array_mut().into_iter().flatten() {
                convert(text, item);
                for edit in item["edits"].as_array_mut().into_iter().flatten() {
                    convert(text, edit);
                }
            }
        }
    }
}

/// [`positions`] for one object with `path` and `range`; the UTF-8 start of
/// its range when it is in the module.
fn convert(text: &str, located: &mut Value) -> Option<u32> {
    if located["path"].as_str() != Some(DISPLAYED) {
        return None;
    }
    let range = located.get("range")?;
    let start = u32::try_from(range["start"].as_u64()?).ok()?;
    let end = u32::try_from(range["end"].as_u64()?).ok()?;
    located["from"] = json!(utf16_offset(text, start));
    located["to"] = json!(utf16_offset(text, end));
    Some(start)
}

/// The UTF-16 offset of the UTF-8 offset `byte` of `text`.
#[must_use]
pub fn utf16_offset(text: &str, byte: u32) -> u32 {
    let byte = floor_boundary(text, byte as usize);
    u32::try_from(text[..byte].encode_utf16().count()).unwrap_or(u32::MAX)
}

/// The UTF-8 offset of the UTF-16 offset `unit` of `text`; inside a
/// surrogate pair, the start of its character.
#[must_use]
pub fn byte_offset(text: &str, unit: u32) -> u32 {
    let mut units = 0;
    for (i, c) in text.char_indices() {
        units += c.len_utf16();
        if units > unit as usize {
            return u32::try_from(i).unwrap_or(u32::MAX);
        }
    }
    u32::try_from(text.len()).unwrap_or(u32::MAX)
}

fn floor_boundary(text: &str, byte: usize) -> usize {
    let mut byte = byte.min(text.len());
    while !text.is_char_boundary(byte) {
        byte -= 1;
    }
    byte
}

/// `line:column` from 1 of the UTF-8 offset `byte`, the column in UTF-16.
fn line_column(text: &str, byte: u32) -> String {
    let before = &text[..floor_boundary(text, byte as usize)];
    let line = before.matches('\n').count() + 1;
    let column = before
        .rsplit('\n')
        .next()
        .unwrap_or("")
        .encode_utf16()
        .count()
        + 1;
    format!("{line}:{column}")
}
