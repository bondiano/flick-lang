//! The official backends, `flick/js` and `flick/go`: not in the compiler but
//! release assets, which a project pins in `flick.lock` and the package store
//! keeps by digest. A compiler built from a checkout takes that checkout's own
//! components instead, as it takes its own standard library.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{env, fs, process};

use flick_backend::{BackendId, BackendPin, VersionRange};
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::lock::{LOCK_FILE, Lock, LockedBackend};
use flick_core::project::{Project, store};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::component::Package;
use crate::fetch;

pub(super) const JS: &str = "flick/js";
pub(super) const GO: &str = "flick/go";
pub(super) const IDS: [&str; 2] = [JS, GO];

/// The index the release workflow keeps of every published component.
const INDEX: &str =
    "https://github.com/bondiano/flick-lang/releases/download/backends-index/backends.json";

/// One published component in the index.
#[derive(Deserialize)]
struct Entry {
    id: String,
    version: String,
    sha256: String,
    url: String,
    backend_api: VersionRange,
    ir: VersionRange,
}

#[derive(Deserialize)]
struct Index {
    backends: Vec<Entry>,
}

fn failure(message: String) -> Diagnostic {
    Diagnostic::new(codes::P056, Severity::Error, message)
}

pub(super) fn by_id(
    id: &str,
    root: &Path,
    project: Option<&Project>,
    tools: Option<&BTreeMap<String, Vec<String>>>,
) -> Result<Package, Diagnostic> {
    let (pin, bytes) = resolve(id, project)?;
    Package::from_bytes(root, pin, &bytes, tools)
}

/// Loads an official backend from a local checkout or local store without ever
/// reaching the network.
pub(super) fn load_local(
    id: &str,
    root: &Path,
    project: Option<&Project>,
    tools: Option<&BTreeMap<String, Vec<String>>>,
) -> Result<Package, Diagnostic> {
    let (pin, bytes) = local_bytes(id, project).ok_or_else(|| {
        failure(format!(
            "`{id}` is not available locally: build once online or run `flick install`"
        ))
    })?;
    Package::from_bytes(root, pin, &bytes, tools)
}

/// Whether `version` falls inside `[minimum, maximum_exclusive)`.
pub(super) fn accepts(range: &VersionRange, version: &str) -> bool {
    let (Ok(min), Ok(max), Ok(version)) = (
        Version::parse(&range.minimum),
        Version::parse(&range.maximum_exclusive),
        Version::parse(version),
    ) else {
        return false;
    };
    min <= version && version < max
}

/// Resolves the official backend `id` for scaffolding (`flick new` / `flick init`),
/// returning `(pin, bytes, lock_entry)`. A local checkout returns `None` for
/// `lock_entry`; a release-index resolution returns `Some(entry)` so the
/// generated project pins `[[backend]]` in `flick.lock`.
pub(super) fn resolve_target(
    id: &str,
    existing_lock: Option<&Lock>,
    offline: bool,
) -> Result<(BackendPin, Vec<u8>, Option<LockedBackend>), Diagnostic> {
    if let Some((pin, bytes)) = checkout(id) {
        return Ok((pin, bytes, None));
    }
    let offline = offline || super::offline();
    if let Some(entry) = existing_lock.and_then(|l| l.backends.iter().find(|b| b.id == id).cloned())
    {
        let bytes = fetched_with_offline(&entry, offline)?;
        return Ok((pin_of(&entry), bytes, Some(entry)));
    }
    if offline {
        return Err(failure(format!(
            "`{LOCK_FILE}` pins no `{id}`, and `--offline` fetches nothing"
        )));
    }
    let entry = latest(id)?;
    let bytes = fetched_with_offline(&entry, false)?;
    Ok((pin_of(&entry), bytes, Some(entry)))
}

/// Resolves the official backend `id` to `(pin, bytes)` for reading its
/// descriptor, using a checkout, `flick.lock` + store, or (when online)
/// downloading into the store without mutating `flick.lock`.
pub(super) fn fetch_for_descriptor(
    id: &str,
    project: Option<&Project>,
    offline: bool,
) -> Result<(BackendPin, Vec<u8>), Diagnostic> {
    if let Some(found) = checkout(id) {
        return Ok(found);
    }
    let offline = offline || super::offline();
    if let Some(entry) = project.and_then(|p| lock_entry(p, id)) {
        return fetched_with_offline(&entry, offline).map(|bytes| (pin_of(&entry), bytes));
    }
    if offline {
        return Err(failure(format!(
            "`{LOCK_FILE}` pins no `{id}`, and `--offline` fetches nothing: build once online or run `flick install`"
        )));
    }
    let entry = latest(id)?;
    let bytes = fetched_with_offline(&entry, false)?;
    Ok((pin_of(&entry), bytes))
}

