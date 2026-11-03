//! `flick dev`: watches the project, builds each change into a new
//! generation and replaces the running app with it. A failed build leaves
//! the running version alone. A good one first stops the old version
//! (SIGTERM: the runner interrupts `main`, joins its fibers and runs its
//! finalizers) and starts the new one only after that shutdown ends; a
//! shutdown that fails stops the restarts, and its cause stays on stderr.
//!
//! A project with several entries, none named, runs each that is not a
//! client's, and Vite for its pages: the bundler builds the client's
//! modules itself and replaces them in the page as they change.

use std::process::ExitCode;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::source::SourceMap;
use signal_hook::consts::SIGTERM;

use super::run::{Generation, build_into_generation, dev_entries, find_project, launch, vite};
use super::test::{watch_snapshot, watched_roots};
use crate::args::{GlobalArguments, RunArguments};
use crate::child::{self, Running};
use crate::report;

/// A running generation: a process per entry it started.
struct Live {
    generation: Generation,
    running: Vec<Running>,
}

impl Live {
    /// Stops the app the way a signal does and removes its generation once
    /// every process has ended; the first exit code that is not a clean
    /// stop, else the first.
    fn stop(mut self, signal: usize) -> u8 {
        for running in &self.running {
            child::send(running.child.id(), signal);
        }
        let codes: Vec<u8> = self
            .running
            .iter_mut()
            .map(|running| {
                let code = running.child.wait().map_or(1, child::code);
                running.drain();
                code
            })
            .collect();
        drop(self.generation);
        let clean = |code: &&u8| **code == 0 || usize::from(**code) == 128 + signal;
        codes
            .iter()
            .find(|code| !clean(code))
            .or(codes.first())
            .copied()
            .unwrap_or(0)
    }
}

pub fn dev(options: &RunArguments, global: &GlobalArguments) -> ExitCode {
    let signals = child::signals();
    let project = match find_project(global) {
        Ok(p) => p,
        Err((sources, diagnostics)) => {
            report::render(global, &sources, &diagnostics);
            return report::exit_code(&diagnostics);
        }
    };
    let names = match dev_entries(&project, options) {
        Ok(names) => names,
        Err(message) => {
            let d = Diagnostic::new(codes::P007, Severity::Error, message);
            report::render(global, &SourceMap::default(), &[d]);
            return ExitCode::from(2);
        }
    };
    let several = options.name.is_none() && options.entry.is_none();
    let mut pages = if several && project.config.entries.iter().any(|e| e.client) {
        vite(&project.root, global)
    } else {
        None
    };
    let paths = watched_roots(&project);
    let mut live: Option<Live> = None;
    let mut seen = None;
    loop {
        let now = watch_snapshot(&paths);
        if seen.as_ref() != Some(&now) {
            seen = Some(now);
            if let Ok(generation) = build_into_generation(&project, options, global) {
                if let Some(old) = live.take() {
                    report::log(global, "dev: stopping the running version");
                    let code = old.stop(SIGTERM as usize);
                    // 143: interrupted and cleaned up; 0: it had ended
                    if code != 143 && code != 0 {
                        let d = Diagnostic::new(
                            codes::P021,
                            Severity::Error,
                            format!(
                                "the running version did not shut down cleanly (exit code {code}): restarts stop"
                            ),
                        );
                        report::render(global, &SourceMap::default(), &[d]);
                        return ExitCode::from(1);
                    }
                }
                report::log(global, "dev: starting the new version");
                let running: Vec<Running> = names
                    .iter()
                    .filter_map(|name| launch(&generation, name, options, global))
                    .collect();
                live = (!running.is_empty()).then_some(Live {
                    generation,
                    running,
                });
            } else if live.is_some() {
                report::log(global, "dev: the build failed; the running version stays");
            }
        }
        let signal = signals.load(Ordering::Relaxed);
        if signal != 0 {
            if let Some(mut pages) = pages.take() {
                child::send(pages.child.id(), signal);
                let _ = pages.child.wait();
                pages.drain();
            }
            return ExitCode::from(
                live.map_or(128 + u8::try_from(signal).unwrap_or(0), |l| l.stop(signal)),
            );
        }
        // one entry ending stops the version, which a change starts again
        let ended = live.as_mut().and_then(|l| {
            l.running
                .iter_mut()
                .find_map(|r| r.child.try_wait().ok().flatten())
        });
        if let Some(status) = ended {
            let code = child::code(status);
            report::log(
                global,
                &format!("dev: the app ended with exit code {code}; waiting for a change"),
            );
            if let Some(l) = live.take() {
                l.stop(SIGTERM as usize);
            }
        }
        thread::sleep(Duration::from_millis(200));
    }
}
