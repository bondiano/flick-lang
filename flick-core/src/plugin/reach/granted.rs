//! What a project's own build reaches for its plugins: each call checked
//! against the grants, asked of the host's [`Outside`], and recorded with
//! the hash of its answer.

use std::fmt::Write as _;
use std::net::Ipv6Addr;
use std::path::Path;

use flick_plugin::{ReachError, Request, Response};
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::{Asked, Reach, SNAPSHOTS, Used, inside};
use crate::diagnostics::codes;

/// The hex of the hash of `parts`, each delimited.
pub fn hash<'a>(parts: impl IntoIterator<Item = &'a [u8]>) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(u64::try_from(part.len()).unwrap_or(u64::MAX).to_le_bytes());
        hasher.update(part);
    }
    hasher.finalize().iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

fn answer<T: Serialize>(value: &T) -> String {
    hash([serde_json::to_vec(value).unwrap_or_default().as_slice()])
}

impl Reach {
    fn denied(&mut self, package: &str, what: &str, line: &str) -> ReachError {
        let message = format!(
            "plugin `{package}` asks {what}, which this project does not grant: add `{line}` to `[plugins.{package}]` in flick.toml"
        );
        self.problems.push((codes::P038, message.clone()));
        ReachError::Denied(message)
    }

    pub fn read_file(&mut self, path: &str) -> Result<Vec<u8>, ReachError> {
        let gate = self.gate()?;
        let granted =
            inside(path) && (gate.grants.fs_read.iter()).any(|g| Path::new(path).starts_with(g));
        if !granted {
            return Err(self.denied(
                &gate.package,
                &format!("to read `{path}`"),
                &format!("fs-read = [\"{path}\"]"),
            ));
        }
        let read = gate
            .outside
            .read(&gate.root.join(path))
            .map_err(|e| e.to_string());
        self.used.push(Used {
            asked: Asked::File(path.to_owned()),
            answer: answer(&read),
        });
        read.map_err(|e| ReachError::Failed(format!("`{path}`: {e}")))
    }

    pub fn env(&mut self, name: &str) -> Result<Option<String>, ReachError> {
        let gate = self.gate()?;
        if !gate.grants.env.iter().any(|g| g == name) {
            return Err(self.denied(
                &gate.package,
                &format!("for `{name}`"),
                &format!("env = [\"{name}\"]"),
            ));
        }
        let value = gate.outside.env(name);
        self.used.push(Used {
            asked: Asked::Env(name.to_owned()),
            answer: answer(&value),
        });
        Ok(value)
    }

    pub fn fetch(&mut self, request: &Request) -> Result<Response, ReachError> {
        let gate = self.gate()?;
        let what = format!("`{} {}`", request.method, request.url);
        let Some(host) = authority(&request.url) else {
            let message = format!(
                "plugin `{}` fetches {what}, which is no `http` or `https` URL a project can grant",
                gate.package
            );
            self.problems.push((codes::P038, message.clone()));
            return Err(ReachError::Denied(message));
        };
        if !gate.grants.net.iter().any(|g| grants_host(g, &host)) {
            let name = net_endpoint(&host).map(|(name, _)| name).unwrap_or(host);
            return Err(self.denied(
                &gate.package,
                &format!("to fetch {what}"),
                &format!("net = [\"{name}\"]"),
            ));
        }
        let key = snapshot_key(request);
        let snapshot = gate
            .root
            .join(SNAPSHOTS)
            .join(&gate.package)
            .join(format!("{key}.json"));
        let answered = gate.outside.fetch(&snapshot, request);
        if let Err(ReachError::Offline(message)) = &answered {
            self.problems.push((codes::P039, message.clone()));
        }
        if let Ok(response) = &answered {
            self.used.push(Used {
                asked: Asked::Fetch(key),
                answer: answer(response),
            });
        }
        answered
    }

    /// Whether every answer `used` records is still the same: a kept
    /// result holds then. A snapshot gone is a change; the network is not
    /// asked.
    pub fn still(&self, used: &[Used]) -> bool {
        let Some(gate) = self.gate.as_deref() else {
            return used.is_empty();
        };
        used.iter().all(|u| {
            let now = match &u.asked {
                Asked::File(path) => answer(
                    &gate
                        .outside
                        .read(&gate.root.join(path))
                        .map_err(|e| e.to_string()),
                ),
                Asked::Env(name) => answer(&gate.outside.env(name)),
                // the block's source answers these: `BlockSource::still`
                Asked::LiteralOf(_) | Asked::Published(_) | Asked::PublishedNames => return false,
                Asked::Fetch(key) => {
                    let at = gate
                        .root
                        .join(SNAPSHOTS)
                        .join(&gate.package)
                        .join(format!("{key}.json"));
                    match gate.outside.replay(&at) {
                        Some(response) => answer(&response),
                        None => return false,
                    }
                }
            };
            now == u.answer
        })
    }
}