/// The component bytes of the official backend `id` when already available
/// locally (checkout or local package store), without ever reaching the network.
pub(super) fn local_bytes(id: &str, project: Option<&Project>) -> Option<(BackendPin, Vec<u8>)> {
    if let Some(found) = checkout(id) {
        return Some(found);
    }
    let entry = project.and_then(|p| lock_entry(p, id))?;
    let path = stored_path(&entry.sha256)?;
    let bytes = fs::read(path).ok()?;
    (fetch::hex(&Sha256::digest(&bytes)) == entry.sha256).then(|| (pin_of(&entry), bytes))
}

/// The pin of the official backend `id` as far as it is known without the
/// network: a checkout's own, or the one `flick.lock` names.
pub(super) fn known(id: &str, project: Option<&Project>) -> Option<BackendPin> {
    if let Some((pin, _)) = checkout(id) {
        return Some(pin);
    }
    project
        .and_then(|p| lock_entry(p, id))
        .map(|entry| pin_of(&entry))
}

/// The component of the official backend `id` and its pin, from a checkout,
/// `flick.lock` and the store, or the index; whatever the index gives is
/// pinned in `flick.lock` of `project`.
fn resolve(id: &str, project: Option<&Project>) -> Result<(BackendPin, Vec<u8>), Diagnostic> {
    if let Some(found) = checkout(id) {
        return Ok(found);
    }
    if let Some(entry) = project.and_then(|p| lock_entry(p, id)) {
        return fetched(&entry).map(|bytes| (pin_of(&entry), bytes));
    }
    if super::offline() {
        return Err(failure(format!(
            "`{LOCK_FILE}` pins no `{id}`, and `--offline` fetches nothing: build once online or run `flick install`"
        )));
    }
    let entry = latest(id)?;
    let bytes = fetched(&entry)?;
    if let Some(project) = project {
        record(project, &entry)?;
    }
    Ok((pin_of(&entry), bytes))
}

/// What `flick install` and `flick update` do for the official backend `id`
/// of `project`: keep the pin `flick.lock` has and its component in the
/// store, or pin the newest release; `update` pins the newest in any case.
pub(super) fn pin(
    project: &Project,
    id: &str,
    update: bool,
    locked: bool,
    offline: bool,
) -> Result<(), Diagnostic> {
    if checkout(id).is_some() {
        return Ok(());
    }
    match lock_entry(project, id).filter(|_| !update) {
        Some(entry) => fetched(&entry).map(|_| ()),
        None if locked => Err(Diagnostic::new(
            codes::P034,
            Severity::Error,
            format!("`{id}` is not in `{LOCK_FILE}`, and `--locked` leaves it as it is"),
        )),
        None if offline || super::offline() => Err(failure(format!(
            "`{LOCK_FILE}` pins no `{id}`, and `--offline` fetches nothing"
        ))),
        None => {
            let entry = latest(id)?;
            fetched(&entry)?;
            record(project, &entry)
        }
    }
}

fn pin_of(entry: &LockedBackend) -> BackendPin {
    BackendPin {
        identity: BackendId {
            id: entry.id.clone(),
            version: entry.version.clone(),
        },
        sha256: entry.sha256.clone(),
    }
}

/// The component of a checkout of this repository: around the working
/// directory, under `FLICK_BACKENDS_PATH`, or where this binary was built
/// while that is still there. An index `FLICK_BACKENDS_INDEX` names wins.
fn checkout(id: &str) -> Option<(BackendPin, Vec<u8>)> {
    if env::var_os("FLICK_BACKENDS_INDEX").is_some_and(|i| !i.is_empty()) {
        return None;
    }
    let name = id.strip_prefix("flick/")?;
    let explicit = env::var_os("FLICK_BACKENDS_PATH")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from);
    let cwd = env::current_dir().ok();
    let built_from = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf);
    let dir = explicit.map(|d| d.join(name)).or_else(|| {
        (cwd.iter().chain(&built_from))
            .flat_map(|d| d.ancestors())
            .find(|root| {
                root.join("flick-core/Cargo.toml").is_file()
                    && root.join(".cargo/config.toml").is_file()
            })
            .map(|root| root.join("backends").join(name))
    })?;
    let package = dir.join("package");
    let pin: BackendPin =
        serde_json::from_slice(&fs::read(package.join("backend.json")).ok()?).ok()?;
    let bytes = fs::read(package.join("component.wasm")).ok()?;
    (pin.identity.id == id).then_some((pin, bytes))
}

fn lock_entry(project: &Project, id: &str) -> Option<LockedBackend> {
    let lock = fetch::read_lock(&project.root.join(LOCK_FILE)).ok()?;
    lock.backends.into_iter().find(|b| b.id == id)
}

fn stored_path(sha256: &str) -> Option<PathBuf> {
    store::store_dir().map(|d| d.join("backends").join(format!("{sha256}.wasm")))
}

/// The bytes of `entry`: from the store, else downloaded, checked against
/// its digest and stored.
fn fetched(entry: &LockedBackend) -> Result<Vec<u8>, Diagnostic> {
    fetched_with_offline(entry, super::offline())
}

