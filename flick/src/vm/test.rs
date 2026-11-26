//! In-process `flick test --target vm` execution on `flick_vm::tier0`.

use std::io::{self, IsTerminal as _, Write as _};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use std::{env, fmt::Write as _};

use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::ir::memory::Name as MemoryName;
use flick_core::project;
use flick_core::source::SourceMap;
use flick_vm::tier0::{Heap, TestCaseResult, execute_tests_program};

use super::{VmTier, lower, resolve_tier, target_override};
use crate::args::{Color, GlobalArguments, Reporter, TestArguments};
use crate::backends::options;
use crate::commands::build;
use crate::commands::check::select;
use crate::progress::{Mode, Progress};
use crate::sources::{self, FsHost};
use crate::{builtins, package_cache, report};

#[derive(Clone)]
struct ReportItem {
    name: String,
    status: &'static str,
    kind: Option<&'static str>,
    message: Option<String>,
}

#[allow(clippy::too_many_lines)]
pub(super) fn test(options: &TestArguments, global: &GlobalArguments) -> ExitCode {
    if options.watch {
        let diag = Diagnostic::new(
            codes::P012,
            Severity::Error,
            "`flick test --watch --target vm` is not supported yet; run without `--watch`",
        );
        report::render(global, &SourceMap::default(), &[diag]);
        return ExitCode::from(2);
    }
    let progress = Progress::new(if matches!(options.reporter, Reporter::Json) {
        Mode::Hidden
    } else {
        Mode::for_cli(global)
    });
    let mut sources = SourceMap::default();
    let host = FsHost::new();
    let fail = |sources: &SourceMap, diagnostics: Vec<Diagnostic>| -> ExitCode {
        report::render(global, sources, &diagnostics);
        report::exit_code(&diagnostics)
    };
    progress.stage("Resolving project");
    let (project, mut roots) = match select(&host, &mut sources, global.project.as_deref(), &[]) {
        Ok(selected) => selected,
        Err(diagnostic) => {
            drop(progress);
            return fail(&sources, vec![diagnostic]);
        }
    };
    let Some(project) = project else {
        return ExitCode::from(1);
    };
    let tier = match resolve_tier(options.vm_tier.as_deref(), Some(&project)) {
        Ok(tier) => tier,
        Err(diagnostics) => {
            drop(progress);
            return fail(&sources, diagnostics);
        }
    };
    let target_overridden = options::resolve_target(options.target.as_deref(), Some(&project))
        .is_ok_and(|(_, overridden)| overridden);
    let mut companions = Vec::new();
    let skip = sources::skip_dirs(Some(&project));
    if project.source_root().is_dir() {
        sources::companions_skipping(
            &project.source_root().to_string_lossy(),
            &skip,
            &mut companions,
        );
    }
    roots.extend(companions.into_iter().map(PathBuf::from));
    let wanted: Vec<PathBuf> = options
        .paths
        .iter()
        .map(|p| project::normalize(&host.cwd.join(p)))
        .collect();
    progress.stage("Preparing compiler");
    if let Err(diagnostic) = builtins::prepare_in(false) {
        drop(progress);
        return fail(&sources, vec![diagnostic]);
    }
    progress.stage("Compiling Flick modules");
    let override_cfg = target_override(
        target_overridden,
        true,
        wanted.clone(),
        options.no_native_overrides,
    );
    let (mut diagnostics, mut compilation) = package_cache::compile_for(
        &host,
        &mut sources,
        Some(&project),
        &roots,
        Some(&override_cfg),
        None,
    );
    build::hooks::ir(Some(&project), &mut compilation, &mut diagnostics);
    if global.warnings_as_errors {
        diagnostics = report::promote(&diagnostics);
    }
    let has_errors = diagnostics.iter().any(|d| d.severity == Severity::Error);
    if diagnostics.is_empty() {
        drop(progress);
    } else {
        drop(progress);
        report::render(global, &sources, &diagnostics);
        if has_errors {
            return report::exit_code(&diagnostics);
        }
    }
    let suites: Vec<String> = compilation
        .modules
        .iter()
        .filter(|m| m.test && m.interface.exports_value("tests"))
        .filter(|m| wanted.is_empty() || wanted.contains(&m.path))
        .filter(|m| compilation.module_available(&m.name))
        .map(|m| m.name.clone())
        .collect();
    let seed = options.seed.unwrap_or_else(default_seed);
    if suites.is_empty() {
        return emit_report(&[], &[], seed, options, global, 0);
    }
    let release = build::release(options.profile, Some(&project.config.build));
    let entries: Vec<MemoryName> = suites
        .iter()
        .map(|suite| MemoryName::new(suite, "tests"))
        .collect();
    let lowered = match lower::lower(&compilation, &entries, release) {
        Ok(lowered) => lowered,
        Err(err) => {
            return fail(
                &sources,
                vec![Diagnostic::new(
                    codes::P012,
                    Severity::Error,
                    format!("VM lowering failed: {err}"),
                )],
            );
        }
    };
    execute_suites(tier, &lowered, &entries, seed, options, global)
}

