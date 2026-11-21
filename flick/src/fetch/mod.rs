//! Installing the fetched dependencies of a project: every package its
//! manifests name by git or url, directly or through a library, is in the
//! package store and pinned in `flick.lock`.

mod download;

pub use download::{bytes, hex};

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use flick_core::project::config::{self, Dependency, Source};
use flick_core::project::lock::{LOCK_FILE, Lock, Locked, LockedPlugin};
use flick_core::project::{self, CONFIG_FILE, store};
use flick_core::source::SourceMap;
use sha2::{Digest, Sha256};

#[cfg(feature = "wasm-backends")]
use crate::saved;

/// How far `install` may go to get what the lock lacks.
#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    /// Fail rather than change `flick.lock`.
    pub locked: bool,
    /// Use only the package store, never the network.
    pub offline: bool,
}

/// Fetches what the project at `root` with `dependencies` needs and writes
/// its `flick.lock`; with `update`, the packages it names are fetched again
/// instead of read from the lock. The names of the packages it fetched.
pub fn install(
    root: &Path,
    dependencies: &[Dependency],
    update: &[String],
    options: Options,
) -> Result<Vec<String>, String> {
    let lock_path = root.join(LOCK_FILE);
    let old = read_lock(&lock_path)?;
    let store =
        store::store_dir().ok_or("no cache directory for the package store: set FLICK_STORE")?;
    let mut install = Install {
        old: &old,
        store: &store,
        update,
        options,
        lock: Lock::default(),
        fetched: Vec::new(),
        seen: BTreeSet::new(),
    };
    install.add(root, dependencies)?;
    let Install {
        mut lock, fetched, ..
    } = install;
    lock.plugins = plugins(root, dependencies)?;
    // the backend and template pins are theirs to keep or move
    lock.backends.clone_from(&old.backends);
    lock.templates.clone_from(&old.templates);
    let empty = lock.packages.is_empty()
        && lock.plugins.is_empty()
        && lock.backends.is_empty()
        && lock.templates.is_empty();
    let changed = lock.render() != old.render() || !lock_path.exists() && !empty;
    if changed && options.locked {
        let rebuilt = (lock.plugins.iter())
            .find(|p| !old.plugins.contains(p) && old.plugins.iter().any(|o| o.name == p.name));
        return Err(match rebuilt {
            Some(p) if lock.packages == old.packages => format!(
                "the component of the plugin of `{}` is not the one `{LOCK_FILE}` was written with: \
                 run `flick install` without `--locked`",
                p.name
            ),
            _ => format!(
                "`{LOCK_FILE}` does not pin what the manifests name: run `flick install` without `--locked`"
            ),
        });
    }
    if changed {
        fs::write(&lock_path, lock.render())
            .map_err(|e| format!("cannot write `{}`: {e}", lock_path.display()))?;
    }
    Ok(fetched)
}

/// The components of the plugins of the path dependencies of the project at
/// `root` that are built: a fetched one is pinned by the package's checksum,
/// and one not built yet has nothing to pin.
fn plugins(root: &Path, dependencies: &[Dependency]) -> Result<Vec<LockedPlugin>, String> {
    let mut out = Vec::new();
    for d in dependencies {
        let Some(dir) = d.path() else { continue };
        let library = project::normalize(&root.join(dir));
        let Some(component) = manifest(&library).and_then(|c| c.package?.plugin) else {
            continue;
        };
        let path = project::normalize(&library.join(component));
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("cannot read `{}`: {e}", path.display())),
        };
        out.push(LockedPlugin {
            name: d.name.clone(),
            component: relative(root, &path),
            checksum: format!("sha256:{}", hex(&Sha256::digest(&bytes))),
        });
    }
    Ok(out)
}

/// `path` from `root`, with `/` between its parts.
pub fn relative(root: &Path, path: &Path) -> String {
    let (mut from, mut to) = (root.components().peekable(), path.components().peekable());
    while from.peek().is_some() && from.peek() == to.peek() {
        from.next();
        to.next();
    }
    let up = from.map(|_| "..".to_owned());
    let down = to.map(|c| c.as_os_str().to_string_lossy().into_owned());
    up.chain(down).collect::<Vec<_>>().join("/")
}

/// The directory and pin metadata of a remote template package fetched into the
/// package store for `flick new --from` or `flick init --from`.
#[cfg(feature = "wasm-backends")]
pub struct FetchedTemplate {
    pub dir: PathBuf,
    /// `url+<url>` or `git+<url>?tag=<tag>[&subdir=<dir>]#<commit>`.
    pub source: String,
    /// `sha256:` and the tree hash of the unpacked/cloned package.
    pub checksum: String,
    /// For `url` sources, `sha256:` and the verified tarball digest.
    pub archive: Option<String>,
}

/// Why fetching a remote template package failed.
#[cfg(feature = "wasm-backends")]
pub enum FetchTemplateError {
    /// Network, store, verification, or missing subdirectory failure (`FLICK-P034`).
    Fetch(String),
    /// A subdirectory symlink escapes the fetched package directory (`FLICK-P055`).
    Escape(String),
}

