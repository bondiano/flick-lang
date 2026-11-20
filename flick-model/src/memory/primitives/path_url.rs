//! File URL conversion and percent-encoding helpers for counted POSIX `Path` primitives.

use std::str;

use super::path_ops::path_resolve;

pub(super) fn path_from_file_url(url: &str) -> Result<String, String> {
    let trimmed = url.trim_matches(|c: char| c <= ' ');
    let clean: String = trimmed
        .chars()
        .filter(|&c| c != '\t' && c != '\n' && c != '\r')
        .collect();
    let Some(colon) = clean.find(':') else {
        return Err(format!("invalid URL {url}"));
    };
    let scheme = &clean[..colon];
    if scheme.is_empty()
        || !scheme.as_bytes()[0].is_ascii_alphabetic()
        || !scheme
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
    {
        return Err(format!("invalid URL {url}"));
    }
    if !scheme.eq_ignore_ascii_case("file") {
        return Err("URL must be of scheme file".into());
    }
    let rest = &clean[colon + 1..];
    let rest = rest.split('#').next().unwrap_or(rest);
    let rest = rest.split('?').next().unwrap_or(rest);
    let path_raw = if let Some(after) = rest
        .strip_prefix("//")
        .or_else(|| rest.strip_prefix("\\\\"))
        .or_else(|| rest.strip_prefix("/\\"))
        .or_else(|| rest.strip_prefix("\\/"))
    {
        let cut = after.find(['/', '\\']).unwrap_or(after.len());
        let host = &after[..cut];
        if !host.is_empty() && !host.eq_ignore_ascii_case("localhost") {
            return Err("Invalid file URL host".into());
        }
        &after[cut..]
    } else {
        rest
    };
    let normalized = normalize_url_path(path_raw);
    if normalized.to_ascii_lowercase().contains("%2f") {
        return Err("must not include encoded / characters".into());
    }
    decode_uri_component(&normalized).ok_or_else(|| format!("invalid URL {url}"))
}

pub(super) fn path_to_file_url(path: &str) -> String {
    let mut resolved = path_resolve(&[path.to_owned()]);
    if path.ends_with('/') && !resolved.ends_with('/') {
        resolved.push('/');
    }
    let mut replaced = String::with_capacity(resolved.len());
    for ch in resolved.chars() {
        match ch {
            '%' => replaced.push_str("%25"),
            '\\' => replaced.push_str("%5C"),
            '\n' => replaced.push_str("%0A"),
            '\r' => replaced.push_str("%0D"),
            '\t' => replaced.push_str("%09"),
            '?' => replaced.push_str("%3F"),
            '#' => replaced.push_str("%23"),
            _ => replaced.push(ch),
        }
    }
    format!("file://{}", normalize_url_path(&replaced))
}

fn normalize_url_path(s: &str) -> String {
    let s = s
        .strip_prefix('/')
        .or_else(|| s.strip_prefix('\\'))
        .unwrap_or(s);
    let mut segs: Vec<String> = Vec::new();
    let parts: Vec<&str> = s.split(['/', '\\']).collect();
    for (i, seg) in parts.iter().enumerate() {
        let last = i + 1 == parts.len();
        let lower = seg.to_ascii_lowercase();
        if matches!(lower.as_str(), ".." | ".%2e" | "%2e." | "%2e%2e") {
            segs.pop();
            if last {
                segs.push(String::new());
            }
        } else if matches!(lower.as_str(), "." | "%2e") {
            if last {
                segs.push(String::new());
            }
        } else {
            segs.push(percent_encode_path(seg));
        }
    }
    if segs.is_empty() {
        String::new()
    } else {
        format!("/{}", segs.join("/"))
    }
}

fn percent_encode_path(s: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        let encode = !(0x20..=0x7E).contains(&b)
            || matches!(
                b,
                b' ' | b'"' | b'#' | b'<' | b'>' | b'?' | b'^' | b'`' | b'{' | b'}'
            );
        if encode {
            out.push('%');
            out.push(char::from(HEX[usize::from(b >> 4)]));
            out.push(char::from(HEX[usize::from(b & 0xF)]));
        } else {
            out.push(char::from(b));
        }
    }
    out
}

fn decode_uri_component(s: &str) -> Option<String> {
    if !s.contains('%') {
        return Some(s.to_owned());
    }
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            let ch = s[i..].chars().next()?;
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }
        let mut run = Vec::new();
        while i < bytes.len() && bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return None;
            }
            let hi = hex_val(bytes[i + 1])?;
            let lo = hex_val(bytes[i + 2])?;
            run.push((hi << 4) | lo);
            i += 3;
        }
        let decoded = str::from_utf8(&run).ok()?;
        out.push_str(decoded);
    }
    Some(out)
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
