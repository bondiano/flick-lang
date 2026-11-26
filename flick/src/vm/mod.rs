//! Built-in in-process VM target (`--target vm`) on `flick_vm::tier0`.

#[cfg(feature = "vm")]
mod lower;
#[cfg(feature = "vm")]
mod run;
#[cfg(feature = "vm")]
mod test;

#[cfg(feature = "vm")]
use std::collections::BTreeMap;
#[cfg(feature = "vm")]
use std::path::PathBuf;
use std::process::ExitCode;

#[cfg(any(test, feature = "vm"))]
use flick_core::diagnostics::suggest;
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::Project;
#[cfg(feature = "vm")]
use flick_core::project::TargetOverride;
#[cfg(any(test, feature = "vm"))]
use flick_core::project::config::TargetValue;
#[cfg(not(feature = "vm"))]
use flick_core::source::SourceMap;

use crate::args::{GlobalArguments, RunArguments, TestArguments};
use crate::backends::options;
#[cfg(not(feature = "vm"))]
use crate::report;

/// Supported VM execution tiers.
#[cfg(any(test, feature = "vm"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VmTier {
    /// Reference counted tier-0 executor (`flick_vm::tier0`).
    Tier0,
}

#[cfg(any(test, feature = "vm"))]
impl VmTier {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "0" | "tier0" => Some(Self::Tier0),
            _ => None,
        }
    }
}

/// Whether the effective target for this command is `vm`.
pub(crate) fn is_vm_target(cli_target: Option<&str>, project: Option<&Project>) -> bool {
    options::resolve_target(cli_target, project).is_ok_and(|(choice, _)| choice.is_vm())
}

/// Validates `[target.vm]` in `project` and `--vm-tier`, returning the active tier.
#[cfg(any(test, feature = "vm"))]
pub(crate) fn resolve_tier(
    cli_tier: Option<&str>,
    project: Option<&Project>,
) -> Result<VmTier, Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();
    let mut toml_tier: Option<VmTier> = None;

    if let Some(table) = project.and_then(|p| p.config.targets.get("vm")) {
        for (key, entry) in table {
            if key != "tier" {
                let mut diag = Diagnostic::error(
                    codes::P058,
                    format!("unknown option `{key}` in `[target.vm]` for `flick/vm`"),
                    entry.key_span,
                );
                if let Some(closest) = suggest::closest(key, ["tier"]) {
                    diag = diag.did_you_mean(entry.key_span, || Some(closest.to_owned()));
                }
                diagnostics.push(diag);
                continue;
            }
            match &entry.value {
                TargetValue::String(text) => {
                    if let Some(tier) = VmTier::parse(text) {
                        toml_tier = Some(tier);
                    } else {
                        let mut diag = Diagnostic::error(
                            codes::P058,
                            format!(
                                "invalid value `{text}` for option `tier` of `flick/vm`; \
                                 expected one of: `0`, `tier0`"
                            ),
                            entry.value_span,
                        );
                        if let Some(closest) = suggest::closest(text, ["0", "tier0"]) {
                            diag = diag
                                .did_you_mean(entry.value_span, || Some(format!("\"{closest}\"")));
                        }
                        diagnostics.push(diag);
                    }
                }
                _ => {
                    diagnostics.push(Diagnostic::error(
                        codes::P058,
                        "option `tier` of `flick/vm` expects a string (`\"0\"` or `\"tier0\"`)",
                        entry.value_span,
                    ));
                }
            }
        }
    }

    let cli_parsed = match cli_tier {
        Some(raw) => {
            if let Some(tier) = VmTier::parse(raw) {
                Some(tier)
            } else {
                let mut diag = Diagnostic::new(
                    codes::P058,
                    Severity::Error,
                    format!(
                        "invalid VM tier `{raw}` for `--vm-tier`; expected one of: `0`, `tier0`"
                    ),
                );
                if let Some(closest) = suggest::closest(raw, ["0", "tier0"]) {
                    diag = diag
                        .with_fix(|| (format!("did you mean `--vm-tier {closest}`?"), Vec::new()));
                }
                diagnostics.push(diag);
                None
            }
        }
        None => None,
    };

    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    Ok(cli_parsed.or(toml_tier).unwrap_or(VmTier::Tier0))
}

/// Builds the compiler `TargetOverride` for the `vm` target.
#[cfg(feature = "vm")]
pub(super) fn target_override(
    target_overridden: bool,
    for_tests: bool,
    test_paths: Vec<PathBuf>,
    no_native_overrides: bool,
) -> TargetOverride {
    TargetOverride {
        target: Some("vm".to_owned()),
        target_overridden,
        backend: None,
        options: BTreeMap::new(),
        rt: Some(false),
        ui: None,
        effect_adapter: None,
        for_tests,
        test_paths,
        skip_backend_check_target: true,
        disable_native_overrides: no_native_overrides,
    }
}