fn default_seed() -> u32 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    #[allow(clippy::cast_possible_truncation)]
    let seed = (nanos as u32) & 0x7fff_ffff;
    seed
}

fn execute_suites(
    tier: VmTier,
    lowered: &lower::LoweredProgram,
    entries: &[MemoryName],
    seed: u32,
    options: &TestArguments,
    global: &GlobalArguments,
) -> ExitCode {
    let VmTier::Tier0 = tier;
    let started = Instant::now();
    #[allow(clippy::cast_possible_wrap)]
    let seed_i32 = seed as i32;
    let mut raw_results: Vec<TestCaseResult> = Vec::new();
    for entry in entries {
        let mut heap = Heap::new();
        let results = match execute_tests_program(
            &mut heap,
            &lowered.program,
            entry,
            lowered.root.as_ref(),
            seed_i32,
            u64::MAX,
            &[],
        ) {
            Ok(results) => results,
            Err(err) => {
                let _ = heap.finish();
                eprintln!("vm test runner error in `{}`: {err}", entry.module);
                return ExitCode::from(1);
            }
        };
        if let Err(leak) = heap.finish() {
            eprintln!("vm memory leak after `{}`: {leak}", entry.module);
            return ExitCode::from(1);
        }
        raw_results.extend(results);
    }
    let filter = options.filter.as_deref().unwrap_or("");
    let filtered: Vec<TestCaseResult> = raw_results
        .into_iter()
        .filter(|r| r.name.contains(filter))
        .collect();
    if options.list {
        let mut stdout = io::stdout().lock();
        for item in filtered.iter().filter(|r| !r.skipped) {
            let _ = writeln!(stdout, "{}", item.name);
        }
        let _ = stdout.flush();
        return ExitCode::SUCCESS;
    }
    let mut ran = Vec::new();
    let mut skipped = Vec::new();
    for case in filtered {
        if case.skipped {
            skipped.push(ReportItem {
                name: case.name,
                status: "skipped",
                kind: None,
                message: None,
            });
        } else if case.passed {
            ran.push(ReportItem {
                name: case.name,
                status: "passed",
                kind: None,
                message: None,
            });
        } else {
            let (kind, message) = classify_failure(case.message.unwrap_or_default());
            ran.push(ReportItem {
                name: case.name,
                status: "failed",
                kind: Some(kind),
                message: Some(message),
            });
        }
    }
    let elapsed_ms = started.elapsed().as_millis();
    emit_report(&ran, &skipped, seed, options, global, elapsed_ms)
}

fn classify_failure(message: String) -> (&'static str, String) {
    if message.starts_with("die: ") {
        ("defect", message)
    } else if message == "interrupted" {
        ("interrupted", message)
    } else if message.starts_with("fail: ") {
        ("failure", message)
    } else {
        ("assertion", message)
    }
}

