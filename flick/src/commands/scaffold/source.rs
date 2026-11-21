//! Template contributor resolution for `flick new` and `flick init`:
//! official `--target` backends and third-party `--from` packages (`path`,
//! `url` with `--sha256`, or `git` with `--tag` and optional `--subdir`).

use std::env;
use std::fs;
use std::io::Read as _;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use flick_backend::{BackendPin, Limits};
use flick_core::diagnostics::{Code, Diagnostic, Severity, codes, suggest};
use flick_core::project::config::{self, Source};
use flick_core::project::lock::{Lock, LockedBackend, LockedTemplate};
use flick_core::project::{self, CONFIG_FILE};
use flick_core::source::SourceMap;
use semver::{Version, VersionReq};
use sha2::{Digest, Sha256};

use super::resolve_path;
use crate::args::ScaffoldArguments;
use crate::backends::options::{OFFICIAL_GO, OFFICIAL_JS};
use crate::commands::contributed::Contributor;
use crate::toolchain::COMPILER;
use crate::{backends, fetch, plugins};

pub(super) enum LockPin {
    OfficialBackend(Option<LockedBackend>),
    Template(LockedTemplate),
}

pub(super) struct LoadedSource {
    pub(super) contributor: Arc<dyn Contributor>,
    pub(super) pin: LockPin,
}

pub(super) fn load_source(
    dir: &Path,
    options: &ScaffoldArguments,
    existing_lock: Option<&Lock>,
    offline: bool,
    sources: &mut SourceMap,
) -> Result<LoadedSource, Diagnostic> {
    if options.from.is_some() {
        return load_from_source(dir, options, offline, sources);
    }
    let id = resolve_official_target(options.target.as_deref())?;
    let (contributor, locked_backend) = backends::scaffold_target(id, existing_lock, offline)?;
    Ok(LoadedSource {
        contributor,
        pin: LockPin::OfficialBackend(locked_backend),
    })
}

fn load_from_source(
    dir: &Path,
    options: &ScaffoldArguments,
    offline: bool,
    sources: &mut SourceMap,
) -> Result<LoadedSource, Diagnostic> {
    let source = parse_from_source(options)?;
    let (pkg_dir, source_pin, tree_checksum, archive) = match &source {
        Source::Path(raw) => {
            let cwd = env::current_dir().map_err(|e| {
                Diagnostic::new(
                    codes::P032,
                    Severity::Error,
                    format!("cannot determine working directory: {e}"),
                )
            })?;
            let candidate = project::normalize(&cwd.join(raw));
            if !candidate.is_dir() {
                return Err(Diagnostic::new(
                    codes::P032,
                    Severity::Error,
                    format!("template path `{raw}` is not a directory"),
                ));
            }
            let abs_pkg = fs::canonicalize(&candidate).unwrap_or(candidate);
            let abs_target = resolve_path(dir);
            let rel = fetch::relative(&abs_target, &abs_pkg);
            (abs_pkg, format!("path+{rel}"), None, None)
        }
        Source::Git { .. } | Source::Url { .. } => {
            let fetched = fetch::fetch_template(&source, offline).map_err(|err| match err {
                fetch::FetchTemplateError::Fetch(msg) => {
                    Diagnostic::new(codes::P034, Severity::Error, msg)
                }
                fetch::FetchTemplateError::Escape(msg) => {
                    Diagnostic::new(codes::P055, Severity::Error, msg)
                }
            })?;
            (
                fetched.dir,
                fetched.source,
                Some(fetched.checksum),
                fetched.archive,
            )
        }
    };

    if has_package_entry(&pkg_dir, CONFIG_FILE) {
        load_plugin_template(&pkg_dir, source_pin, tree_checksum, archive, sources)
    } else if has_package_entry(&pkg_dir, "backend.json") {
        load_backend_template(&pkg_dir, source_pin, tree_checksum, archive)
    } else {
        Err(Diagnostic::new(
            codes::P032,
            Severity::Error,
            format!(
                "template package at `{}` contains neither `{CONFIG_FILE}` (with `[package] plugin`) nor `backend.json`",
                pkg_dir.display()
            ),
        ))
    }
}