/// Diagnostic emitted when `flick build --target vm` is invoked.
pub(crate) fn unsupported_build_diagnostic() -> Diagnostic {
    Diagnostic::new(
        codes::P012,
        Severity::Error,
        "building standalone VM images (`flick build --target vm`) is not supported yet; \
         use `flick run --target vm` or `flick test --target vm`",
    )
}

/// Diagnostic emitted when `flick dev --target vm` is invoked.
pub(crate) fn unsupported_dev_diagnostic() -> Diagnostic {
    Diagnostic::new(
        codes::P012,
        Severity::Error,
        "`flick dev --target vm` is not supported yet; \
         use `flick run --target vm` or `flick test --target vm`",
    )
}

/// Diagnostic emitted when `flick inspect --emit output --target vm` is invoked.
pub(crate) fn unsupported_inspect_diagnostic() -> Diagnostic {
    Diagnostic::new(
        codes::P012,
        Severity::Error,
        "`flick inspect --emit output --target vm` is not supported yet; \
         use `--emit typed-ir`, `--emit iface`, or `--emit ast`",
    )
}

/// Diagnostic emitted when `--target vm` is selected in a build without the `vm` Cargo feature.
#[cfg(not(feature = "vm"))]
pub(crate) fn disabled_diagnostic() -> Diagnostic {
    Diagnostic::new(
        codes::P013,
        Severity::Error,
        "the `vm` target is not enabled in this build of `flick` \
         (compiled without the `vm` feature)",
    )
}

/// Status lines reported by `flick doctor --target vm`.
pub(crate) fn doctor_lines() -> Vec<(Option<bool>, String, String)> {
    #[cfg(feature = "vm")]
    {
        vec![(
            Some(true),
            "backend".to_owned(),
            "flick/vm (in-process, tier 0)".to_owned(),
        )]
    }
    #[cfg(not(feature = "vm"))]
    {
        vec![(
            Some(false),
            "backend".to_owned(),
            "flick/vm is not enabled in this build (compiled without the `vm` feature)".to_owned(),
        )]
    }
}

/// Runs a project entry or standalone `.fl` file in process on the VM.
pub(crate) fn run(options: &RunArguments, global: &GlobalArguments) -> ExitCode {
    #[cfg(feature = "vm")]
    {
        run::run(options, global)
    }
    #[cfg(not(feature = "vm"))]
    {
        let _ = options;
        let diag = disabled_diagnostic();
        report::render(global, &SourceMap::default(), &[diag]);
        ExitCode::from(2)
    }
}

/// Runs project tests in process on the VM.
pub(crate) fn test(options: &TestArguments, global: &GlobalArguments) -> ExitCode {
    #[cfg(feature = "vm")]
    {
        test::test(options, global)
    }
    #[cfg(not(feature = "vm"))]
    {
        let _ = options;
        let diag = disabled_diagnostic();
        report::render(global, &SourceMap::default(), &[diag]);
        ExitCode::from(2)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use flick_core::project::{Project, config};
    use flick_core::source::SourceMap;

    use super::*;

    fn project_from(toml: &str) -> Project {
        let mut sources = SourceMap::default();
        let file = sources.add("flick.toml", toml);
        Project {
            root: PathBuf::from("/project"),
            config: config::parse(toml, file).expect("valid config"),
            config_file: file,
            workspace: None,
        }
    }

    #[test]
    fn resolve_tier_accepts_default_and_tier0_and_rejects_invalid() {
        let proj = project_from(
            "schema-version = 1\n[project]\nkind = \"lib\"\n[build]\ntarget = \"vm\"\n\
             [target.vm]\ntier = \"tier0\"\n",
        );
        assert_eq!(resolve_tier(None, Some(&proj)), Ok(VmTier::Tier0));
        assert_eq!(resolve_tier(Some("0"), Some(&proj)), Ok(VmTier::Tier0));
        assert_eq!(resolve_tier(Some("tier0"), None), Ok(VmTier::Tier0));

        let bad_cli = resolve_tier(Some("tier9"), None).expect_err("bad cli tier");
        assert_eq!(bad_cli[0].code, codes::P058);

        let bad_toml = project_from(
            "schema-version = 1\n[project]\nkind = \"lib\"\n[target.vm]\ntier = \"2\"\n",
        );
        let errs = resolve_tier(None, Some(&bad_toml)).expect_err("bad toml tier");
        assert_eq!(errs[0].code, codes::P058);
        assert!(errs[0].span.is_some());

        let unknown_key = project_from(
            "schema-version = 1\n[project]\nkind = \"lib\"\n[target.vm]\ntire = \"0\"\n",
        );
        let errs = resolve_tier(None, Some(&unknown_key)).expect_err("unknown key");
        assert_eq!(errs[0].code, codes::P058);
        assert!(errs[0].span.is_some());
    }
}
