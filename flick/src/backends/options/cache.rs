//! Descriptor cache (`<store>/descriptors/<sha256>.cbor`) and backend
//! descriptor lookup.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read as _;
use std::path::PathBuf;

use flick_backend::host::{Budget, WasmBackend};
use flick_backend::{Backend as _, BackendId, BackendPin, Descriptor, Limits};
use flick_core::diagnostics::Diagnostic;
use flick_core::project::{Project, store};
use semver::Version;

use super::{OFFICIAL_GO, OFFICIAL_JS, TargetChoice};
use crate::backends::official;
use crate::component;
use crate::saved;

fn descriptor_cache_path(sha256: &str) -> Option<PathBuf> {
    Some(
        store::store_dir()?
            .join("descriptors")
            .join(format!("{sha256}.cbor")),
    )
}

fn read_descriptor(sha256: &str) -> Option<Descriptor> {
    let bytes = fs::read(descriptor_cache_path(sha256)?).ok()?;
    ciborium::from_reader(bytes.as_slice()).ok()
}

pub(crate) fn save_descriptor(sha256: &str, descriptor: &Descriptor) {
    let Some(path) = descriptor_cache_path(sha256) else {
        return;
    };
    let mut bytes = Vec::new();
    if ciborium::into_writer(descriptor, &mut bytes).is_ok() {
        let _ = saved::write(&path, &bytes);
    }
}

fn load_and_cache_descriptor(pin: BackendPin, bytes: &[u8]) -> Option<Descriptor> {
    let engine = component::engine().ok()?;
    let cancel = component::cancellation().ok()?.clone();
    let sha = pin.sha256.clone();
    let backend =
        WasmBackend::load_with_cancellation(engine, pin, bytes, Budget::default(), cancel).ok()?;
    let descriptor = backend.describe();
    save_descriptor(&sha, &descriptor);
    Some(descriptor)
}

fn read_cached_descriptor_by_id(id: &str) -> Option<Descriptor> {
    let dir = store::store_dir()?.join("descriptors");
    let mut best: Option<(Version, String, Descriptor)> = None;
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        let Ok(desc) = ciborium::from_reader::<Descriptor, _>(bytes.as_slice()) else {
            continue;
        };
        if desc.identity.id != id {
            continue;
        }
        if !official::accepts(&desc.backend_api, flick_backend::VERSION)
            || !official::accepts(&desc.ir, flick_ir::VERSION)
        {
            continue;
        }
        let Ok(version) = Version::parse(&desc.identity.version) else {
            continue;
        };
        let name = path.to_string_lossy().into_owned();
        let replace = match &best {
            None => true,
            Some((best_ver, best_name, _)) => (&version, &name) > (best_ver, best_name),
        };
        if replace {
            best = Some((version, name, desc));
        }
    }
    best.map(|(_, _, desc)| desc)
}

/// Obtain the descriptor for `target`. When `allow_fetch` is false (such as
/// `--help` or cross-backend suggestions), never reaches the network.
pub(super) fn descriptor_for(
    target: &TargetChoice,
    project: Option<&Project>,
    allow_fetch: bool,
    offline: bool,
) -> (Option<Descriptor>, Option<Diagnostic>) {
    match target {
        TargetChoice::Pinned { package, .. } => {
            if let Some(cached) = read_descriptor(&package.sha256) {
                return (Some(cached), None);
            }
            let Some(project) = project else {
                return (None, None);
            };
            let limit = Limits::default().artifact_bytes;
            let mut bytes = Vec::new();
            let ok = fs::File::open(project.root.join(&package.component))
                .and_then(|f| {
                    f.take(u64::try_from(limit).unwrap_or(u64::MAX) + 1)
                        .read_to_end(&mut bytes)
                })
                .is_ok();
            if !ok || bytes.len() > limit {
                return (None, None);
            }
            let pin = BackendPin {
                identity: BackendId {
                    id: package.id.clone(),
                    version: package.version.clone(),
                },
                sha256: package.sha256.clone(),
            };
            (load_and_cache_descriptor(pin, &bytes), None)
        }
        TargetChoice::Official { id, .. } => {
            let known_pin = official::known(id, project);
            if let Some(pin) = &known_pin
                && let Some(cached) = read_descriptor(&pin.sha256)
            {
                return (Some(cached), None);
            }
            if let Some((pin, bytes)) = official::local_bytes(id, project) {
                return (load_and_cache_descriptor(pin, &bytes), None);
            }
            if known_pin.is_none()
                && let Some(cached) = read_cached_descriptor_by_id(id)
            {
                return (Some(cached), None);
            }
            if allow_fetch {
                return match official::fetch_for_descriptor(id, project, offline) {
                    Ok((pin, bytes)) => (load_and_cache_descriptor(pin, &bytes), None),
                    Err(diag) => (None, Some(diag)),
                };
            }
            (read_cached_descriptor_by_id(id), None)
        }
    }
}

pub(super) fn all_known_descriptors(project: Option<&Project>) -> Vec<Descriptor> {
    let mut by_id: BTreeMap<String, Descriptor> = BTreeMap::new();
    for (id, short) in [(OFFICIAL_JS, "js"), (OFFICIAL_GO, "go")] {
        let choice = TargetChoice::Official { id, short };
        if let (Some(desc), _) = descriptor_for(&choice, project, false, true) {
            by_id.insert(desc.identity.id.clone(), desc);
        }
    }
    if let Some(dir) = store::store_dir().map(|d| d.join("descriptors")) {
        for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
            if let Ok(bytes) = fs::read(entry.path())
                && let Ok(desc) = ciborium::from_reader::<Descriptor, _>(bytes.as_slice())
            {
                by_id.entry(desc.identity.id.clone()).or_insert(desc);
            }
        }
    }
    by_id.into_values().collect()
}