#[cfg(feature = "wasm-backends")]
fn is_hex_sha256(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Fetches a `git` or `url` template source into the package store, reusing
/// cached verified trees for repeated or `--offline` runs.
#[cfg(feature = "wasm-backends")]
pub fn fetch_template(
    source: &Source,
    offline: bool,
) -> Result<FetchedTemplate, FetchTemplateError> {
    let store = store::store_dir().ok_or_else(|| {
        FetchTemplateError::Fetch(
            "no cache directory for the package store: set FLICK_STORE".to_owned(),
        )
    })?;
    match source {
        Source::Url { sha256, .. } => fetch_url_template(&store, source, sha256, offline),
        Source::Git { subdir, .. } => {
            fetch_git_template(&store, source, subdir.as_deref(), offline)
        }
        Source::Path(_) => Err(FetchTemplateError::Fetch(
            "a path template is not fetched".to_owned(),
        )),
    }
}

#[cfg(feature = "wasm-backends")]
fn fetch_url_template(
    store: &Path,
    source: &Source,
    sha256: &str,
    offline: bool,
) -> Result<FetchedTemplate, FetchTemplateError> {
    let digest = sha256.to_ascii_lowercase();
    let pin_path = store.join("url-pins").join(&digest);
    if let Ok(tree) = fs::read_to_string(&pin_path) {
        let hash = tree.trim();
        let dir = store.join(hash);
        if is_hex_sha256(hash) && dir.is_dir() {
            let canon_dir = fs::canonicalize(&dir).map_err(|e| {
                FetchTemplateError::Fetch(format!(
                    "cannot inspect cached template `{}`: {e}",
                    dir.display()
                ))
            })?;
            return Ok(FetchedTemplate {
                dir: canon_dir,
                source: source.key(),
                checksum: format!("sha256:{hash}"),
                archive: Some(format!("sha256:{digest}")),
            });
        }
    }
    if offline {
        return Err(FetchTemplateError::Fetch(format!(
            "template source `{}` is not in the package store, and `--offline` fetches nothing",
            source.key()
        )));
    }
    let stored = download::fetch(source, store).map_err(FetchTemplateError::Fetch)?;
    let hash = stored
        .checksum
        .strip_prefix("sha256:")
        .unwrap_or(&stored.checksum);
    let _ = saved::write(&pin_path, hash.as_bytes());
    let dir = store.join(hash);
    let canon_dir = fs::canonicalize(&dir).map_err(|e| {
        FetchTemplateError::Fetch(format!(
            "cannot inspect fetched template `{}`: {e}",
            dir.display()
        ))
    })?;
    Ok(FetchedTemplate {
        dir: canon_dir,
        source: source.key(),
        checksum: stored.checksum,
        archive: Some(format!("sha256:{digest}")),
    })
}

#[cfg(feature = "wasm-backends")]
fn fetch_git_template(
    store: &Path,
    source: &Source,
    subdir: Option<&str>,
    offline: bool,
) -> Result<FetchedTemplate, FetchTemplateError> {
    let key = source.key();
    let key_hash = hex(&Sha256::digest(key.as_bytes()));
    let pin_path = store.join("git-pins").join(&key_hash);
    let (root, commit, checksum) = if offline {
        let cached = fs::read_to_string(&pin_path).ok().and_then(|text| {
            let mut lines = text.lines();
            let commit = lines.next()?.trim().to_owned();
            let hash = lines.next()?.trim().to_owned();
            let dir = store.join(&hash);
            (!commit.is_empty() && is_hex_sha256(&hash) && dir.is_dir()).then_some((
                dir,
                commit,
                format!("sha256:{hash}"),
            ))
        });
        cached.ok_or_else(|| {
            FetchTemplateError::Fetch(format!(
                "template source `{key}` is not in the package store, and `--offline` fetches nothing"
            ))
        })?
    } else {
        let stored = download::fetch(source, store).map_err(FetchTemplateError::Fetch)?;
        let commit = stored.pin.unwrap_or_default();
        let hash = stored
            .checksum
            .strip_prefix("sha256:")
            .unwrap_or(&stored.checksum)
            .to_owned();
        let _ = saved::write(&pin_path, format!("{commit}\n{hash}\n").as_bytes());
        (store.join(&hash), commit, stored.checksum)
    };
    let canon_root = fs::canonicalize(&root).map_err(|e| {
        FetchTemplateError::Fetch(format!("cannot inspect template repository `{key}`: {e}"))
    })?;
    let dir = match subdir {
        Some(sub) => resolve_git_subdir(&root, &canon_root, sub, &key)?,
        None => canon_root,
    };
    let pinned_source = if commit.is_empty() {
        key
    } else {
        format!("{key}#{commit}")
    };
    Ok(FetchedTemplate {
        dir,
        source: pinned_source,
        checksum,
        archive: None,
    })
}

#[cfg(feature = "wasm-backends")]
fn resolve_git_subdir(
    root: &Path,
    canon_root: &Path,
    sub: &str,
    key: &str,
) -> Result<PathBuf, FetchTemplateError> {
    let at = root.join(sub);
    if fs::symlink_metadata(&at).is_err() {
        return Err(FetchTemplateError::Fetch(format!(
            "subdirectory `{sub}` does not exist in template repository `{key}`"
        )));
    }
    let canon_at = fs::canonicalize(&at).map_err(|e| {
        FetchTemplateError::Fetch(format!(
            "cannot inspect subdirectory `{sub}` in template repository `{key}`: {e}"
        ))
    })?;
    if !canon_at.starts_with(canon_root) {
        return Err(FetchTemplateError::Escape(format!(
            "subdirectory `{sub}` escapes template repository `{key}`"
        )));
    }
    if !canon_at.is_dir() {
        return Err(FetchTemplateError::Fetch(format!(
            "subdirectory `{sub}` does not exist in template repository `{key}`"
        )));
    }
    Ok(canon_at)
}

/// The `flick.lock` of the project at `root`; none is an empty one.
pub fn lock_of(root: &Path) -> Result<Lock, String> {
    read_lock(&root.join(LOCK_FILE))
}

pub(crate) fn read_lock(path: &Path) -> Result<Lock, String> {
    let Ok(text) = fs::read_to_string(path) else {
        return Ok(Lock::default());
    };
    let mut sources = SourceMap::default();
    let file = sources.add(path.display().to_string(), &text);
    Lock::parse(&text, file).map_err(|d| d.message)
}

struct Install<'a> {
    old: &'a Lock,
    store: &'a Path,
    update: &'a [String],
    options: Options,
    lock: Lock,
    fetched: Vec<String>,
    /// Libraries already followed, by directory.
    seen: BTreeSet<PathBuf>,
}