fn use_color(global: &GlobalArguments) -> bool {
    match global.color {
        Color::Always => true,
        Color::Never => false,
        Color::Auto => {
            io::stdout().is_terminal()
                && env::var_os("NO_COLOR").is_none()
                && env::var_os("TERM").is_none_or(|term| term != "dumb")
        }
    }
}

fn emit_report(
    ran: &[ReportItem],
    skipped: &[ReportItem],
    seed: u32,
    options: &TestArguments,
    global: &GlobalArguments,
    elapsed_ms: u128,
) -> ExitCode {
    let failed = ran.iter().filter(|r| r.status == "failed").count();
    let passed = ran.len() - failed;
    let mut stdout = io::stdout().lock();
    match options.reporter {
        Reporter::Json => {
            let mut out = String::new();
            let _ = write!(
                out,
                "{{\"seed\":{seed},\"passed\":{passed},\"failed\":{failed}"
            );
            if !skipped.is_empty() {
                let _ = write!(out, ",\"skipped\":{}", skipped.len());
            }
            out.push_str(",\"tests\":[");
            for (idx, item) in ran.iter().chain(skipped).enumerate() {
                if idx > 0 {
                    out.push(',');
                }
                let name_json = serde_json::to_string(&item.name).expect("string json");
                let status_json = serde_json::to_string(item.status).expect("status json");
                let _ = write!(out, "{{\"name\":{name_json},\"status\":{status_json}");
                if let Some(kind) = item.kind {
                    let kind_json = serde_json::to_string(kind).expect("kind json");
                    let _ = write!(out, ",\"kind\":{kind_json}");
                }
                if let Some(msg) = &item.message {
                    let msg_json = serde_json::to_string(msg).expect("message json");
                    let _ = write!(out, ",\"message\":{msg_json}");
                }
                out.push('}');
            }
            out.push_str("]}\n");
            let _ = stdout.write_all(out.as_bytes());
        }
        Reporter::Human => {
            let color = use_color(global);
            let reset = if color { "\x1b[0m" } else { "" };
            let dim = if color { "\x1b[2m" } else { "" };
            let title = if color {
                "\x1b[1;36mflick test\x1b[0m"
            } else {
                "flick test"
            };
            let _ = writeln!(stdout, "  {title}  {} tests\n", ran.len());
            for (idx, item) in ran.iter().enumerate() {
                let marker = if item.status == "passed" {
                    format!("{}✓{reset}", if color { "\x1b[32m" } else { "" })
                } else {
                    format!("{}✗{reset}", if color { "\x1b[31m" } else { "" })
                };
                let _ = writeln!(
                    stdout,
                    "  {marker} {dim}[{}/{}]{reset} {}  {dim}(0 ms){reset}",
                    idx + 1,
                    ran.len(),
                    item.name
                );
                if item.status == "failed" {
                    let kind = item.kind.unwrap_or("failure");
                    let msg = item
                        .message
                        .as_deref()
                        .unwrap_or("")
                        .replace('\n', "\n    ");
                    let _ = writeln!(stdout, "    {kind}: {msg}");
                }
            }
            if !skipped.is_empty() {
                let _ = writeln!(stdout);
                for item in skipped {
                    let _ = writeln!(stdout, "  {dim}- {}  (skipped){reset}", item.name);
                }
            }
            let pass_color = if color && passed > 0 { "\x1b[32m" } else { dim };
            let fail_color = if color && failed > 0 { "\x1b[31m" } else { dim };
            let also = if skipped.is_empty() {
                String::new()
            } else {
                format!(" · {dim}{}{reset} skipped", skipped.len())
            };
            let total = ran.len() + skipped.len();
            let _ = writeln!(
                stdout,
                "\n  Tests  {pass_color}{passed}{reset} passed · {fail_color}{failed}{reset} failed{also} · {total} total"
            );
            let _ = writeln!(stdout, "  Time   {dim}{elapsed_ms} ms{reset}");
            if failed > 0 {
                let _ = writeln!(stdout, "seed: {seed} (rerun with --seed {seed})");
            }
        }
    }
    let _ = stdout.flush();
    if failed > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
