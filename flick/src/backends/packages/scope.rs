//! Path normalization and sandbox-boundary validation for host package
//! discovery.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::{env, fs};

use flick_backend::ProjectFile;

use super::slashed;

/// Strips a Windows verbatim prefix (`\\?\` or `\\?\UNC\`) before normalizing
/// path separators to `/` so canonical paths stay standard Windows drive or UNC
/// paths (`C:/...` or `//server/share/...`) instead of `//?/C:/...`.
pub(crate) fn normalize_canonical_str(raw: &str) -> String {
    let stripped = if let Some(unc) = raw.strip_prefix(r"\\?\UNC\") {
        format!("//{unc}")
    } else if let Some(local) = raw.strip_prefix(r"\\?\") {
        local.to_owned()
    } else if let Some(unc) = raw.strip_prefix("//?/UNC/") {
        format!("//{unc}")
    } else if let Some(local) = raw.strip_prefix("//?/") {
        local.to_owned()
    } else {
        raw.to_owned()
    };
    stripped.replace('\\', "/")
}

/// Canonical path of `path` with Windows verbatim prefixes stripped and `/` separators.
pub(crate) fn canonical_slashed(path: &Path) -> String {
    let real = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    normalize_canonical_str(&real.to_string_lossy())
}

fn has_windows_drive(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

pub(super) fn is_absolute_slashed(path: &str) -> bool {
    path.starts_with('/') || (has_windows_drive(path) && path.as_bytes().get(2) == Some(&b'/'))
}

/// Whether `path` is a non-empty relative path made solely of normal segments
/// without backslashes, NUL bytes, `.`, `..`, root, or drive prefixes.
pub(crate) fn is_safe_relative(path: &str) -> bool {
    if path.is_empty() || path.contains(['\\', '\0']) || has_windows_drive(path) {
        return false;
    }
    let mut has_normal = false;
    for component in Path::new(path).components() {
        match component {
            Component::Normal(_) => has_normal = true,
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => return false,
        }
    }
    has_normal
}

/// Whether `pattern` returned by `package-files` is a safe relative pattern
/// inside the project root (no hidden dot-segments, no traversal or absolute path).
pub(crate) fn is_safe_pattern(pattern: &str) -> bool {
    is_safe_relative(pattern) && pattern.split('/').all(|seg| !seg.starts_with('.'))
}

/// Whether `pattern` is a structured package-directory wildcard pattern
/// (`<dir>/<wildcard...>/<manifest>`) permitted for bounded ancestor lookups.
fn is_ancestor_pattern(pattern: &str) -> bool {
    let parts: Vec<&str> = pattern.split('/').collect();
    if parts.len() < 3 {
        return false;
    }
    let first = parts[0];
    let last = parts[parts.len() - 1];
    let middle = &parts[1..parts.len() - 1];
    !first.contains('*')
        && !first.starts_with('.')
        && !last.contains('*')
        && !last.starts_with('.')
        && middle.iter().all(|seg| seg.contains('*'))
}

fn segment_matches(pattern: &str, segment: &str) -> bool {
    match pattern.split_once('*') {
        None => pattern == segment,
        Some((prefix, suffix)) => {
            segment.len() >= prefix.len() + suffix.len()
                && segment.starts_with(prefix)
                && segment.ends_with(suffix)
        }
    }
}

fn pattern_matches(pattern: &str, rel: &Path) -> bool {
    let mut pat_parts = pattern.split('/');
    let mut rel_parts = rel.components();
    loop {
        match (pat_parts.next(), rel_parts.next()) {
            (Some(p), Some(Component::Normal(s))) => {
                let Some(s) = s.to_str() else {
                    return false;
                };
                if s.starts_with('.') || !segment_matches(p, s) {
                    return false;
                }
            }
            (None, None) => return true,
            _ => return false,
        }
    }
}

fn user_home_dir() -> Option<PathBuf> {
    let home = env::var_os("HOME").or_else(|| env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(canonical_slashed(Path::new(&home))))
}

fn is_forbidden_ancestor(dir: &Path, home: Option<&Path>, temp: Option<&Path>) -> bool {
    let text = slashed(dir);
    let trimmed = if text == "/" {
        "/"
    } else {
        text.trim_end_matches('/')
    };
    if trimmed.is_empty() || trimmed == "/" {
        return true;
    }
    let bytes = trimmed.as_bytes();
    if bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return true;
    }
    if let Some(rest) = trimmed.strip_prefix("//") {
        let mut parts = rest.split('/');
        if matches!(
            (parts.next(), parts.next(), parts.next()),
            (Some(_), _, None)
        ) {
            return true;
        }
    }
    // Depth-1 Unix system roots (`/Users`, `/home`, `/var`, `/private`, `/etc`, `/tmp`, ...).
    if let Some(rest) = trimmed.strip_prefix('/')
        && !rest.contains('/')
    {
        return true;
    }
    if home.is_some_and(|h| h.starts_with(dir)) {
        return true;
    }
    if temp.is_some_and(|t| t.starts_with(dir)) {
        return true;
    }
    false
}

