//! `[dependencies]`, `[package]` and `[plugins]` of `flick.toml`.

use std::collections::BTreeMap;
use std::ops;

use serde::Deserialize;
use toml::Spanned;

use super::{Dependency, Grants, Package, Source};
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

fn source(raw: RawDependency) -> Result<Source, String> {
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
    Ok(Package {
        name,
        version,
        flick,
        plugin,
        span: at,
    })
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
}

/// The grants of `[plugins]`, by package, each entry checked.
pub(super) fn grants(
    raw: BTreeMap<String, Spanned<RawGrants>>,
    span: &dyn Fn(ops::Range<usize>) -> Span,
) -> Result<BTreeMap<String, Grants>, Diagnostic> {
    let name = |n: &str| {
        !n.is_empty()
            && (n.bytes())
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-.".contains(&b))
    };
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
            let RawGrants { fs_read, net, env } = value.into_inner();
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
            Ok((package, Grants { fs_read, net, env }))
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
