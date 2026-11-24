//! `[dependencies]`, `[package]` and `[plugins]` of `flick.toml`.
//! `flick.toml`.

use std::collections::{BTreeMap, BTreeSet};
use std::ops;

use serde::Deserialize;
use toml::Spanned;

use super::{Dependency, Grants, HostConfig, Package, Source};
use crate::diagnostics::{Diagnostic, codes};
use crate::iface;
use crate::plugin::reach::{inside, net_endpoint};
use crate::source::Span;
use crate::syntax::when;

/// A dependency as written: the keys of one of its sources.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawDependency {
    path: Option<String>,
    git: Option<String>,
    tag: Option<String>,
    subdir: Option<String>,
    url: Option<String>,
    sha256: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawPackage {
    name: String,
    version: String,
    flick: Option<String>,
    plugin: Option<String>,
    #[serde(default)]
    load: Vec<String>,
    #[serde(default)]
    hooks: Vec<String>,
    #[serde(default)]
    watch: Vec<String>,
    when: Option<Spanned<String>>,
    #[serde(default)]
    abi: BTreeMap<Spanned<String>, Spanned<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawHost {
    name: Spanned<String>,
    backend: Spanned<String>,
    requires: Option<Spanned<String>>,
    #[serde(default)]
    tags: Vec<Spanned<String>>,
}

/// The dependencies of `[dependencies]`, each with one source.
pub(super) fn dependencies(
    raw: BTreeMap<String, Spanned<RawDependency>>,
    span: &dyn Fn(ops::Range<usize>) -> Span,
) -> Result<Vec<Dependency>, Diagnostic> {
    raw.into_iter()
        .map(|(name, value)| {
            let at = span(value.span());
            let source = source(value.into_inner()).map_err(|message| {
                Diagnostic::error(codes::P032, format!("dependency `{name}`: {message}"), at)
            })?;
            Ok(Dependency {
                name,
                source,
                span: at,
            })
        })
        .collect()
}

pub(super) fn source(raw: RawDependency) -> Result<Source, String> {
    let RawDependency {
        path,
        git,
        tag,
        subdir,
        url,
        sha256,
    } = raw;
    match (path, git, url) {
        (Some(path), None, None) if tag.is_none() && subdir.is_none() && sha256.is_none() => {
            Ok(Source::Path(path))
        }
        (None, Some(url), None) if sha256.is_none() => match tag {
            Some(tag) => Ok(Source::Git { url, tag, subdir }),
            None => Err("a `git` dependency names its `tag`".to_owned()),
        },
        (None, None, Some(url)) if tag.is_none() && subdir.is_none() => match sha256 {
            Some(sha256) if is_sha256(&sha256) => Ok(Source::Url { url, sha256 }),
            Some(_) => Err("`sha256` is 64 hexadecimal digits".to_owned()),
            None => Err("a `url` dependency names the `sha256` of its tarball".to_owned()),
        },
        _ => Err(
            "give one source: `{ path = \"…\" }`, `{ git = \"…\", tag = \"…\" }` or \
             `{ url = \"…\", sha256 = \"…\" }`"
                .to_owned(),
        ),
    }
}

fn is_sha256(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| b.is_ascii_hexdigit())
}