/// The key of a request's snapshot: its method, URL and body; headers are
/// left out, so a token sent in one is no part of what is committed.
pub fn snapshot_key(request: &Request) -> String {
    hash([
        request.method.as_bytes(),
        request.url.as_bytes(),
        request.body.as_slice(),
    ])
}

/// The host and port of an `http` or `https` URL, in the form a grant
/// names; none for another URL, or one whose authority hides its host.
fn authority(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))?;
    let raw = if rest.starts_with('[') {
        let close = rest.find(']')?;
        let mut end = close + 1;
        if rest[end..].starts_with(':') {
            let from = end + 1;
            let stop = rest[from..]
                .find(['/', '?', '#', '\\'])
                .map_or(rest.len(), |i| from + i);
            end = stop;
        }
        let tail = &rest[end..];
        // a `\` ends a domain, then the rest of the URL is not fetched as
        // that host; an address in brackets is refused instead, so a grant
        // of `[::1]` cannot cover `http://[::1]\@other/`
        if !(tail.is_empty() || tail.starts_with(['/', '?', '#'])) {
            return None;
        }
        rest[..end].to_ascii_lowercase()
    } else {
        let end = rest.find(['/', '?', '#', '\\']).unwrap_or(rest.len());
        rest[..end].to_ascii_lowercase()
    };
    let (host, port) = net_endpoint(&raw)?;
    Some(match port {
        Some(port) => format!("{host}:{port}"),
        None => host,
    })
}

/// The host and optional port `text` grants. An IPv6 address is canonical
/// (`[2001:db8::1]`), so two spellings of one address match and a different
/// address does not. A zone (`%`) is refused: dropping it would grant
/// another interface. A mapped IPv4 address stays IPv6. A port stays the
/// digits as written, so `:080` is not `:80`.
pub(crate) fn net_endpoint(text: &str) -> Option<(String, Option<String>)> {
    if let Some(rest) = text.strip_prefix('[') {
        let (literal, after) = rest.split_once(']')?;
        if literal.is_empty() || literal.contains('%') {
            return None;
        }
        let addr: Ipv6Addr = literal.parse().ok()?;
        let port = match after {
            "" => None,
            port => {
                let port = port.strip_prefix(':')?;
                if !port_digits(port) {
                    return None;
                }
                Some(port.to_owned())
            }
        };
        return Some((format!("[{addr}]"), port));
    }
    if !domain_host(text) {
        return None;
    }
    match text.split_once(':') {
        Some((name, port)) => Some((name.to_owned(), Some(port.to_owned()))),
        None => Some((text.to_owned(), None)),
    }
}

/// Whether `port` is a `u16` written in digits, with no sign or other suffix.
fn port_digits(port: &str) -> bool {
    !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) && port.parse::<u16>().is_ok()
}

/// A domain or a dotted IPv4 address: lower-case labels and one optional port.
fn domain_host(host: &str) -> bool {
    let label = |label: &str| {
        !label.is_empty()
            && !label.starts_with('-')
            && !label.ends_with('-')
            && (label.bytes()).all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    };
    let labels = |name: &str| name.split('.').all(label);
    match host.split_once(':') {
        Some((name, port)) => labels(name) && port_digits(port),
        None => labels(host),
    }
}

