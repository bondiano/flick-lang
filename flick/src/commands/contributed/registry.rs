//! Contributor discovery, loading, and descriptor caching for built-in `std`,
//! project backends, and dependency plugins.

use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::Arc;

use flick_backend::command::{self, Descriptor, Invocation};
use flick_backend::host::runtime::Cancellation;
use flick_core::plugin::first_party;
use flick_core::project::{Project, store};
use flick_plugin::Plugin;
use sha2::{Digest, Sha256};

use crate::commands::plugins as plugin_commands;
use crate::{backends, fetch, plugins, saved};

/// What runs the commands of one backend or plugin.
pub(crate) trait Contributor: Send + Sync {
    fn describe(&self) -> Result<Vec<Descriptor>, String>;
    fn run(&self, invocation: &Invocation) -> Result<command::Plan, Vec<command::Diagnostic>>;
    fn resume(
        &self,
        state: Vec<u8>,
        outputs: Vec<command::File>,
        answers: Vec<command::Answer>,
    ) -> Result<command::Plan, Vec<command::Diagnostic>>;
    fn open(&self, invocation: &Invocation) -> Result<command::Opened, Vec<command::Diagnostic>>;
    fn handle(
        &self,
        state: Vec<u8>,
        request: Vec<u8>,
        answers: &[command::Answer],
        cancel: &Cancellation,
    ) -> Result<command::Handled, String>;
    fn close(&self, state: Vec<u8>) -> command::Plan;
    fn templates(&self) -> Result<Vec<command::TemplateDescriptor>, String>;
    fn scaffold(
        &self,
        request: &command::TemplateRequest,
    ) -> Result<command::Plan, Vec<command::Diagnostic>>;
}

/// First-party native plugin contributor (`std`) wrapped with panic guards so
/// its commands run in-process without WASM instantiation or disk cache files.
pub(super) struct NativeContributor {
    plugin: &'static (dyn Plugin + Send + Sync),
}

impl NativeContributor {
    pub(super) fn std() -> Self {
        Self {
            plugin: first_party::std_commands(),
        }
    }
}

fn guarded<T>(call: impl FnOnce() -> T) -> Result<T, String> {
    panic::catch_unwind(AssertUnwindSafe(call)).map_err(|payload| {
        if let Some(msg) = payload.downcast_ref::<&str>() {
            format!("contributor panicked: {msg}")
        } else if let Some(msg) = payload.downcast_ref::<String>() {
            format!("contributor panicked: {msg}")
        } else {
            "contributor panicked".into()
        }
    })
}

impl Contributor for NativeContributor {
    fn describe(&self) -> Result<Vec<Descriptor>, String> {
        guarded(|| self.plugin.commands())
    }

    fn run(&self, invocation: &Invocation) -> Result<command::Plan, Vec<command::Diagnostic>> {
        match guarded(|| self.plugin.run(invocation)) {
            Ok(result) => result,
            Err(why) => Err(vec![command::Diagnostic::error("P013", why)]),
        }
    }

    fn resume(
        &self,
        state: Vec<u8>,
        outputs: Vec<command::File>,
        answers: Vec<command::Answer>,
    ) -> Result<command::Plan, Vec<command::Diagnostic>> {
        match guarded(|| self.plugin.resume(state, outputs, answers)) {
            Ok(result) => result,
            Err(why) => Err(vec![command::Diagnostic::error("P013", why)]),
        }
    }

    fn open(&self, invocation: &Invocation) -> Result<command::Opened, Vec<command::Diagnostic>> {
        match guarded(|| self.plugin.open(invocation)) {
            Ok(result) => result,
            Err(why) => Err(vec![command::Diagnostic::error("P013", why)]),
        }
    }

    fn handle(
        &self,
        state: Vec<u8>,
        request: Vec<u8>,
        answers: &[command::Answer],
        _cancel: &Cancellation,
    ) -> Result<command::Handled, String> {
        guarded(|| self.plugin.handle(state, request, answers.to_vec()))?
    }

    fn close(&self, state: Vec<u8>) -> command::Plan {
        guarded(|| self.plugin.close(state)).unwrap_or_else(|_| command::Plan::empty())
    }

    fn templates(&self) -> Result<Vec<command::TemplateDescriptor>, String> {
        guarded(|| self.plugin.templates())
    }

    fn scaffold(
        &self,
        request: &command::TemplateRequest,
    ) -> Result<command::Plan, Vec<command::Diagnostic>> {
        match guarded(|| self.plugin.scaffold(request)) {
            Ok(result) => result,
            Err(why) => Err(vec![command::Diagnostic::error("P013", why)]),
        }
    }
}