fn has_package_entry(dir: &Path, name: &str) -> bool {
    fs::symlink_metadata(dir.join(name))
        .is_ok_and(|meta| meta.is_file() || meta.file_type().is_symlink())
}

fn resolve_official_target(requested: Option<&str>) -> Result<&'static str, Diagnostic> {
    match requested {
        None | Some("js" | OFFICIAL_JS) => Ok(OFFICIAL_JS),
        Some("go" | OFFICIAL_GO) => Ok(OFFICIAL_GO),
        Some(other) => {
            let mut diag = Diagnostic::new(
                codes::P059,
                Severity::Error,
                format!("unknown target backend `{other}`; available targets: `js`, `go`"),
            );
            if let Some(suggestion) = suggest::closest(other, ["js", "go"]) {
                diag = diag.with_fix(|| (format!("did you mean `--target {suggestion}`?"), vec![]));
            } else {
                diag = diag.with_fix(|| {
                    (
                        "use `--from <source>` to scaffold from a third-party template package"
                            .to_owned(),
                        vec![],
                    )
                });
            }
            Err(diag)
        }
    }
}

fn is_safe_rel_path(path: &str) -> bool {
    !path.is_empty()
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

fn parse_from_source(options: &ScaffoldArguments) -> Result<Source, Diagnostic> {
    let raw = options.from.as_deref().expect("--from checked");
    let bad = |msg: String| Diagnostic::new(codes::P032, Severity::Error, msg);
    if let Some(tag) = &options.tag {
        let url = raw.strip_prefix("git+").unwrap_or(raw).to_owned();
        if url.is_empty() {
            return Err(bad("`--from` git URL cannot be empty".into()));
        }
        if let Some(sub) = &options.subdir
            && !is_safe_rel_path(sub)
        {
            return Err(bad(format!(
                "`--subdir \"{sub}\"` must be a relative path inside the repository, without `..`"
            )));
        }
        return Ok(Source::Git {
            url,
            tag: tag.clone(),
            subdir: options.subdir.clone(),
        });
    }
    if let Some(sha256) = &options.sha256 {
        if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(bad("`--sha256` must be 64 hexadecimal digits".into()));
        }
        let url = raw.strip_prefix("url+").unwrap_or(raw).to_owned();
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(bad(
                "`--sha256` requires an `http://` or `https://` tarball URL in `--from`".into(),
            ));
        }
        return Ok(Source::Url {
            url,
            sha256: sha256.clone(),
        });
    }
    if raw.starts_with("http://")
        || raw.starts_with("https://")
        || raw.starts_with("url+")
        || raw.starts_with("git+")
        || raw.starts_with("file://")
    {
        return Err(bad(format!(
            "remote `--from \"{raw}\"` requires verification: pass `--sha256 <64-hex>` for a tarball URL or `--tag <tag>` for a git repository"
        )));
    }
    let path = raw.strip_prefix("path+").unwrap_or(raw).to_owned();
    if path.is_empty() {
        return Err(bad("`--from` path cannot be empty".into()));
    }
    Ok(Source::Path(path))
}

fn read_contained_file(
    pkg_dir: &Path,
    rel: &str,
    read_code: Code,
) -> Result<(PathBuf, Vec<u8>), Diagnostic> {
    if !is_safe_rel_path(rel) {
        return Err(Diagnostic::new(
            codes::P032,
            Severity::Error,
            format!("package path `{rel}` must be a relative path inside the package"),
        ));
    }
    let root = fs::canonicalize(pkg_dir).map_err(|e| {
        Diagnostic::new(
            codes::P032,
            Severity::Error,
            format!("cannot inspect `{}`: {e}", pkg_dir.display()),
        )
    })?;
    let candidate = pkg_dir.join(rel);
    let resolved = fs::canonicalize(&candidate).map_err(|e| {
        Diagnostic::new(
            read_code,
            Severity::Error,
            format!("cannot read `{}`: {e}", candidate.display()),
        )
    })?;
    if !resolved.starts_with(&root) {
        return Err(Diagnostic::new(
            codes::P055,
            Severity::Error,
            format!("`{rel}` escapes its template package directory"),
        ));
    }
    if !resolved.is_file() {
        return Err(Diagnostic::new(
            read_code,
            Severity::Error,
            format!("cannot read `{}`: not a regular file", candidate.display()),
        ));
    }
    let limit = Limits::default().artifact_bytes;
    let mut bytes = Vec::new();
    fs::File::open(&resolved)
        .and_then(|f| {
            f.take(u64::try_from(limit).unwrap_or(u64::MAX) + 1)
                .read_to_end(&mut bytes)
        })
        .map_err(|e| {
            Diagnostic::new(
                read_code,
                Severity::Error,
                format!("cannot read `{}`: {e}", candidate.display()),
            )
        })?;
    if bytes.len() > limit {
        return Err(Diagnostic::new(
            codes::P013,
            Severity::Error,
            format!("`{rel}` exceeds its file size limit"),
        ));
    }
    Ok((resolved, bytes))
}