fn fetched_with_offline(entry: &LockedBackend, offline: bool) -> Result<Vec<u8>, Diagnostic> {
    let path = stored_path(&entry.sha256);
    if let Some(bytes) = path.as_ref().and_then(|p| fs::read(p).ok())
        && fetch::hex(&Sha256::digest(&bytes)) == entry.sha256
    {
        return Ok(bytes);
    }
    let remote = entry.url.starts_with("http://") || entry.url.starts_with("https://");
    if remote && offline {
        return Err(failure(format!(
            "`{}` {} is not in the package store, and `--offline` fetches nothing",
            entry.id, entry.version
        )));
    }
    let bytes = fetch::bytes(&entry.url).map_err(failure)?;
    let actual = fetch::hex(&Sha256::digest(&bytes));
    if actual != entry.sha256 {
        return Err(failure(format!(
            "`{}` has sha256 {actual}, and `{}` {} is {}",
            entry.url, entry.id, entry.version, entry.sha256
        )));
    }
    if let Some(path) = path {
        let saved = path
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| {
                let staging = path.with_extension(format!("wasm.{}", process::id()));
                fs::write(&staging, &bytes).and_then(|()| fs::rename(&staging, &path))
            });
        saved.map_err(|e| failure(format!("cannot store `{}`: {e}", path.display())))?;
    }
    Ok(bytes)
}

/// The newest release of `id` in the index that accepts this compiler's
/// backend API and IR.
fn latest(id: &str) -> Result<LockedBackend, Diagnostic> {
    let url = env::var("FLICK_BACKENDS_INDEX")
        .ok()
        .filter(|i| !i.is_empty())
        .unwrap_or_else(|| INDEX.to_owned());
    let bytes =
        fetch::bytes(&url).map_err(|e| failure(format!("cannot read the backends index: {e}")))?;
    let index: Index = serde_json::from_slice(&bytes)
        .map_err(|e| failure(format!("`{url}` is no backends index: {e}")))?;
    let newest = (index.backends.iter())
        .filter(|e| e.id == id)
        .filter(|e| {
            accepts(&e.backend_api, flick_backend::VERSION) && accepts(&e.ir, flick_ir::VERSION)
        })
        .filter_map(|e| Some((Version::parse(&e.version).ok()?, e)))
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, e)| e);
    let Some(entry) = newest else {
        return Err(failure(format!(
            "the backends index lists no release of `{id}` for backend API {} and IR {}",
            flick_backend::VERSION,
            flick_ir::VERSION
        )));
    };
    Ok(LockedBackend {
        id: entry.id.clone(),
        version: entry.version.clone(),
        sha256: entry.sha256.clone(),
        url: entry.url.clone(),
    })
}

/// Pins `entry` in the `flick.lock` of `project`, beside what it pins.
fn record(project: &Project, entry: &LockedBackend) -> Result<(), Diagnostic> {
    let path = project.root.join(LOCK_FILE);
    let mut lock = fetch::read_lock(&path).map_err(failure)?;
    lock.backends.retain(|b| b.id != entry.id);
    lock.backends.push(entry.clone());
    fs::write(&path, lock.render())
        .map_err(|e| failure(format!("cannot write `{}`: {e}", path.display())))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use flick_backend::Backend;
    use flick_backend::host::{Budget, WasmBackend};
    use flick_core::project::official_backends;

    use super::{IDS, checkout};
    use crate::backends::options;
    use crate::component;

    /// The backends index lists what each pin says its component accepts,
    /// so the pin must say what the component's descriptor does.
    #[test]
    fn each_official_pin_lists_what_its_component_accepts() {
        let core_official = official_backends();
        for id in IDS {
            let (pin, bytes) = checkout(id).expect("this checkout's component");
            let engine = component::engine().expect("an engine");
            let cancel = component::cancellation().expect("a cancellation").clone();
            let backend =
                WasmBackend::load_with_cancellation(engine, pin, &bytes, Budget::default(), cancel)
                    .expect("the component loads");
            let descriptor = backend.describe();
            assert!(flick_backend::validate_tags(&descriptor).is_ok(), "{id}");
            assert!(
                flick_backend::validate_foreign_metadata(&descriptor).is_ok(),
                "{id}"
            );
            let from_core = core_official
                .iter()
                .find(|b| b.id == id)
                .unwrap_or_else(|| panic!("official_backends missing {id}"));
            assert_eq!(
                &options::descriptor_tags(&descriptor),
                from_core,
                "{id} component descriptor differs from core official_backends()"
            );
            let name = id.trim_start_matches("flick/");
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("../backends/{name}/package/backend.json"));
            let listed: serde_json::Value =
                serde_json::from_slice(&fs::read(path).expect("the pin")).expect("JSON");
            for (key, range) in [
                ("backend_api", &descriptor.backend_api),
                ("ir", &descriptor.ir),
            ] {
                assert_eq!(listed[key]["minimum"], range.minimum.as_str(), "{id} {key}");
                assert_eq!(
                    listed[key]["maximum_exclusive"],
                    range.maximum_exclusive.as_str(),
                    "{id} {key}"
                );
            }
        }
    }
}
