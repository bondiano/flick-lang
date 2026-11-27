//! CLI help document formatting matching Effect CLI's plain formatter.

use std::fmt::Write as _;

use flick_core::opt::json_string;

use super::{CliMultiplicity, CliParam, CliSpec};

pub(super) fn param_flag_name(param: &CliParam) -> String {
    param.label.replace('_', "-")
}

pub(super) fn printable_flag(name: &str) -> String {
    let prefix = if name.encode_utf16().count() == 1 {
        "-"
    } else {
        "--"
    };
    format!("{prefix}{name}")
}

pub(super) fn quoted_choices(values: &[&str]) -> String {
    values
        .iter()
        .map(|value| json_string(value))
        .collect::<Vec<_>>()
        .join(" | ")
}

pub(super) fn render_help(spec: &CliSpec, path: &str) -> String {
    let mut out = String::new();
    if !spec.description.is_empty() {
        writeln!(out, "DESCRIPTION\n  {}\n", spec.description).expect("write to String");
    }

    write!(out, "USAGE\n  {path}").expect("write to String");
    if !spec.subcommands.is_empty() {
        out.push_str(" <subcommand>");
    }
    out.push_str(" [flags]");
    let mut operands = Vec::new();
    let mut flags = Vec::new();
    for param in &spec.params {
        let name = param_flag_name(param);
        if param.flag {
            let mut names = format!("--{name}");
            for alias in &param.aliases {
                write!(names, ", {}", printable_flag(alias)).expect("write to String");
            }
            names.push_str(" string");
            flags.push((names, param.help.clone()));
        } else {
            let display = if matches!(param.multi, CliMultiplicity::Many) {
                format!("{name}...")
            } else {
                name
            };
            let mut description = param.help.clone();
            if matches!(param.multi, CliMultiplicity::Required) {
                write!(out, " <{display}>").expect("write to String");
            } else {
                write!(out, " [<{display}>]").expect("write to String");
                description.push_str(" (optional)");
            }
            operands.push((format!("{display} string"), description));
        }
    }

    table(&mut out, "ARGUMENTS", operands, 25);
    table(&mut out, "FLAGS", flags, usize::MAX);
    let globals = [
        ("--help, -h", "Show help information"),
        ("--version, -v", "Show version information"),
        ("--wizard", "Start wizard mode for a command"),
        (
            "--completions <bash|zsh|fish|powershell|sh|pwsh>",
            "Print shell completion script (choices: bash, zsh, fish, powershell, sh, pwsh)",
        ),
        (
            "--log-level <all|trace|debug|info|warn|warning|error|fatal|none>",
            "Sets the minimum log level (choices: all, trace, debug, info, warn, warning, error, fatal, none)",
        ),
        (
            "--trace",
            "Send spans, logs and metrics to OTEL_EXPORTER_OTLP_ENDPOINT",
        ),
    ];
    table(
        &mut out,
        "GLOBAL FLAGS",
        globals.map(|(a, b)| (a.to_owned(), b.to_owned())).into(),
        usize::MAX,
    );
    let commands = spec
        .subcommands
        .iter()
        .map(|(sub, help)| {
            let summary = if help.is_empty() {
                &sub.description
            } else {
                help
            };
            (sub.name.clone(), summary.clone())
        })
        .collect();
    table(&mut out, "SUBCOMMANDS", commands, 20);
    out.push('\n');
    out
}

fn table(out: &mut String, title: &str, rows: Vec<(String, String)>, width_cap: usize) {
    let Some(longest) = rows.iter().map(|(label, _)| label.chars().count()).max() else {
        return;
    };
    let column = (longest + 4).min(width_cap);
    write!(out, "\n\n{title}").expect("write to String");
    for (label, description) in rows {
        let width = column.max(label.chars().count() + 1);
        write!(out, "\n  {label:width$}{description}").expect("write to String");
    }
}