fn load_plugin_template(
    pkg_dir: &Path,
    source_pin: String,
    tree_checksum: Option<String>,
    archive: Option<String>,
    sources: &mut SourceMap,
) -> Result<LoadedSource, Diagnostic> {
    let (manifest_path, raw_manifest) = read_contained_file(pkg_dir, CONFIG_FILE, codes::P032)?;
    let text = String::from_utf8(raw_manifest).map_err(|_| {
        Diagnostic::new(
            codes::P032,
            Severity::Error,
            format!("`{}` is not valid UTF-8", manifest_path.display()),
        )
    })?;
    let file = sources.add(manifest_path.display().to_string(), &text);
    let config = config::parse(&text, file)?;
    let Some(package) = config.package else {
        return Err(Diagnostic::new(
            codes::P032,
            Severity::Error,
            format!(
                "template package `{}` has no `[package]` section",
                manifest_path.display()
            ),
        ));
    };
    if let Some(requirement) = &package.flick {
        let matches = VersionReq::parse(requirement)
            .ok()
            .zip(Version::parse(COMPILER).ok())
            .is_some_and(|(req, ver)| req.matches(&ver));
        if !matches {
            return Err(Diagnostic::new(
                codes::P033,
                Severity::Error,
                format!(
                    "template package `{}` needs flick {requirement}, and this is flick {COMPILER}",
                    package.name
                ),
            ));
        }
    }
    let Some(plugin_rel) = package.plugin else {
        return Err(Diagnostic::new(
            codes::P032,
            Severity::Error,
            format!(
                "template package `{}` declares no `[package] plugin` component",
                package.name
            ),
        ));
    };
    let (real_component, bytes) = read_contained_file(pkg_dir, &plugin_rel, codes::P035)?;
    let component_digest = format!("sha256:{}", fetch::hex(&Sha256::digest(&bytes)));
    let contributor = plugins::contributor(&package.name, &real_component)
        .map_err(|why| Diagnostic::new(codes::P035, Severity::Error, why))?;
    let locked = LockedTemplate {
        template: String::new(),
        package: package.name,
        version: package.version,
        source: source_pin,
        checksum: tree_checksum.unwrap_or_else(|| component_digest.clone()),
        component: component_digest,
        archive,
    };
    Ok(LoadedSource {
        contributor,
        pin: LockPin::Template(locked),
    })
}

fn load_backend_template(
    pkg_dir: &Path,
    source_pin: String,
    tree_checksum: Option<String>,
    archive: Option<String>,
) -> Result<LoadedSource, Diagnostic> {
    let (pin_path, raw_pin) = read_contained_file(pkg_dir, "backend.json", codes::P032)?;
    let pin: BackendPin = serde_json::from_slice(&raw_pin).map_err(|e| {
        Diagnostic::new(
            codes::P032,
            Severity::Error,
            format!("invalid `{}`: {e}", pin_path.display()),
        )
    })?;
    let (_, bytes) = read_contained_file(pkg_dir, "component.wasm", codes::P013)?;
    let contributor = backends::scaffold_package(&pin, &bytes)?;
    let component_digest = format!("sha256:{}", pin.sha256);
    let locked = LockedTemplate {
        template: String::new(),
        package: pin.identity.id,
        version: pin.identity.version,
        source: source_pin,
        checksum: tree_checksum.unwrap_or_else(|| component_digest.clone()),
        component: component_digest,
        archive,
    };
    Ok(LoadedSource {
        contributor,
        pin: LockPin::Template(locked),
    })
}