/// `[package]`, with a `flick` requirement that parses as one.
pub(super) fn package(
    raw: Spanned<RawPackage>,
    at: Span,
    text: &str,
    span: &dyn Fn(ops::Range<usize>) -> Span,
) -> Result<Package, Diagnostic> {
    let RawPackage {
        name,
        version,
        flick,
        plugin,
        load,
        hooks,
        watch,
        when: raw_when,
        abi: raw_abi,
    } = raw.into_inner();
    let error = |message: String| Diagnostic::error(codes::P032, message, at);
    semver::Version::parse(&version).map_err(|e| {
        error(format!(
            "`version = \"{version}\"` is not a semver version: {e}"
        ))
    })?;
    if let Some(flick) = &flick {
        semver::VersionReq::parse(flick).map_err(|e| {
            error(format!(
                "`flick = \"{flick}\"` is not a version requirement: {e}"
            ))
        })?;
    }
    // a fetched library names a file of its own, not one beside it
    if let Some(plugin) = &plugin
        && !inside(plugin)
    {
        return Err(error(format!(
            "`plugin = \"{plugin}\"` must be a path inside the package, without `..`"
        )));
    }
    if !load.is_empty() && plugin.is_none() {
        return Err(error(
            "`load` needs the `plugin` that loads the files".into(),
        ));
    }
    if !hooks.is_empty() && plugin.is_none() {
        return Err(error(
            "`hooks` needs the `plugin` that takes part in them".into(),
        ));
    }
    if !watch.is_empty() && plugin.is_none() {
        return Err(error(
            "`watch` needs the `plugin` that checks the calls".into(),
        ));
    }
    if let Some(bad) = watch.iter().find(|w| !is_watched(w)) {
        return Err(error(format!(
            "`watch = [\"{bad}\"]`: a watched name is `Module.name`, `Module.Sub.name` or `*.name` for the name of any module, `Module.Service` for a service, or a tag, `@maker` or `@cell`"
        )));
    }
    if let Some(bad) = hooks.iter().find(|h| !matches!(h.as_str(), "emit" | "ir")) {
        return Err(error(format!(
            "`hooks = [\"{bad}\"]`: a hook is `emit`, after the build writes its files, or `ir`, before it optimizes"
        )));
    }
    // a module path has no dot of its own, and `.fl` is Flick's
    if let Some(bad) = load.iter().find(|e| {
        e.is_empty()
            || e.as_str() == "fl"
            || !e
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    }) {
        return Err(error(format!(
            "`load = [\"{bad}\"]`: an extension is lowercase letters and digits, without the dot, and not `fl`"
        )));
    }
    let when_cond = match raw_when {
        Some(raw_when) => {
            let literal = span(raw_when.span());
            let cond = raw_when.into_inner();
            if let Err(err) = when::parse(&cond) {
                let place = when::subspan(text, literal, err.range);
                return Err(Diagnostic::error(
                    codes::P032,
                    format!("invalid `[package] when` condition: {err}"),
                    place,
                ));
            }
            Some((cond, literal))
        }
        None => None,
    };
    let abi = parse_package_abi(raw_abi, span)?;
    Ok(Package {
        name,
        version,
        flick,
        plugin,
        load,
        hooks,
        watch,
        when: when_cond,
        abi,
        span: at,
    })
}

fn parse_package_abi(
    raw_abi: BTreeMap<Spanned<String>, Spanned<String>>,
    span: &dyn Fn(ops::Range<usize>) -> Span,
) -> Result<BTreeMap<String, (String, Span)>, Diagnostic> {
    let mut abi = BTreeMap::new();
    for (abi_name, raw_req) in raw_abi {
        let name_span = span(abi_name.span());
        let abi_name = abi_name.into_inner();
        if !is_abi_name(&abi_name) {
            return Err(Diagnostic::error(
                codes::P032,
                format!(
                    "`[package.abi]` key `{abi_name}` is not a valid ABI name: use lowercase ASCII letters, digits, `.`, `-`, or `_`"
                ),
                name_span,
            ));
        }
        let req_span = span(raw_req.span());
        let req = raw_req.into_inner();
        semver::VersionReq::parse(&req).map_err(|e| {
            Diagnostic::error(
                codes::P032,
                format!(
                    "`[package.abi]` `{abi_name} = \"{req}\"` is not a version requirement: {e}"
                ),
                req_span,
            )
        })?;
        abi.insert(abi_name, (req, req_span));
    }
    Ok(abi)
}

fn is_abi_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    first.is_ascii_lowercase()
        && !name.ends_with('.')
        && !name.ends_with('-')
        && !name.contains("..")
        && bytes.all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'_')
        })
}

/// `[host]`: a host environment package for a backend.
pub(super) fn host(
    raw: Spanned<RawHost>,
    at: Span,
    text: &str,
    span: &dyn Fn(ops::Range<usize>) -> Span,
) -> Result<HostConfig, Diagnostic> {
    let RawHost {
        name,
        backend,
        requires,
        tags,
    } = raw.into_inner();
    let name_span = span(name.span());
    let name = name.into_inner();
    if !when::is_valid_tag(&name) {
        return Err(Diagnostic::error(
            codes::P032,
            format!(
                "`[host] name = \"{name}\"` is not a valid host name: names start with a lowercase letter and use lowercase letters, digits, `-`, or `_`"
            ),
            name_span,
        ));
    }
    let backend_span = span(backend.span());
    let backend = backend.into_inner();
    if !is_backend_name(&backend) {
        return Err(Diagnostic::error(
            codes::P032,
            format!(
                "`[host] backend = \"{backend}\"` is not a valid backend name: use a short name like `js` or a backend id like `flick/js`"
            ),
            backend_span,
        ));
    }
    let requires = match requires {
        Some(raw_req) => {
            let literal = span(raw_req.span());
            let cond = raw_req.into_inner();
            if let Err(err) = when::parse(&cond) {
                let place = when::subspan(text, literal, err.range);
                return Err(Diagnostic::error(
                    codes::P032,
                    format!("invalid `[host] requires` condition: {err}"),
                    place,
                ));
            }
            Some((cond, literal))
        }
        None => None,
    };
    let mut seen = BTreeSet::new();
    let mut parsed_tags = Vec::with_capacity(tags.len());
    for raw_tag in tags {
        let tag_span = span(raw_tag.span());
        let tag = raw_tag.into_inner();
        if !when::is_valid_tag(&tag) {
            return Err(Diagnostic::error(
                codes::P032,
                format!(
                    "`[host] tags` contains invalid tag `{tag}`: tags start with a lowercase letter and use lowercase letters, digits, `-`, or `_`"
                ),
                tag_span,
            ));
        }
        if !seen.insert(tag.clone()) {
            return Err(Diagnostic::error(
                codes::P032,
                format!("`[host] tags` lists `{tag}` more than once"),
                tag_span,
            ));
        }
        parsed_tags.push((tag, tag_span));
    }
    Ok(HostConfig {
        name,
        backend,
        requires,
        tags: parsed_tags,
        span: at,
    })
}

