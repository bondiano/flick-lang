//! Pre-scanning of raw CLI `argv` (stopping at `--`) before dynamic clap
//! parsing, including detection of removed legacy flags (`FLICK-P057`).

use std::ffi::OsString;

use flick_backend::Verb;
use flick_core::diagnostics::{Diagnostic, Severity, codes};

use crate::args::{Color, DiagnosticsFormat, GlobalArguments};

pub(super) struct Scan {
    pub(super) verb: Option<Verb>,
    pub(super) help: bool,
    pub(super) help_subcommand: bool,
    pub(super) offline: bool,
    pub(super) target: Option<String>,
    pub(super) emit: Option<String>,
    pub(super) project: Option<String>,
    pub(super) package: Option<String>,
    pub(super) diagnostics: DiagnosticsFormat,
    pub(super) color: Color,
    pub(super) removed: Option<Diagnostic>,
}

impl Default for Scan {
    fn default() -> Self {
        Self {
            verb: None,
            help: false,
            help_subcommand: false,
            offline: false,
            target: None,
            emit: None,
            project: None,
            package: None,
            diagnostics: DiagnosticsFormat::Human,
            color: Color::Auto,
            removed: None,
        }
    }
}

impl Scan {
    pub(super) fn minimal_global(&self) -> GlobalArguments {
        GlobalArguments {
            diagnostics: self.diagnostics,
            color: self.color,
            quiet: false,
            verbose: false,
            timings: false,
            project: self.project.clone(),
            package: self.package.clone(),
            warnings_as_errors: false,
            offline: self.offline,
            allow_tool: Vec::new(),
        }
    }
}

fn parse_verb(word: &str) -> Option<Verb> {
    match word {
        "build" => Some(Verb::Build),
        "run" => Some(Verb::Run),
        "dev" => Some(Verb::Dev),
        "test" => Some(Verb::Test),
        "inspect" => Some(Verb::Inspect),
        _ => None,
    }
}

fn scan_global_value(
    arg: &str,
    next: Option<&str>,
) -> Option<(&'static str, Option<String>, bool)> {
    const FLAGS: [(&str, &str); 6] = [
        ("--diagnostics", "diagnostics"),
        ("--color", "color"),
        ("--project", "project"),
        ("--package", "package"),
        ("-p", "package"),
        ("--allow-tool", "allow-tool"),
    ];
    for (prefix, key) in FLAGS {
        if arg == prefix {
            return Some((key, next.map(str::to_owned), next.is_some()));
        }
        if let Some(rest) = arg.strip_prefix(prefix).and_then(|r| r.strip_prefix('=')) {
            return Some((key, Some(rest.to_owned()), false));
        }
    }
    None
}

fn apply_global(scan: &mut Scan, key: &str, value: Option<String>) {
    let Some(value) = value else { return };
    match key {
        "diagnostics" => {
            if value == "json" {
                scan.diagnostics = DiagnosticsFormat::Json;
            } else if value == "human" {
                scan.diagnostics = DiagnosticsFormat::Human;
            }
        }
        "color" => {
            if value == "always" {
                scan.color = Color::Always;
            } else if value == "never" {
                scan.color = Color::Never;
            } else if value == "auto" {
                scan.color = Color::Auto;
            }
        }
        "project" => scan.project = Some(value),
        "package" => scan.package = Some(value),
        _ => {}
    }
}

fn is_global_bool(arg: &str) -> bool {
    matches!(
        arg,
        "--quiet" | "--silent" | "--verbose" | "--timings" | "--warnings-as-errors" | "--offline"
    )
}

