//! Minimal WHATWG URL parser and serializer for `Config.url` (matching `runtime-go/flickrt/url_*.go`).

use std::net::Ipv6Addr;

fn special_port(scheme: &str) -> Option<&'static str> {
    Some(match scheme {
        "http" | "ws" => "80",
        "https" | "wss" => "443",
        "ftp" => "21",
        "file" => "",
        _ => return None,
    })
}

fn c0_set(c: u8) -> bool {
    !(0x20..=0x7E).contains(&c)
}

fn fragment_set(c: u8) -> bool {
    c0_set(c) || matches!(c, b' ' | b'"' | b'<' | b'>' | b'`')
}

fn query_set(c: u8) -> bool {
    c0_set(c) || matches!(c, b' ' | b'"' | b'#' | b'<' | b'>')
}

fn special_query_set(c: u8) -> bool {
    query_set(c) || c == b'\''
}

fn path_set(c: u8) -> bool {
    query_set(c) || matches!(c, b'?' | b'^' | b'`' | b'{' | b'}')
}

fn userinfo_set(c: u8) -> bool {
    path_set(c)
        || matches!(
            c,
            b'/' | b':' | b';' | b'=' | b'@' | b'[' | b'\\' | b']' | b'|'
        )
}

fn percent_encode(s: &str, pred: fn(u8) -> bool) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if pred(b) {
            out.push('%');
            out.push(char::from(HEX[usize::from(b >> 4)]));
            out.push(char::from(HEX[usize::from(b & 0xF)]));
        } else {
            out.push(char::from(b));
        }
    }
    out
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(h), Some(l)) = (hex_nibble(bytes[i + 1]), hex_nibble(bytes[i + 2]))
        {
            out.push((h << 4) | l);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn scheme_end(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_alphabetic() {
        return None;
    }
    let mut i = 1;
    while i < bytes.len()
        && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'+' | b'-' | b'.'))
    {
        i += 1;
    }
    Some(i)
}

fn cut_port(s: &str) -> (&str, Option<&str>) {
    let mut inside = false;
    for (i, b) in s.bytes().enumerate() {
        match b {
            b'[' => inside = true,
            b']' => inside = false,
            b':' if !inside => return (&s[..i], Some(&s[i + 1..])),
            _ => {}
        }
    }
    (s, None)
}

fn parse_host(s: &str, special: bool) -> Option<String> {
    const FORBIDDEN: &str = "\x00\t\n\r #/:<>?@[\\]^|";
    if let Some(rest) = s.strip_prefix('[') {
        let inner = rest.strip_suffix(']')?;
        let addr: Ipv6Addr = inner.parse().ok()?;
        return Some(format!("[{addr}]"));
    }
    if !special {
        if s.chars().any(|c| FORBIDDEN.contains(c)) {
            return None;
        }
        return Some(percent_encode(s, c0_set));
    }
    if s.is_empty() {
        return Some(String::new());
    }
    let decoded = percent_decode(s);
    if !decoded.is_ascii() {
        return None;
    }
    let domain = decoded.to_ascii_lowercase();
    if domain.is_empty()
        || domain
            .bytes()
            .any(|b| b < 0x20 || b == b'%' || b == 0x7F || FORBIDDEN.as_bytes().contains(&b))
    {
        return None;
    }
    if ends_in_number(&domain) {
        return parse_ipv4(&domain);
    }
    Some(domain)
}

