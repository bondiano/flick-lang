use std::collections::BTreeSet;

use unicode_normalization::UnicodeNormalization;

use super::{Limits, string};

/// A portable relative path, with no aliases on Windows or normalization
/// aliases on macOS. Directory/file collisions are rejected as well.
pub(super) fn insert(
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
        let reserved = matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (base.starts_with("COM") || base.starts_with("LPT"))
                && base.len() == 4
                && matches!(base.as_bytes()[3], b'1'..=b'9');
        if part.is_empty() || matches!(part, "." | "..") || part.ends_with(['.', ' ']) || reserved {
            return Err(format!("invalid backend output path `{path}`"));
        }
    }
    let key = path.to_lowercase();
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
