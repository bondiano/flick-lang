use std::collections::BTreeSet;

use caseless::Caseless;
use unicode_normalization::UnicodeNormalization;

use super::{Limits, string};

/// Validate a portable relative path and return its canonical collision key.
fn canonical_key(path: &str, limits: Limits) -> Result<String, String> {
    string(path, limits)?;
    if path.is_empty()
        || path
            .chars()
            .any(|c| c.is_control() || matches!(c, '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*'))
        || !path.nfc().eq(path.chars())
    {
        return Err(format!("invalid backend output path `{path}`"));
    }
    for part in path.split('/') {
        // ext4 permits 255 bytes; HFS+ stores up to 255 decomposed UTF-16 units.
        // https://kernel.org/doc/html/latest/filesystems/ext4/directory.html
        // https://developer.apple.com/library/archive/technotes/tn/tn1150.html
        if part.len() > 255
            || !part.is_ascii() && part.nfd().map(char::len_utf16).sum::<usize>() > 255
        {
            return Err(format!(
                "backend output path component exceeds its portable length: `{path}`"
            ));
        }
        let base = part
            .split('.')
            .next()
            .unwrap_or_default()
            .trim_end_matches(' ')
            .to_ascii_uppercase();
        // Windows also reserves the ISO-8859-1 superscript device digits:
        // https://learn.microsoft.com/windows/win32/fileio/naming-a-file
        let device_digit = base
            .strip_prefix("COM")
            .or_else(|| base.strip_prefix("LPT"));
        let reserved = matches!(
            base.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
        ) || device_digit.is_some_and(|digit| {
            matches!(
                digit,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        });
        if part.is_empty()
            || matches!(part, "." | "..")
            || part.ends_with(['.', ' '])
            || reserved
            || dos_short_name(part)
        {
            return Err(format!("invalid backend output path `{path}`"));
        }
    }
    // Conservatively combine Unicode caseless matching with Windows'
    // uppercase aliases (including dotless i). Normalize after folding too.
    // https://www.unicode.org/faq/casemap_charprop.html
    let key: String = path
        .nfd()
        .flat_map(char::to_uppercase)
        .default_case_fold()
        .nfd()
        .filter(|c| !hfs_ignorable(*c))
        .collect();
    for part in key.split('/') {
        if part.is_empty() || matches!(part, "." | "..") {
            return Err(format!("invalid backend output path `{path}`"));
        }
    }
    Ok(key)
}

fn collides_with_file_prefix(key: &str, paths: &BTreeSet<String>) -> bool {
    let mut prefix = String::new();
    for part in key.split('/') {
        if !prefix.is_empty() {
            if paths.contains(&prefix) {
                return true;
            }
            prefix.push('/');
        }
        prefix.push_str(part);
    }
    false
}

fn collides_with_entry_or_dir(key: &str, paths: &BTreeSet<String>) -> bool {
    paths.contains(key)
        || paths
            .range(format!("{key}/")..)
            .next()
            .is_some_and(|p| p.starts_with(&format!("{key}/")))
}

/// A portable relative path, with no aliases on Windows or normalization
/// aliases on macOS. Directory/file collisions are rejected as well.
pub(crate) fn insert(
    path: &str,
    paths: &mut BTreeSet<String>,
    limits: Limits,
) -> Result<(), String> {
    let key = canonical_key(path, limits)?;
    if collides_with_file_prefix(&key, paths) {
        return Err(format!(
            "backend output path collides with a file: `{path}`"
        ));
    }
    if collides_with_entry_or_dir(&key, paths) {
        return Err(format!(
            "duplicate or colliding backend output path `{path}`"
        ));
    }
    paths.insert(key);
    Ok(())
}

/// Reject an output path that equals, aliases, or collides as a file/directory
/// prefix with an unowned module's declared layout path.
pub(crate) fn check_unowned(
    path: &str,
    unowned: &BTreeSet<String>,
    limits: Limits,
) -> Result<(), String> {
    if unowned.is_empty() {
        return Ok(());
    }
    let key = canonical_key(path, limits)?;
    if collides_with_file_prefix(&key, unowned) || collides_with_entry_or_dir(&key, unowned) {
        return Err(format!(
            "backend output path `{path}` violates declared module layout ownership"
        ));
    }
    Ok(())
}

/// The 16 formatting characters ignored by TN1150's `FastUnicodeCompare`.
/// <https://developer.apple.com/library/archive/technotes/tn/tn1150.html>
fn hfs_ignorable(character: char) -> bool {
    matches!(character, '\u{200c}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{206a}'..='\u{206f}' | '\u{feff}')
}

/// NTFS may assign these 8.3 aliases to otherwise unrelated long names.
fn dos_short_name(part: &str) -> bool {
    let (stem, extension) = part.split_once('.').unwrap_or((part, ""));
    let Some((_, digits)) = stem.rsplit_once('~') else {
        return false;
    };
    stem.encode_utf16().count() <= 8
        && extension.encode_utf16().count() <= 3
        && !digits.is_empty()
        && digits.bytes().all(|byte| byte.is_ascii_digit())
}