/// Whether the grant `granted` covers `host`: a name at any port, or a
/// name and port at that one. An address in brackets is one IPv6 address.
fn grants_host(granted: &str, host: &str) -> bool {
    let Some((granted_host, granted_port)) = net_endpoint(granted) else {
        return false;
    };
    let Some((host, port)) = net_endpoint(host) else {
        return false;
    };
    granted_host == host && (granted_port.is_none() || granted_port == port)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use flick_plugin::{ReachError, Request, Response};

    use super::super::{Gate, Outside, Reach};
    use super::{authority, grants_host, snapshot_key};
    use crate::diagnostics::codes;
    use crate::project::config::Grants;

    /// Files and snapshots in memory; offline, a request no snapshot
    /// answers is no answer.
    #[derive(Default)]
    struct Memory {
        files: Mutex<HashMap<PathBuf, Vec<u8>>>,
        snapshots: Mutex<HashMap<PathBuf, Response>>,
    }

    impl Outside for Memory {
        fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
            (self.files.lock().expect("lock").get(path).cloned())
                .ok_or_else(|| io::ErrorKind::NotFound.into())
        }

        fn env(&self, name: &str) -> Option<String> {
            (name == "TOKEN").then(|| "t".to_owned())
        }

        fn fetch(&self, snapshot: &Path, _: &Request) -> Result<Response, ReachError> {
            self.replay(snapshot)
                .ok_or_else(|| ReachError::Offline("offline".into()))
        }

        fn replay(&self, snapshot: &Path) -> Option<Response> {
            self.snapshots.lock().expect("lock").get(snapshot).cloned()
        }
    }

    fn reach(outside: &Arc<Memory>) -> Reach {
        Reach::new(Some(Arc::new(Gate {
            package: "sql".into(),
            root: PathBuf::from("/p"),
            grants: Grants {
                fs_read: vec!["schema".into()],
                net: vec!["db.example.com".into()],
                env: vec!["TOKEN".into()],
                ..Grants::default()
            },
            outside: Arc::clone(outside) as Arc<dyn Outside>,
        })))
    }

    fn get(url: &str) -> Request {
        Request {
            method: "GET".into(),
            url: url.into(),
            ..Request::default()
        }
    }

    #[test]
    fn what_the_project_grants_is_reached_and_recorded() {
        let outside = Arc::new(Memory::default());
        outside
            .files
            .lock()
            .expect("lock")
            .insert("/p/schema/a.sql".into(), b"create".to_vec());
        let mut reach = reach(&outside);
        assert_eq!(reach.read_file("schema/a.sql"), Ok(b"create".to_vec()));
        assert_eq!(reach.env("TOKEN"), Ok(Some("t".into())));
        assert!(reach.problems.is_empty());
        assert_eq!(reach.used.len(), 2);
        let used = reach.used.clone();
        assert!(reach.still(&used));
        outside
            .files
            .lock()
            .expect("lock")
            .insert("/p/schema/a.sql".into(), b"drop".to_vec());
        assert!(!reach.still(&used));
    }

    #[test]
    fn what_it_does_not_grant_is_denied_with_the_line_to_add() {
        let outside = Arc::new(Memory::default());
        let mut reach = reach(&outside);
        for asked in [
            reach.read_file("secrets/key").map(|_| ()),
            reach.read_file("schema/../secrets/key").map(|_| ()),
            reach.env("HOME").map(|_| ()),
            reach.fetch(&get("https://evil.com/")).map(|_| ()),
            reach
                .fetch(&get("https://db.example.com@evil.com/"))
                .map(|_| ()),
            reach.fetch(&get("file:///etc/passwd")).map(|_| ()),
        ] {
            assert!(matches!(asked, Err(ReachError::Denied(_))), "{asked:?}");
        }
        assert!(reach.problems.iter().all(|(code, _)| *code == codes::P038));
        assert!(
            reach.problems[2]
                .1
                .contains("add `env = [\"HOME\"]` to `[plugins.sql]`")
        );
        assert!(reach.used.is_empty());
        let mut nothing = Reach::default();
        assert!(matches!(nothing.env("TOKEN"), Err(ReachError::Denied(_))));
        assert_eq!(nothing.problems[0].0, codes::P038);
    }

    #[test]
    fn a_fetch_answers_from_its_snapshot_or_is_offline() {
        let outside = Arc::new(Memory::default());
        let mut reach = reach(&outside);
        let request = get("https://db.example.com:5432/tables");
        assert!(matches!(reach.fetch(&request), Err(ReachError::Offline(_))));
        assert_eq!(reach.problems[0].0, codes::P039);
        let at = Path::new("/p/.flick/plugin-snapshots/sql")
            .join(format!("{}.json", snapshot_key(&request)));
        let answer = Response {
            status: 200,
            body: b"[]".to_vec(),
        };
        outside
            .snapshots
            .lock()
            .expect("lock")
            .insert(at.clone(), answer.clone());
        let mut again = Reach::new(reach.gate.clone());
        assert_eq!(again.fetch(&request), Ok(answer));
        let used = again.used.clone();
        assert!(again.still(&used));
        outside.snapshots.lock().expect("lock").remove(&at);
        assert!(!again.still(&used));
    }

    #[test]
    fn a_url_names_its_host_or_none() {
        assert_eq!(
            authority("https://db.example.com/x?y").as_deref(),
            Some("db.example.com")
        );
        assert_eq!(
            authority("http://localhost:8080/").as_deref(),
            Some("localhost:8080")
        );
        assert_eq!(authority("http://user@evil.com/"), None);
        assert_eq!(
            authority("http://good.com\\@evil.com/"),
            Some("good.com".into())
        );
        assert_eq!(authority("ftp://db.example.com/"), None);
        assert_eq!(authority("http://[::1]/").as_deref(), Some("[::1]"));
        assert_eq!(authority("http://[::1]\\@other/"), None);
    }

    #[test]
    fn an_ipv6_grant_is_one_address_in_canonical_form() {
        assert_eq!(
            authority("http://[0:0:0:0:0:0:0:1]:8080/x").as_deref(),
            Some("[::1]:8080")
        );
        assert_eq!(
            authority("https://[2001:0DB8:0000::1]/").as_deref(),
            Some("[2001:db8::1]")
        );
        assert_eq!(authority("http://[::1]@other/"), None);
        assert_eq!(authority("http://user@[::1]/"), None);
        assert_eq!(authority("http://[fe80::1%eth0]/"), None);
        assert_eq!(authority("http://[::1]:/"), None);
        assert_eq!(authority("http://[::1]:99999/"), None);
        assert!(grants_host("[::1]", "[::1]"));
        assert!(grants_host("[::1]", "[::1]:8080"));
        assert!(grants_host("[2001:DB8::1]", "[2001:db8:0:0:0:0:0:1]"));
        assert!(grants_host("[::1]:8080", "[::1]:8080"));
        assert!(!grants_host("[::1]:8080", "[::1]:9090"));
        assert!(!grants_host("[::1]:8080", "[::1]"));
        assert!(!grants_host("[::1]:8080", "[::1]:08080"));
        assert!(!grants_host("[::1]:080", "[::1]:80"));
        assert!(!grants_host("[::1]", "[::2]"));
        assert!(!grants_host("[::]", "[::1]"));
        assert!(!grants_host("127.0.0.1", "[::ffff:127.0.0.1]"));
        assert!(!grants_host("[::ffff:127.0.0.1]", "127.0.0.1"));
        assert!(!grants_host("[::1]", "localhost"));
        assert_eq!(
            authority("http://[::ffff:127.0.0.1]/").as_deref(),
            Some("[::ffff:127.0.0.1]")
        );
    }

    #[test]
    fn an_ipv6_fetch_uses_the_grant_or_stays_offline() {
        let outside = Arc::new(Memory::default());
        let mut reach = Reach::new(Some(Arc::new(Gate {
            package: "sql".into(),
            root: PathBuf::from("/p"),
            grants: Grants {
                net: vec!["[::1]".into(), "[2001:db8::1]:443".into()],
                ..Grants::default()
            },
            outside: Arc::clone(&outside) as Arc<dyn Outside>,
        })));
        let asked = get("http://[0:0:0:0:0:0:0:1]/tables");
        assert!(matches!(reach.fetch(&asked), Err(ReachError::Offline(_))));
        assert_eq!(reach.problems[0].0, codes::P039);
        let other = get("https://[2001:DB8::1]:443/v");
        assert!(matches!(reach.fetch(&other), Err(ReachError::Offline(_))));
        // `[::1]` covers every port; the other grant is only port 443 of another address.
        let any_port = get("http://[::1]:9/no");
        assert!(matches!(
            reach.fetch(&any_port),
            Err(ReachError::Offline(_))
        ));
        let wrong = get("http://[::2]/no");
        let refused = reach.fetch(&wrong);
        assert!(matches!(refused, Err(ReachError::Denied(_))), "{refused:?}");
        assert!(reach.problems.iter().any(|(code, message)| {
            *code == codes::P038 && message.contains("net = [\"[::2]\"]")
        }));
        let mapped = get("http://127.0.0.1/");
        let refused = reach.fetch(&mapped);
        assert!(matches!(refused, Err(ReachError::Denied(_))), "{refused:?}");
        let mut narrow = Reach::new(Some(Arc::new(Gate {
            package: "sql".into(),
            root: PathBuf::from("/p"),
            grants: Grants {
                net: vec!["[::1]:8080".into()],
                ..Grants::default()
            },
            outside: outside as Arc<dyn Outside>,
        })));
        let refused = narrow.fetch(&get("http://[::1]:80/"));
        assert!(
            matches!(refused, Err(ReachError::Denied(_)))
                && narrow.problems[0].1.contains("net = [\"[::1]\"]"),
            "{refused:?} {}",
            narrow.problems[0].1
        );
    }

    #[test]
    fn a_grant_names_a_host_at_any_port_or_at_one() {
        assert!(grants_host("db.example.com", "db.example.com:5432"));
        assert!(grants_host("localhost:8080", "localhost:8080"));
        assert!(!grants_host("localhost:8080", "localhost:9090"));
        assert!(!grants_host("example.com", "evil-example.com"));
    }
}
