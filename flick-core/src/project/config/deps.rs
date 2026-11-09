//! `[dependencies]`, `[js.dependencies]`, `[package]` and `[plugins]` of
//! `flick.toml`.

use std::collections::BTreeMap;
use std::ops;

use serde::Deserialize;
use toml::Spanned;

use super::{Dependency, Grants, JsDependency, Package, Source};
use crate::diagnostics::{Diagnostic, codes};
use crate::plugin::reach::inside;
use crate::source::Span;

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
}

/// `[js]` as written: its only table is `dependencies`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawJs {
    #[serde(default)]
    dependencies: BTreeMap<String, Spanned<String>>,
}

/// The npm packages of `[js.dependencies]`. What a fetched library writes
/// here reaches the project's `package.json`, so an entry names a package of
/// the registry: no path, no URL, no `workspace:` or `file:` protocol.
pub(super) fn js_dependencies(
    raw: Option<RawJs>,
    span: &dyn Fn(ops::Range<usize>) -> Span,
) -> Result<Vec<JsDependency>, Diagnostic> {
    (raw.into_iter().flat_map(|js| js.dependencies))
        .map(|(name, version)| {
            let at = span(version.span());
            let version = version.into_inner();
            let error = |what: String| {
                Diagnostic::error(codes::P032, format!("`[js.dependencies]`: {what}"), at)
            };
            if !is_npm_name(&name) {
                return Err(error(format!("`{name}` is not the name of an npm package")));
            }
            if !is_registry_version(&version) {
                return Err(error(format!(
                    "`{name} = \"{version}\"` is not a version or range of the registry"
                )));
            }
            Ok(JsDependency {
                name,
                version,
                span: at,
            })
        })
        .collect()
}

/// `name` or `@scope/name`, which cannot leave `node_modules`.
fn is_npm_name(name: &str) -> bool {
    let part = |p: &str| {
        !p.is_empty()
            && !p.starts_with('.')
            && p.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~'))
    };
    match name.strip_prefix('@') {
        Some(scoped) => scoped
            .split_once('/')
            .is_some_and(|(scope, name)| part(scope) && part(name)),
        None => part(name),
    }
}

fn is_registry_version(version: &str) -> bool {
    !version.trim().is_empty() && !version.contains([':', '/', '\\'])
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
pub(super) fn package(raw: Spanned<RawPackage>, at: Span) -> Result<Package, Diagnostic> {
    let RawPackage {
        name,
        version,
        flick,
        plugin,
        load,
        hooks,
        watch,
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
            "`watch = [\"{bad}\"]`: a watched name is `Module.name`, `Module.Sub.name` or `*.name` for the name of any module"
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
    Ok(Package {
        name,
        version,
        flick,
        plugin,
        load,
        hooks,
        watch,
        span: at,
    })
}

/// Whether `watched` names a value of a module: `Ui.Event.queue`, or
/// `*.parse` for the name of any module.
fn is_watched(watched: &str) -> bool {
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
    value(name) && (module == "*" || module.split('.').all(part))
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
    // labels of lower-case letters, digits and inner `-`, between dots
    let label = |l: &str| {
        !l.is_empty()
            && !l.starts_with('-')
            && !l.ends_with('-')
            && (l.bytes()).all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    };
    let name = |n: &str| n.split('.').all(label);
    let host = |h: &str| match h.split_once(':') {
        Some((n, port)) => name(n) && port.parse::<u16>().is_ok(),
        None => name(h),
    };
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
            if let Some(h) = net.iter().find(|h| !host(h)) {
                return Err(wrong(format!(
                    "`net` names `{h}`, which is no host, as `db.example.com` or `localhost:8080`"
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
    use super::{RawDependency, is_npm_name, is_registry_version, source};
    use crate::project::config::Source;

    fn raw(text: &str) -> Result<Source, String> {
        source(toml::from_str::<RawDependency>(text).expect("valid toml"))
    }

    #[test]
    fn a_js_dependency_names_a_package_of_the_registry() {
        for name in ["left-pad", "@acme/left.pad", "lodash_es"] {
            assert!(is_npm_name(name), "{name}");
        }
        for name in ["", "../up", "@acme", "@acme/", "a/b", ".hidden", "@a/../b"] {
            assert!(!is_npm_name(name), "{name}");
        }
        for version in ["1.3.0", "^1.3.0", ">=1 <3", "1.x || 2.x", "latest"] {
            assert!(is_registry_version(version), "{version}");
        }
        for version in ["", " ", "file:../x", "workspace:*", "git+ssh://x/y", "../x"] {
            assert!(!is_registry_version(version), "{version}");
        }
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
