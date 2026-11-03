//! `flick.lock`: the exact sources of every fetched dependency of a project,
//! direct or not, so that a build fetches the same files each time.

use std::fmt::Write;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::config::Source;
use crate::diagnostics::{Diagnostic, codes};
use crate::source::{FileId, Span};

pub const LOCK_FILE: &str = "flick.lock";

const LOCK_VERSION: i64 = 1;

/// The pinned packages of a project.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lock {
    pub packages: Vec<Locked>,
}

/// One fetched package: what the manifest named and what was fetched.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Locked {
    pub name: String,
    pub version: String,
    /// [`Source::key`], then `#` and the commit of a git source.
    pub source: String,
    /// `sha256:` and the hash of its files, which names its directory in the
    /// package store.
    pub checksum: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    version: i64,
    #[serde(default)]
    package: Vec<Locked>,
}

impl Locked {
    /// Whether this pins `source`.
    pub fn pins(&self, source: &Source) -> bool {
        let key = source.key();
        self.source == key
            || self
                .source
                .strip_prefix(key.as_str())
                .is_some_and(|rest| rest.starts_with('#'))
    }

    /// The directory of its files in the package store `store`.
    pub fn dir(&self, store: &Path) -> PathBuf {
        let hash = self
            .checksum
            .strip_prefix("sha256:")
            .unwrap_or(&self.checksum);
        store.join(hash)
    }
}

impl Lock {
    /// The package pinning the dependency `name` from `source`.
    pub fn find(&self, name: &str, source: &Source) -> Option<&Locked> {
        self.packages
            .iter()
            .find(|p| p.name == name && p.pins(source))
    }

    pub fn parse(text: &str, file: FileId) -> Result<Lock, Diagnostic> {
        let error = |message: String| {
            Diagnostic::error(
                codes::P034,
                format!("invalid `{LOCK_FILE}`: {message}; `flick install` writes it again"),
                Span::new(file, 0, 0),
            )
        };
        let raw: Raw = toml::from_str(text).map_err(|e| error(e.message().trim().to_owned()))?;
        if raw.version != LOCK_VERSION {
            return Err(error(format!(
                "version {} is not {LOCK_VERSION}",
                raw.version
            )));
        }
        Ok(Lock {
            packages: raw.package,
        })
    }

    /// The file's text: the packages by name, so that it changes only where
    /// they do.
    pub fn render(&self) -> String {
        let mut packages: Vec<&Locked> = self.packages.iter().collect();
        packages.sort_by(|a, b| (&a.name, &a.source).cmp(&(&b.name, &b.source)));
        let mut out = format!(
            "# Written by `flick install`; do not edit it by hand.\nversion = {LOCK_VERSION}\n"
        );
        for p in packages {
            let _ = write!(
                out,
                "\n[[package]]\nname = {}\nversion = {}\nsource = {}\nchecksum = {}\n",
                quote(&p.name),
                quote(&p.version),
                quote(&p.source),
                quote(&p.checksum)
            );
        }
        out
    }
}

/// `text` as a TOML basic string.
fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04X}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::{Lock, Locked};
    use crate::project::config::Source;
    use crate::source::SourceMap;

    #[test]
    fn a_lock_reads_back_what_it_writes() {
        let lock = Lock {
            packages: vec![Locked {
                name: "std".into(),
                version: "0.1.0".into(),
                source: "git+https://x/y?tag=std-v0.1.0&subdir=packages/std#abc123".into(),
                checksum: format!("sha256:{}", "0f".repeat(32)),
            }],
        };
        let text = lock.render();
        let mut map = SourceMap::default();
        let file = map.add("flick.lock", &text);
        assert_eq!(Lock::parse(&text, file), Ok(lock.clone()));
        let source = Source::Git {
            url: "https://x/y".into(),
            tag: "std-v0.1.0".into(),
            subdir: Some("packages/std".into()),
        };
        assert!(lock.find("std", &source).is_some());
        let other = Source::Git {
            url: "https://x/y".into(),
            tag: "std-v0.2.0".into(),
            subdir: Some("packages/std".into()),
        };
        assert!(lock.find("std", &other).is_none());
    }
}
