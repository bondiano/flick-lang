//! What a plugin reaches outside its blocks, as the project grants it in
//! `[plugins.<package>]`: a file under the project's root, a variable of
//! the environment, an answer over HTTP. The gate checks each call against
//! the grants and asks the host's [`Outside`]; what was reached is recorded
//! with a hash of its answer, so a kept result is used again only while
//! each answer stays the same.

use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use flick_plugin::{ReachError, Request, Response};
use serde::{Deserialize, Serialize};

use crate::diagnostics::{Code, codes};
use crate::project::config::Grants;

#[cfg(feature = "native-project")]
mod granted;

#[cfg(feature = "native-project")]
pub use granted::{hash, snapshot_key};

/// Where snapshots of answers over HTTP are kept, from the project's root:
/// one directory for each package.
pub const SNAPSHOTS: &str = ".flick/plugin-snapshots";

/// The world outside the compiler, as the host gives it.
pub trait Outside: Send + Sync {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;
    fn env(&self, name: &str) -> Option<String>;
    /// The answer to `request`: the one kept at `snapshot`, or, online, the
    /// one the host fetches and keeps there.
    fn fetch(&self, snapshot: &Path, request: &Request) -> Result<Response, ReachError>;
    /// The answer kept at `snapshot`, without the network.
    fn replay(&self, snapshot: &Path) -> Option<Response>;
}

/// What the plugin of a package may reach for the blocks of a project.
pub struct Gate {
    pub package: String,
    /// The project's root, which paths and snapshots count from.
    pub root: PathBuf,
    pub grants: Grants,
    pub outside: Arc<dyn Outside>,
}

/// What a call reached, as a kept result is checked again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Asked {
    File(String),
    Env(String),
    /// The snapshot of a request, by its key.
    Fetch(String),
    /// The base type of an enum, as the block's module names it.
    LiteralOf(String),
    /// What the plugin published of a name, as the block's module names it.
    Published(String),
    /// The names the plugin published that the block's module sees.
    PublishedNames,
}

impl Asked {
    /// Whether what the block's module sees answers it, not the gate.
    pub fn by_module(&self) -> bool {
        matches!(
            self,
            Asked::LiteralOf(_) | Asked::Published(_) | Asked::PublishedNames
        )
    }
}

/// What a call reached and the hash of its answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Used {
    pub asked: Asked,
    pub answer: String,
}

/// What the calls of one block reached, and what was refused, each with
/// its code: `FLICK-P038` beyond the grants, `FLICK-P039` offline.
#[derive(Default)]
pub struct Reach {
    gate: Option<Arc<Gate>>,
    pub used: Vec<Used>,
    pub problems: Vec<(Code, String)>,
}

impl Reach {
    pub fn new(gate: Option<Arc<Gate>>) -> Self {
        Reach {
            gate,
            used: Vec::new(),
            problems: Vec::new(),
        }
    }

    /// The gate, or why there is none: a block of no project's own module
    /// reaches nothing.
    fn gate(&mut self) -> Result<Arc<Gate>, ReachError> {
        if let Some(gate) = &self.gate {
            return Ok(Arc::clone(gate));
        }
        let message = "a plugin reaches nothing for a module of no project's own".to_owned();
        self.problems.push((codes::P038, message.clone()));
        Err(ReachError::Denied(message))
    }
}

/// A build without a project's own files, as the playground's, grants
/// nothing: every call is refused, and nothing is recorded.
#[cfg(not(feature = "native-project"))]
impl Reach {
    pub fn read_file(&mut self, _: &str) -> Result<Vec<u8>, ReachError> {
        self.gate().map(|_| Vec::new())
    }

    pub fn env(&mut self, _: &str) -> Result<Option<String>, ReachError> {
        self.gate().map(|_| None)
    }

    pub fn fetch(&mut self, _: &Request) -> Result<Response, ReachError> {
        self.gate().map(|_| Response {
            status: 0,
            body: Vec::new(),
        })
    }

    pub fn still(&self, used: &[Used]) -> bool {
        used.is_empty()
    }
}

impl Reach {
    /// `answer` to `literal-of` of `name`, kept with what the block asked.
    pub fn literal_of(&mut self, name: &str, answer: Option<String>) -> Option<String> {
        self.used.push(Used {
            asked: Asked::LiteralOf(name.to_owned()),
            answer: literal_answer(answer.as_deref()),
        });
        answer
    }

    /// `names`, the answer to `published-names`, kept with what the block
    /// asked.
    pub fn published_names(&mut self, names: Vec<String>) -> Vec<String> {
        self.used.push(Used {
            asked: Asked::PublishedNames,
            answer: names.join("\n"),
        });
        names
    }

    /// `answer` to `published` of `name`, kept with what the block asked.
    pub fn published(&mut self, name: &str, answer: Option<Vec<u8>>) -> Option<Vec<u8>> {
        self.used.push(Used {
            asked: Asked::Published(name.to_owned()),
            answer: published_answer(answer.as_deref()),
        });
        answer
    }
}

/// How an answer to `literal-of` is kept.
pub fn literal_answer(answer: Option<&str>) -> String {
    format!("{answer:?}")
}

/// How an answer to `published` is kept: a hash of it, or itself in a
/// build without hashes, which keeps results only in memory.
pub fn published_answer(answer: Option<&[u8]>) -> String {
    #[cfg(feature = "native-project")]
    return answer.map_or_else(String::new, |data| hash([b"some".as_slice(), data]));
    #[cfg(not(feature = "native-project"))]
    format!("{answer:?}")
}

/// Whether `path` names something inside a directory: no `..`, no root.
pub fn inside(path: &str) -> bool {
    !path.is_empty()
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use super::inside;

    #[test]
    fn a_path_is_inside_without_parents_or_a_root() {
        assert!(inside("schema/a.sql"));
        assert!(!inside("../a.sql"));
        assert!(!inside("schema/../../a.sql"));
        assert!(!inside("/etc/passwd"));
        assert!(!inside(""));
    }
}