fn ends_in_number(domain: &str) -> bool {
    let mut labels: Vec<&str> = domain.split('.').collect();
    if labels.len() > 1 && labels.last() == Some(&"") {
        labels.pop();
    }
    let Some(&last) = labels.last() else {
        return false;
    };
    if last.is_empty() {
        return false;
    }
    if last.bytes().all(|b| b.is_ascii_digit()) {
        return true;
    }
    last.strip_prefix("0x")
        .or_else(|| last.strip_prefix("0X"))
        .is_some_and(|hex| hex.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn parse_ipv4_number(s: &str) -> Option<u64> {
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return if hex.is_empty() {
            Some(0)
        } else {
            u64::from_str_radix(hex, 16).ok()
        };
    }
    if s.len() > 1 && s.starts_with('0') {
        return u64::from_str_radix(&s[1..], 8).ok();
    }
    s.parse::<u64>().ok()
}

fn parse_ipv4(s: &str) -> Option<String> {
    let mut parts: Vec<&str> = s.split('.').collect();
    if parts.last() == Some(&"") {
        if parts.len() > 1 {
            parts.pop();
        } else {
            return None;
        }
    }
    if parts.len() > 4 {
        return None;
    }
    let mut nums = Vec::with_capacity(parts.len());
    for p in &parts {
        if p.is_empty() {
            return None;
        }
        nums.push(parse_ipv4_number(p)?);
    }
    for &n in &nums[..nums.len() - 1] {
        if n > 255 {
            return None;
        }
    }
    let last = *nums.last()?;
    let shift = 8 * (5 - nums.len());
    if last >= (1u64 << shift) {
        return None;
    }
    let mut ipv4 = last;
    for (i, &n) in nums[..nums.len() - 1].iter().enumerate() {
        ipv4 += n << (8 * (3 - i));
    }
    Some(format!(
        "{}.{}.{}.{}",
        (ipv4 >> 24) & 0xff,
        (ipv4 >> 16) & 0xff,
        (ipv4 >> 8) & 0xff,
        ipv4 & 0xff
    ))
}

fn parse_port(scheme: &str, s: &str) -> Option<String> {
    if s.is_empty() {
        return Some(String::new());
    }
    if !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let digits = s.trim_start_matches('0');
    if digits.len() > 5 {
        return None;
    }
    let port: u32 = if digits.is_empty() {
        0
    } else {
        digits.parse().ok()?
    };
    if port > 65535 {
        return None;
    }
    let p = port.to_string();
    if special_port(scheme) == Some(p.as_str()) {
        Some(String::new())
    } else {
        Some(p)
    }
}

fn normalize_path(s: &str, special: bool) -> String {
    if s.is_empty() && !special {
        return String::new();
    }
    let body = if s.starts_with('/') || (special && s.starts_with('\\')) {
        &s[1..]
    } else {
        s
    };
    let parts: Vec<&str> = if special {
        body.split(['/', '\\']).collect()
    } else {
        body.split('/').collect()
    };
    let mut out: Vec<String> = Vec::new();
    for (i, seg) in parts.iter().enumerate() {
        let last = i + 1 == parts.len();
        let lower = seg.to_ascii_lowercase();
        if matches!(lower.as_str(), ".." | ".%2e" | "%2e." | "%2e%2e") {
            out.pop();
            if last {
                out.push(String::new());
            }
        } else if matches!(lower.as_str(), "." | "%2e") {
            if last {
                out.push(String::new());
            }
        } else {
            out.push(percent_encode(seg, path_set));
        }
    }
    if out.is_empty() {
        String::new()
    } else {
        format!("/{}", out.join("/"))
    }
}

#[derive(Default)]
struct Authority {
    username: String,
    password: String,
    host: String,
    port: String,
}

enum UrlBody {
    WithAuthority { auth: Authority, path: String },
    PathOnly(String),
    Opaque(String),
}

fn parse_url_body(scheme: &str, special: bool, rest: &str) -> Option<UrlBody> {
    if scheme == "file" {
        if rest.len() < 2
            || !matches!(rest.as_bytes()[0], b'/' | b'\\')
            || !matches!(rest.as_bytes()[1], b'/' | b'\\')
        {
            return Some(UrlBody::WithAuthority {
                auth: Authority::default(),
                path: normalize_path(rest, true),
            });
        }
        let after = &rest[2..];
        let cut = after.find(['/', '\\', '?', '#']).unwrap_or(after.len());
        let (raw_auth, p) = (&after[..cut], &after[cut..]);
        let (h, pr) = cut_port(raw_auth);
        let mut host = parse_host(h, true)?;
        let mut port = String::new();
        if let Some(ps) = pr {
            port = parse_port(scheme, ps)?;
        }
        if host == "localhost" {
            host.clear();
        }
        return Some(UrlBody::WithAuthority {
            auth: Authority {
                username: String::new(),
                password: String::new(),
                host,
                port,
            },
            path: normalize_path(p, true),
        });
    }
    if special || rest.starts_with("//") {
        let after = if special {
            rest.trim_start_matches(['/', '\\'])
        } else {
            &rest[2..]
        };
        let cut = if special {
            after.find(['/', '\\', '?', '#']).unwrap_or(after.len())
        } else {
            after.find(['/', '?', '#']).unwrap_or(after.len())
        };
        let (mut raw_auth, p) = (&after[..cut], &after[cut..]);
        let mut username = String::new();
        let mut password = String::new();
        if let Some(at) = raw_auth.rfind('@') {
            let (user, pass) = raw_auth[..at]
                .split_once(':')
                .unwrap_or((&raw_auth[..at], ""));
            username = percent_encode(user, userinfo_set);
            password = percent_encode(pass, userinfo_set);
            raw_auth = &raw_auth[at + 1..];
            if raw_auth.is_empty() {
                return None;
            }
        }
        let (h, pr) = cut_port(raw_auth);
        if h.is_empty() && special {
            return None;
        }
        let host = parse_host(h, special)?;
        let mut port = String::new();
        if let Some(ps) = pr {
            port = parse_port(scheme, ps)?;
        }
        return Some(UrlBody::WithAuthority {
            auth: Authority {
                username,
                password,
                host,
                port,
            },
            path: normalize_path(p, special),
        });
    }
    if rest.starts_with('/') {
        Some(UrlBody::PathOnly(normalize_path(rest, false)))
    } else {
        Some(UrlBody::Opaque(percent_encode(rest, c0_set)))
    }
}

fn serialize_url(scheme: &str, body: &UrlBody, query: Option<&str>, frag: Option<&str>) -> String {
    let mut out = format!("{scheme}:");
    match body {
        UrlBody::WithAuthority { auth, path } => {
            out.push_str("//");
            if !auth.username.is_empty() || !auth.password.is_empty() {
                out.push_str(&auth.username);
                if !auth.password.is_empty() {
                    out.push(':');
                    out.push_str(&auth.password);
                }
                out.push('@');
            }
            out.push_str(&auth.host);
            if !auth.port.is_empty() {
                out.push(':');
                out.push_str(&auth.port);
            }
            out.push_str(path);
        }
        UrlBody::PathOnly(path) => {
            if path.starts_with("//") {
                out.push_str("/.");
            }
            out.push_str(path);
        }
        UrlBody::Opaque(path) => {
            out.push_str(path);
        }
    }
    if let Some(q) = query {
        out.push('?');
        out.push_str(q);
    }
    if let Some(f) = frag {
        out.push('#');
        out.push_str(f);
    }
    out
}

pub(super) fn parse_url_href(input: &str) -> Option<String> {
    let trimmed = input.trim_matches(|c: char| c <= ' ');
    let clean: String = trimmed
        .chars()
        .filter(|&c| c != '\t' && c != '\n' && c != '\r')
        .collect();
    let end = scheme_end(&clean)?;
    if clean.as_bytes().get(end) != Some(&b':') {
        return None;
    }
    let scheme = clean[..end].to_ascii_lowercase();
    let special = special_port(&scheme).is_some();
    let rest = &clean[end + 1..];
    let (rest, frag) = match rest.split_once('#') {
        Some((a, b)) => (a, Some(percent_encode(b, fragment_set))),
        None => (rest, None),
    };
    let (rest, query) = match rest.split_once('?') {
        Some((a, b)) => {
            let enc = if special {
                percent_encode(b, special_query_set)
            } else {
                percent_encode(b, query_set)
            };
            (a, Some(enc))
        }
        None => (rest, None),
    };
    let body = parse_url_body(&scheme, special, rest)?;
    Some(serialize_url(
        &scheme,
        &body,
        query.as_deref(),
        frag.as_deref(),
    ))
}