impl Install<'_> {
    /// The packages `dependencies` of the manifest in `dir` need, and theirs.
    fn add(&mut self, dir: &Path, dependencies: &[Dependency]) -> Result<(), String> {
        for dependency in dependencies {
            let library = match &dependency.source {
                Source::Path(path) => project::normalize(&dir.join(path)),
                source => {
                    let locked = self.locked(dependency)?;
                    let fetched = locked.dir(self.store);
                    source.subdir().map_or(fetched.clone(), |s| fetched.join(s))
                }
            };
            if !self.seen.insert(library.clone()) {
                continue;
            }
            // a path library that is not there is the loader's to report
            if let Some(config) = manifest(&library) {
                self.add(&library, &config.dependencies)?;
            }
        }
        Ok(())
    }

    /// The pin of the fetched dependency `dependency`: the old one when its
    /// files are in the store, else what fetching it gives.
    fn locked(&mut self, dependency: &Dependency) -> Result<Locked, String> {
        let name = &dependency.name;
        if let Some(other) = self.lock.packages.iter().find(|p| p.name == *name) {
            if other.pins(&dependency.source) {
                return Ok(other.clone());
            }
            return Err(format!(
                "two sources of `{name}`: `{}` and `{}`; name one of them",
                other.source,
                dependency.source.key()
            ));
        }
        let reuse = !self.update.contains(name);
        let old = self.old.find(name, &dependency.source).filter(|_| reuse);
        if let Some(old) = old.filter(|o| o.dir(self.store).exists()) {
            self.lock.packages.push(old.clone());
            return Ok(old.clone());
        }
        if self.options.locked && old.is_none() {
            return Err(format!(
                "`{name}` from `{}` is not in `{LOCK_FILE}`, and `--locked` leaves it as it is",
                dependency.source.key()
            ));
        }
        if self.options.offline {
            return Err(format!(
                "`{name}` from `{}` is not in the package store, and `--offline` fetches nothing",
                dependency.source.key()
            ));
        }
        let stored = download::fetch(&dependency.source, self.store)
            .map_err(|e| format!("dependency `{name}`: {e}"))?;
        if let Some(old) = old
            && old.checksum != stored.checksum
        {
            return Err(format!(
                "`{name}` from `{}` has other files than `{LOCK_FILE}` pins: it changed where it is \
                 published; `flick update {name}` takes the new files",
                dependency.source.key()
            ));
        }
        let mut locked = Locked {
            name: name.clone(),
            version: String::new(),
            source: match &stored.pin {
                Some(pin) => format!("{}#{pin}", dependency.source.key()),
                None => dependency.source.key(),
            },
            checksum: stored.checksum,
        };
        let library = dependency
            .source
            .subdir()
            .map_or(locked.dir(self.store), |s| locked.dir(self.store).join(s));
        locked.version = manifest(&library)
            .and_then(|c| c.package)
            .map_or_else(|| "0.0.0".to_owned(), |p| p.version);
        self.fetched.push(name.clone());
        self.lock.packages.push(locked.clone());
        Ok(locked)
    }
}

/// The `flick.toml` of the library in `dir`, when it has a valid one.
fn manifest(dir: &Path) -> Option<config::Config> {
    let path = dir.join(CONFIG_FILE);
    let text = fs::read_to_string(&path).ok()?;
    let mut sources = SourceMap::default();
    let file = sources.add(path.display().to_string(), &text);
    config::parse(&text, file).ok()
}
