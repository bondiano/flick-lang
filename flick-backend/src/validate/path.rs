use std::collections::BTreeSet;

use caseless::Caseless;
use unicode_normalization::UnicodeNormalization;

use super::{Limits, string};

/// A portable relative path, with no aliases on Windows or normalization
/// aliases on macOS. Directory/file collisions are rejected as well.
pub(crate) fn insert(
    path: &str,
    paths: &mut BTreeSet<String>,
    limits: Limits,
) -> Result<(), String> {
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
        let base = part
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        // Windows also reserves the ISO-8859-1 superscript device digits:
        // https://learn.microsoft.com/windows/win32/fileio/naming-a-file
        let device_digit = base
            .strip_prefix("COM")
            .or_else(|| base.strip_prefix("LPT"));
        let reserved = matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || device_digit.is_some_and(|digit| {
                matches!(
                    digit,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            });
        if part.is_empty() || matches!(part, "." | "..") || part.ends_with(['.', ' ']) || reserved {
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
        .collect();
    let mut prefix = String::new();
    for part in key.split('/') {
        if !prefix.is_empty() {
            if paths.contains(&prefix) {
                return Err(format!(
                    "backend output path collides with a file: `{path}`"
                ));
            }
            prefix.push('/');
        }
        prefix.push_str(part);
    }
    if paths
        .range(format!("{key}/")..)
        .next()
        .is_some_and(|p| p.starts_with(&format!("{key}/")))
        || !paths.insert(key)
    {
        return Err(format!(
            "duplicate or colliding backend output path `{path}`"
        ));
    }
    Ok(())
}
