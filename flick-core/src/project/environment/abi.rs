//! Package `[package] abi` compatibility validation (`FLICK-F014`).

use std::collections::{BTreeMap, BTreeSet};

use semver::{Version, VersionReq};

use crate::diagnostics::{Diagnostic, codes};
use crate::project::Project;
use crate::project::packages::Packages;
use crate::source::Span;
use crate::syntax::when;
use crate::types::ForeignBackendSpec;

fn is_package_active(when_cond: Option<&str>, active_tags: Option<&BTreeSet<String>>) -> bool {
    let (Some(cond), Some(tags)) = (when_cond, active_tags) else {
        return true;
    };
    when::parse(cond).is_ok_and(|expr| expr.eval(&|tag| tags.contains(tag)))
}

fn check_one_package(
    pkg_name: &str,
    abi_reqs: &BTreeMap<String, (String, Span)>,
    when_cond: Option<&str>,
    active_tags: Option<&BTreeSet<String>>,
    foreign_backend: &ForeignBackendSpec,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if abi_reqs.is_empty() || !is_package_active(when_cond, active_tags) {
        return;
    }
    let Some(table) = foreign_backend.value_abi.as_ref() else {
        let short = foreign_backend
            .backend_id
            .rsplit('/')
            .next()
            .unwrap_or(&foreign_backend.backend_id);
        let expected_prefix = format!("flick.{short}-ffi");
        if let Some((req, span)) = abi_reqs.get(&expected_prefix) {
            diagnostics.push(Diagnostic::error(
                codes::F014,
                format!(
                    "package `{pkg_name}` requires value ABI `{expected_prefix}` `{req}`, but backend `{}` publishes no versioned value ABI table",
                    foreign_backend.backend_id
                ),
                *span,
            ));
        }
        return;
    };

    if let Some((req_text, span)) = abi_reqs.get(&table.abi.name) {
        let matched = VersionReq::parse(req_text)
            .ok()
            .zip(Version::parse(&table.abi.version).ok())
            .is_some_and(|(req, actual)| req.matches(&actual));
        if !matched {
            diagnostics.push(Diagnostic::error(
                codes::F014,
                format!(
                    "package `{pkg_name}` requires value ABI `{}` `{req_text}`, but backend `{}` provides `{}@{}`",
                    table.abi.name, foreign_backend.backend_id, table.abi.name, table.abi.version
                ),
                *span,
            ));
        }
        return;
    }

    let first_span = abi_reqs.values().next().map(|(_, s)| *s);
    let listed: Vec<String> = abi_reqs
        .iter()
        .map(|(name, (req, _))| format!("`{name}` `{req}`"))
        .collect();
    let message = format!(
        "package `{pkg_name}` requires value ABI {}, but backend `{}` provides `{}@{}`",
        listed.join(", "),
        foreign_backend.backend_id,
        table.abi.name,
        table.abi.version
    );
    if let Some(span) = first_span {
        diagnostics.push(Diagnostic::error(codes::F014, message, span));
    }
}

/// Validates `[package] abi` requirements of the root project and all active
/// dependency packages against the selected backend's published value-ABI table.
pub(super) fn check_package_abis(
    project: Option<&Project>,
    packages: &Packages,
    active_tags: Option<&BTreeSet<String>>,
    foreign_backend: &ForeignBackendSpec,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some(pkg) = project.and_then(|p| p.config.package.as_ref()) {
        check_one_package(
            &pkg.name,
            &pkg.abi,
            pkg.when.as_ref().map(|(c, _)| c.as_str()),
            active_tags,
            foreign_backend,
            diagnostics,
        );
    }
    for (name, abi_reqs, when_cond) in packages.package_abi() {
        check_one_package(
            name,
            abi_reqs,
            when_cond,
            active_tags,
            foreign_backend,
            diagnostics,
        );
    }
}