fn removed_flag_diagnostic(verb: Verb, flag: &str, value: Option<&str>) -> Option<Diagnostic> {
    let make = |msg: &str, fix: &str| {
        Diagnostic::new(codes::P057, Severity::Error, msg).with_fix(|| (fix.to_owned(), Vec::new()))
    };
    match (verb, flag) {
        (Verb::Build, "--emit") => {
            let fix = match value {
                Some("js") => "use `--target js` (or omit it, as `js` is the default target)",
                Some("ts") => {
                    "use `--target js --typescript` (or set `[target.js] typescript = true` in `flick.toml`)"
                }
                Some("go") => {
                    "use `--target go` (or set `[build] target = \"go\"` in `flick.toml`)"
                }
                _ => "use `--target <name>` (and `--typescript` for TypeScript output)",
            };
            Some(make("the `--emit` flag has been removed", fix))
        }
        (Verb::Build, "--emit-dts") => Some(make(
            "the `--emit-dts` flag has been removed",
            "use `--declarations` (or set `[target.js] declarations = true` in `flick.toml`)",
        )),
        (Verb::Build | Verb::Test | Verb::Run | Verb::Dev, "--rt") => Some(make(
            "the `--rt` flag has been removed",
            "use `--runtime rt` (now the default for `flick/js`; use `--runtime effect` for Effect)",
        )),
        (Verb::Test, "--emit") => {
            let fix = match value {
                Some("js") => "use `--target js` (or omit it, as `js` is the default target)",
                Some("go") => {
                    "use `--target go` (or set `[build] target = \"go\"` in `flick.toml`)"
                }
                _ => "use `--target <name>`",
            };
            Some(make("the `--emit` flag has been removed", fix))
        }
        (Verb::Inspect, "--emit") => match value {
            Some("js") => Some(make(
                "the `--emit js` representation has been removed",
                "use `--emit output --target js`",
            )),
            Some("ts") => Some(make(
                "the `--emit ts` representation has been removed",
                "use `--emit output --target js --typescript`",
            )),
            _ => None,
        },
        _ => None,
    }
}

fn scan_verb_flag(scan: &mut Scan, verb: Verb, arg: &str, next: Option<&str>) -> Option<usize> {
    if arg == "--target" {
        if let Some(val) = next {
            scan.target = Some(val.to_owned());
            return Some(2);
        }
        return Some(1);
    }
    if let Some(val) = arg.strip_prefix("--target=") {
        scan.target = Some(val.to_owned());
        return Some(1);
    }
    if arg == "--emit" {
        let val = next.filter(|n| !n.starts_with('-'));
        if verb == Verb::Inspect {
            scan.emit = val.map(str::to_owned);
        }
        if scan.removed.is_none()
            && let Some(diag) = removed_flag_diagnostic(verb, "--emit", val)
        {
            scan.removed = Some(diag);
        }
        return Some(1 + usize::from(val.is_some()));
    }
    if let Some(val) = arg.strip_prefix("--emit=") {
        if verb == Verb::Inspect {
            scan.emit = Some(val.to_owned());
        }
        if scan.removed.is_none()
            && let Some(diag) = removed_flag_diagnostic(verb, "--emit", Some(val))
        {
            scan.removed = Some(diag);
        }
        return Some(1);
    }
    if scan.removed.is_none() {
        if arg == "--emit-dts" || arg.starts_with("--emit-dts=") {
            if let Some(diag) = removed_flag_diagnostic(verb, "--emit-dts", None) {
                scan.removed = Some(diag);
            }
            return Some(1);
        }
        if arg == "--rt" || arg.starts_with("--rt=") {
            if let Some(diag) = removed_flag_diagnostic(verb, "--rt", None) {
                scan.removed = Some(diag);
            }
            return Some(1);
        }
    }
    None
}

/// Pre-scan `argv` (without argv[0]), stopping at `--`.
pub(super) fn scan_argv(argv: &[OsString]) -> Scan {
    let words: Vec<String> = argv
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let mut scan = Scan::default();
    let mut i = 0;
    let mut seen_non_verb_cmd = false;

    while i < words.len() {
        let arg = words[i].as_str();
        if arg == "--" {
            break;
        }
        let next = words.get(i + 1).map(String::as_str);
        if let Some((key, val, consumed)) = scan_global_value(arg, next) {
            apply_global(&mut scan, key, val);
            i += 1 + usize::from(consumed);
            continue;
        }
        if is_global_bool(arg) {
            if arg == "--offline" {
                scan.offline = true;
            }
            i += 1;
            continue;
        }
        if arg == "-h" || arg == "--help" {
            scan.help = true;
            i += 1;
            continue;
        }
        if scan.verb.is_none() && !seen_non_verb_cmd {
            if arg == "help" {
                scan.help = true;
                scan.help_subcommand = true;
                i += 1;
                continue;
            }
            if !arg.starts_with('-') {
                if let Some(v) = parse_verb(arg) {
                    scan.verb = Some(v);
                } else {
                    seen_non_verb_cmd = true;
                }
            }
            i += 1;
            continue;
        }
        if let Some(verb) = scan.verb
            && let Some(step) = scan_verb_flag(&mut scan, verb, arg, next)
        {
            i += step;
            continue;
        }
        i += 1;
    }
    scan
}
