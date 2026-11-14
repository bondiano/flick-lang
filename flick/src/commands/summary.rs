//! What `flick build` says after its diagnostics, as Elm's compiler does:
//! that it succeeded, how much it compiled and where each entry's runner
//! is, or that it failed and where the errors are.

use std::collections::BTreeSet;
use std::env;
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anstyle::{AnsiColor, Style};
use flick_core::diagnostics::Severity;

use crate::command::Outcome;

/// What a build wrote.
pub struct Built {
    #[cfg(feature = "wasm-backends")]
    pub backend: Option<flick_backend::BackendPin>,
    pub go: bool,
    pub rt: bool,
    /// How many modules it compiled; `None` when it reused a cached build.
    pub modules: Option<usize>,
    pub out_dir: PathBuf,
    /// Each entry's module and the runner that starts it.
    pub runners: Vec<(String, PathBuf)>,
}

const SUCCESS: Style = Style::new()
    .bold()
    .fg_color(Some(anstyle::Color::Ansi(AnsiColor::Green)));
const FAILURE: Style = Style::new()
    .bold()
    .fg_color(Some(anstyle::Color::Ansi(AnsiColor::Red)));
const DIM: Style = Style::new().dimmed();
const NAME: Style = Style::new().fg_color(Some(anstyle::Color::Ansi(AnsiColor::Cyan)));

/// The summary of a build that wrote `built`, or of one that failed.
pub fn text(built: Option<&Built>, outcome: &Outcome, took: Duration, color: bool) -> String {
    let paint = |style: Style, text: &str| {
        if color {
            format!("{style}{text}{style:#}")
        } else {
            text.to_owned()
        }
    };
    let count = |severity| {
        outcome
            .diagnostics
            .iter()
            .filter(|d| d.severity == severity)
            .count()
    };
    let (errors, warnings) = (count(Severity::Error), count(Severity::Warning));
    let built = built.filter(|_| errors == 0);
    let Some(built) = built else {
        let files: BTreeSet<String> = outcome
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .filter_map(|d| d.span)
            .map(|s| shown(Path::new(&outcome.sources.get(s.file).path)))
            .collect();
        let place = if files.is_empty() {
            String::new()
        } else {
            format!(" in {}", files.into_iter().collect::<Vec<_>>().join(", "))
        };
        return format!(
            "\n{} {}{place}.\n",
            paint(FAILURE, "Build failed:"),
            problems(errors, warnings)
        );
    };
    let what = match built.modules {
        Some(n) => format!("Compiled {}", plural(n, "module")),
        None => "Up to date, reused the last build".to_owned(),
    };
    let warned = if warnings > 0 {
        format!(", with {}", plural(warnings, "warning"))
    } else {
        String::new()
    };
    let mut out = format!(
        "\n{} {what} in {}{warned}.\n",
        paint(SUCCESS, "Success!"),
        seconds(took)
    );
    if built.runners.is_empty() {
        let _ = writeln!(out, "\n    {}", paint(NAME, &shown(&built.out_dir)));
    } else {
        out.push('\n');
        let width = built
            .runners
            .iter()
            .map(|(m, _)| m.len())
            .max()
            .unwrap_or(0);
        for (module, runner) in &built.runners {
            let _ = writeln!(
                out,
                "    {}{} {} {}",
                paint(NAME, module),
                " ".repeat(width - module.len()),
                paint(DIM, "───>"),
                shown(runner)
            );
        }
    }
    out
}

fn problems(errors: usize, warnings: usize) -> String {
    match warnings {
        0 => plural(errors, "error"),
        _ => format!(
            "{} and {}",
            plural(errors, "error"),
            plural(warnings, "warning")
        ),
    }
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}

fn seconds(took: Duration) -> String {
    format!("{:.2}s", took.as_secs_f64())
}

/// `path` relative to the working directory when it is inside it.
fn shown(path: &Path) -> String {
    env::current_dir()
        .ok()
        .and_then(|cwd| path.strip_prefix(cwd).ok().map(Path::to_path_buf))
        .unwrap_or_else(|| path.to_path_buf())
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use flick_core::diagnostics::{Diagnostic, codes};

    fn outcome(diagnostics: Vec<Diagnostic>) -> Outcome {
        Outcome {
            diagnostics,
            ..Outcome::default()
        }
    }

    #[test]
    fn a_build_says_what_it_compiled_and_where_each_runner_is() {
        let built = Built {
            #[cfg(feature = "wasm-backends")]
            backend: None,
            go: false,
            rt: false,
            modules: Some(3),
            out_dir: PathBuf::from("/x/dist"),
            runners: vec![
                (
                    "App.Main".to_owned(),
                    PathBuf::from("/x/dist/flick-main.js"),
                ),
                ("App.Web".to_owned(), PathBuf::from("/x/dist/flick-web.js")),
            ],
        };
        let text = text(
            Some(&built),
            &outcome(Vec::new()),
            Duration::from_millis(1250),
            false,
        );
        assert!(text.contains("Compiled 3 modules in 1.25s"), "{text}");
        for (module, runner) in &built.runners {
            let line = text
                .lines()
                .find(|line| line.contains(module))
                .expect("entry");
            assert!(line.contains(&runner.display().to_string()), "{line}");
        }
    }

    #[test]
    fn a_failed_build_counts_its_problems() {
        let diagnostics = vec![
            Diagnostic::new(codes::P004, Severity::Error, "no"),
            Diagnostic::new(codes::P004, Severity::Error, "no"),
            Diagnostic::new(codes::P004, Severity::Warning, "hm"),
        ];
        let built = Built {
            #[cfg(feature = "wasm-backends")]
            backend: None,
            go: false,
            rt: false,
            modules: Some(1),
            out_dir: PathBuf::new(),
            runners: Vec::new(),
        };
        let text = text(Some(&built), &outcome(diagnostics), Duration::ZERO, false);
        assert!(text.contains("Build failed:"), "{text}");
        assert!(text.contains("2 errors and 1 warning"), "{text}");
        assert!(
            !text.contains("Success!") && !text.contains("Compiled"),
            "{text}"
        );
    }
}