#[derive(Debug)]
pub(super) enum SourceKind {
    /// Selected or official backend component.
    Backend { official: bool },
    /// Dependency plugin WASM component.
    Plugin(PathBuf),
    /// Built-in first-party `std` contributor (in-memory, no WASM or cache).
    BuiltinStd,
}

/// A backend or a plugin of the project that may add commands.
#[derive(Debug)]
pub(super) struct Source {
    /// The prefix of its qualified names: a plugin's package, or the last
    /// part of a backend's id.
    pub(super) short: String,
    /// What `[tools]` grants it by.
    pub(super) id: String,
    /// The digest of its component, which keys its cached descriptors.
    pub(super) digest: String,
    pub(super) kind: SourceKind,
}

pub(super) fn builtin_std_source() -> Source {
    Source {
        short: first_party::PACKAGE.into(),
        id: first_party::PACKAGE.into(),
        digest: String::new(),
        kind: SourceKind::BuiltinStd,
    }
}

/// Cached descriptors, by the digest of their component.
#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct Cached {
    pub(super) short: String,
    pub(super) id: String,
    pub(super) descriptors: Vec<Descriptor>,
}

fn cache_path(digest: &str) -> Option<PathBuf> {
    Some(
        store::store_dir()?
            .join("commands")
            .join(format!("{digest}.cbor")),
    )
}

pub(super) fn read_cache(digest: &str) -> Option<Cached> {
    let bytes = fs::read(cache_path(digest)?).ok()?;
    ciborium::from_reader(bytes.as_slice()).ok()
}

/// The built-in `std`, the backend and the plugins of `project`, without loading a component.
pub(super) fn sources(project: &Project) -> Vec<Source> {
    let mut sources = vec![builtin_std_source()];
    if let Some((id, digest)) = backends::selected_pin(project) {
        let short = id.rsplit('/').next().unwrap_or(&id).to_owned();
        sources.push(Source {
            short,
            id,
            digest,
            kind: SourceKind::Backend { official: false },
        });
    }
    for (package, path) in plugin_commands::components(project) {
        let Ok(bytes) = fs::read(&path) else { continue };
        sources.push(Source {
            short: package.clone(),
            id: package,
            digest: fetch::hex(&Sha256::digest(&bytes)),
            kind: SourceKind::Plugin(path),
        });
    }
    sources
}

pub(super) fn load(project: &Project, source: &Source) -> Result<Arc<dyn Contributor>, String> {
    match &source.kind {
        SourceKind::BuiltinStd => Ok(Arc::new(NativeContributor::std())),
        SourceKind::Plugin(path) => plugins::contributor(&source.id, path),
        SourceKind::Backend { official: true } => {
            backends::official_contributor(project, &source.id)
        }
        SourceKind::Backend { official: false } => backends::contributor(project),
    }
}

/// Every contributor the store has seen, plus built-in `std` and official backends.
pub(super) fn known(project: &Project) -> Vec<(String, Vec<Descriptor>)> {
    for (id, digest) in backends::officials() {
        let short = id.rsplit('/').next().unwrap_or(&id).to_owned();
        let source = Source {
            short,
            id,
            digest,
            kind: SourceKind::Backend { official: true },
        };
        let _ = descriptors(project, &source);
    }
    let mut all = Vec::new();
    if let Ok(std_ds) = NativeContributor::std().describe() {
        all.push((first_party::PACKAGE.into(), std_ds));
    }
    all.extend(cached_everywhere());
    all
}

/// The descriptors of `source`, in-memory for `std` or from the cache, else
/// from its component, which the cache then keeps.
pub(super) fn descriptors(project: &Project, source: &Source) -> Result<Vec<Descriptor>, String> {
    if matches!(source.kind, SourceKind::BuiltinStd) {
        return NativeContributor::std().describe();
    }
    if let Some(cached) = read_cache(&source.digest) {
        return Ok(cached.descriptors);
    }
    let descriptors = load(project, source)?.describe()?;
    let cached = Cached {
        short: source.short.clone(),
        id: source.id.clone(),
        descriptors: descriptors.clone(),
    };
    if let Some(path) = cache_path(&source.digest) {
        let mut bytes = Vec::new();
        if ciborium::into_writer(&cached, &mut bytes).is_ok() {
            let _ = saved::write(&path, &bytes);
        }
    }
    Ok(descriptors)
}

pub(super) fn cached_everywhere() -> Vec<(String, Vec<Descriptor>)> {
    let Some(dir) = store::store_dir().map(|d| d.join("commands")) else {
        return Vec::new();
    };
    let mut found: Vec<(String, Vec<Descriptor>)> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| fs::read(e.path()).ok())
        .filter_map(|bytes| ciborium::from_reader::<Cached, _>(bytes.as_slice()).ok())
        .map(|c| (c.id, c.descriptors))
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}