/// Computes the highest ancestor directory that package discovery may walk up
/// to from `dir`: the nearest enclosing `.git` root, or the outermost enclosing
/// Flick workspace / package-store root strictly below `$HOME`, `temp_dir()`,
/// and system roots. Defaults to `dir` itself when no enclosing workspace exists.
pub(super) fn ancestor_ceiling(dir: &Path, top_dirs: &[String]) -> PathBuf {
    let home = user_home_dir();
    let temp = Some(PathBuf::from(canonical_slashed(&env::temp_dir())));
    let mut workspace: Option<PathBuf> = None;
    for anc in dir.ancestors() {
        if anc != dir && is_forbidden_ancestor(anc, home.as_deref(), temp.as_deref()) {
            break;
        }
        if anc.join(".git").exists() {
            return anc.to_path_buf();
        }
        let manifest = anc.join("flick.toml");
        if manifest.is_file()
            && fs::read_to_string(&manifest).is_ok_and(|text| text.contains("[workspace]"))
        {
            workspace = Some(anc.to_path_buf());
        }
    }
    if let Some(ws) = workspace {
        return ws;
    }
    // If `dir` itself lives inside a package store directory (e.g. a symlinked
    // virtual store), allow walking up to the parent of the outermost store dir.
    let mut store_root: Option<PathBuf> = None;
    for anc in dir.ancestors() {
        if anc != dir && is_forbidden_ancestor(anc, home.as_deref(), temp.as_deref()) {
            break;
        }
        if let Some(name) = anc.file_name().and_then(|n| n.to_str())
            && top_dirs.iter().any(|t| t == name)
            && let Some(parent) = anc.parent()
            && !is_forbidden_ancestor(parent, home.as_deref(), temp.as_deref())
        {
            store_root = Some(parent.to_path_buf());
        }
    }
    store_root.unwrap_or_else(|| dir.to_path_buf())
}

/// Whether `need` stays within the host's bounded package-discovery sandbox:
/// - no backslashes, NUL bytes, `.`, or `..`
/// - basename declared by one of `package-files`
/// - inside `real_root`, must match one of `patterns` (no dot-segments)
/// - outside `real_root`, must sit under a bounded ancestor `(base, ceiling)`
///   matching a structured wildcard `is_ancestor_pattern`.
pub(super) fn is_allowed_need(
    need: &str,
    real_root: &Path,
    patterns: &[String],
    names: &BTreeSet<&str>,
    supplied_bases: &BTreeSet<(PathBuf, PathBuf)>,
) -> bool {
    if need.is_empty() || need.contains(['\\', '\0']) {
        return false;
    }
    let path = Path::new(need);
    if path
        .components()
        .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return false;
    }
    let Some(name) = need.rsplit('/').next() else {
        return false;
    };
    if !names.contains(name) {
        return false;
    }
    if is_safe_relative(need) {
        return patterns.iter().any(|p| pattern_matches(p, path));
    }
    if !is_absolute_slashed(need) {
        return false;
    }
    for (base, ceiling) in supplied_bases {
        for ancestor in base.ancestors() {
            if !ancestor.starts_with(ceiling) {
                break;
            }
            if let Ok(rel) = path.strip_prefix(ancestor) {
                let matched = if ancestor == real_root {
                    patterns.iter().any(|p| pattern_matches(p, rel))
                } else {
                    patterns
                        .iter()
                        .any(|p| is_ancestor_pattern(p) && pattern_matches(p, rel))
                };
                if matched {
                    return true;
                }
            }
        }
    }
    false
}

pub(super) fn record_supplied(
    file: &ProjectFile,
    scope_ceiling: &Path,
    top_dirs: &[String],
    supplied_bases: &mut BTreeSet<(PathBuf, PathBuf)>,
    supplied_node_dirs: &mut BTreeSet<String>,
) {
    if file.real.is_empty() {
        return;
    }
    if let Some((parent, _)) = file.real.rsplit_once('/') {
        let dir = if parent.is_empty() { "/" } else { parent };
        let path = PathBuf::from(dir);
        let ceiling = if path.starts_with(scope_ceiling) {
            scope_ceiling.to_path_buf()
        } else {
            ancestor_ceiling(&path, top_dirs)
        };
        supplied_bases.insert((path, ceiling));
        supplied_node_dirs.insert(dir.to_owned());
    }
}