fn is_backend_name(name: &str) -> bool {
    match name.split_once('/') {
        Some((scope, short)) => when::is_valid_tag(scope) && when::is_valid_tag(short),
        None => when::is_valid_tag(name),
    }
}

/// Whether `watched` names what a plugin is told of: a value of a module
/// (`Ui.Event.queue`, or `*.parse` for the name of any module), a service
/// (`Atom.Read`), or a tag the compiler follows (`@maker`, `@cell`).
fn is_watched(watched: &str) -> bool {
    if let Some(tag) = watched.strip_prefix('@') {
        return tag == iface::MAKER || tag == iface::CELL;
    }
    let Some((module, name)) = watched.rsplit_once('.') else {
        return false;
    };
    let value = |n: &str| {
        n.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
            && n.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    };
    let part = |p: &str| {
        p.starts_with(|c: char| c.is_ascii_uppercase())
            && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    };
    (value(name) || part(name)) && (module == "*" || module.split('.').all(part))
}

/// What a project grants the plugin of a package, as written.
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(super) struct RawGrants {
    #[serde(default)]
    fs_read: Vec<String>,
    #[serde(default)]
    net: Vec<String>,
    #[serde(default)]
    env: Vec<String>,
    #[serde(default)]
    disable: Vec<String>,
}

/// The grants of `[plugins]`, by package, each entry checked.
pub(super) fn grants(
    raw: BTreeMap<String, Spanned<RawGrants>>,
    span: &dyn Fn(ops::Range<usize>) -> Span,
) -> Result<BTreeMap<String, Grants>, Diagnostic> {
    let variable = |v: &str| {
        v.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
            && v.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    };
    raw.into_iter()
        .map(|(package, value)| {
            let at = span(value.span());
            let RawGrants {
                fs_read,
                net,
                env,
                disable,
            } = value.into_inner();
            let wrong = |what: String| {
                Diagnostic::error(codes::P005, format!("`[plugins.{package}]`: {what}"), at)
            };
            if let Some(p) = fs_read.iter().find(|p| !inside(p)) {
                return Err(wrong(format!(
                    "`fs-read` names `{p}`, which is no path inside the project"
                )));
            }
            if let Some(h) = net.iter().find(|h| net_endpoint(h).is_none()) {
                return Err(wrong(format!(
                    "`net` names `{h}`, which is no host, as `db.example.com`, `localhost:8080` or `[::1]`"
                )));
            }
            if let Some(v) = env.iter().find(|v| !variable(v)) {
                return Err(wrong(format!(
                    "`env` names `{v}`, which is no name of a variable"
                )));
            }
            if let Some(d) = disable.iter().find(|d| !d.split('.').all(variable)) {
                return Err(wrong(format!(
                    "`disable` names `{d}`, which is no head of a block or class, as `sql` or `Cli.Value`"
                )));
            }
            let grants = Grants {
                fs_read,
                net,
                env,
                disable,
                span: Some(at),
            };
            Ok((package, grants))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{RawDependency, source};
    use crate::project::config::Source;

    fn raw(text: &str) -> Result<Source, String> {
        source(toml::from_str::<RawDependency>(text).expect("valid toml"))
    }

    #[test]
    fn a_dependency_has_one_source() {
        assert_eq!(raw("path = \"../lib\""), Ok(Source::Path("../lib".into())));
        assert_eq!(
            raw("git = \"https://x/y\"\ntag = \"v1\"\nsubdir = \"packages/std\""),
            Ok(Source::Git {
                url: "https://x/y".into(),
                tag: "v1".into(),
                subdir: Some("packages/std".into()),
            })
        );
        let sha = "ab".repeat(32);
        assert!(raw(&format!("url = \"https://x/y.tgz\"\nsha256 = \"{sha}\"")).is_ok());
        assert!(raw("git = \"https://x/y\"").is_err());
        assert!(raw("url = \"https://x/y.tgz\"\nsha256 = \"short\"").is_err());
        assert!(raw("path = \"a\"\ngit = \"b\"").is_err());
        assert!(raw("path = \"a\"\ntag = \"v1\"").is_err());
    }
}
