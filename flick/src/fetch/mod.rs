//! Installing the fetched dependencies of a project: every package its
//! manifests name by git or url, directly or through a library, is in the
//! package store and pinned in `flick.lock`.

mod download;

pub use download::hex;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use flick_core::project::config::{self, Dependency, Source};
use flick_core::project::lock::{LOCK_FILE, Lock, Locked, LockedPlugin};
use flick_core::project::{self, CONFIG_FILE, store};
use flick_core::source::SourceMap;
use sha2::{Digest, Sha256};

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
    lock.plugins = plugins(root, dependencies);
    let empty = lock.packages.is_empty() && lock.plugins.is_empty();
    let changed = lock.render() != old.render() || !lock_path.exists() && !empty;
    if changed && options.locked {
        let rebuilt = (lock.plugins.iter()).find(|p| !old.plugins.contains(p));
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
/// `root` that are built: a fetched one is pinned by the package's checksum.
fn plugins(root: &Path, dependencies: &[Dependency]) -> Vec<LockedPlugin> {
    (dependencies.iter())
        .filter_map(|d| {
            let library = project::normalize(&root.join(d.path()?));
            let component = manifest(&library)?.package?.plugin?;
            let path = project::normalize(&library.join(component));
            let bytes = fs::read(&path).ok()?;
            let relative = relative(root, &path);
            Some(LockedPlugin {
                name: d.name.clone(),
                component: relative,
                checksum: format!("sha256:{}", hex(&Sha256::digest(&bytes))),
            })
        })
        .collect()
}

/// `path` from `root`, with `/` between its parts.
fn relative(root: &Path, path: &Path) -> String {
    let (mut from, mut to) = (root.components().peekable(), path.components().peekable());
    while from.peek().is_some() && from.peek() == to.peek() {
        from.next();
        to.next();
    }
    let up = from.map(|_| "..".to_owned());
    let down = to.map(|c| c.as_os_str().to_string_lossy().into_owned());
    up.chain(down).collect::<Vec<_>>().join("/")
}

fn read_lock(path: &Path) -> Result<Lock, String> {
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
