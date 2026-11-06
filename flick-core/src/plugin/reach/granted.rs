//! What a project's own build reaches for its plugins: each call checked
//! against the grants, asked of the host's [`Outside`], and recorded with
//! the hash of its answer.

use std::fmt::Write as _;
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
            let name = host.split(':').next().unwrap_or(&host).to_owned();
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
                // shortcut: the answer depends on other modules, so a block that asked is expanded again each run; keep the answer's hash with the modules' when expanding costs
                Asked::LiteralOf(_) => return false,
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

/// The host and port of an `http` or `https` URL, lower case; none for
/// another URL, or one whose authority hides its host.
fn authority(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))?;
    let end = rest.find(['/', '?', '#', '\\']).unwrap_or(rest.len());
    let host = rest[..end].to_ascii_lowercase();
    let plain = !host.is_empty()
        && (host.bytes())
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-.:".contains(&b));
    // shortcut: an IPv6 host in brackets is refused; allow one when a plugin needs it
    plain.then_some(host)
}

/// Whether the grant `granted` covers `host`: a name at any port, or a
/// name and port at that one.
fn grants_host(granted: &str, host: &str) -> bool {
    if granted.contains(':') {
        granted == host
    } else {
        host.split(':').next() == Some(granted)
    }
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
        assert_eq!(authority("http://[::1]/"), None);
    }

    #[test]
    fn a_grant_names_a_host_at_any_port_or_at_one() {
        assert!(grants_host("db.example.com", "db.example.com:5432"));
        assert!(grants_host("localhost:8080", "localhost:8080"));
        assert!(!grants_host("localhost:8080", "localhost:9090"));
        assert!(!grants_host("example.com", "evil-example.com"));
    }
}
