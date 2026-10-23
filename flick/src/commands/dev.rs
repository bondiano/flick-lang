//! `flick dev`: watches the project, builds each change into a new
//! generation and replaces the running app with it. A failed build leaves
//! the running version alone. A good one first stops the old version
//! (SIGTERM: the runner interrupts `main`, joins its fibers and runs its
//! finalizers) and starts the new one only after that shutdown ends; a
//! shutdown that fails stops the restarts, and its cause stays on stderr.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::{fs, thread};

use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::source::SourceMap;
use signal_hook::consts::SIGTERM;

use super::run::{build_generation, find_project, launch};
use super::test::{stamp, watched_roots};
use crate::args::{GlobalArguments, RunArguments};
use crate::child::{self, Running};
use crate::report;

/// A running generation.
struct Live {
    generation: PathBuf,
    running: Running,
}

impl Live {
    /// Stops the app the way a signal does and removes its generation once
    /// it has ended; the app's exit code.
    fn stop(mut self, signal: usize) -> u8 {
        child::send(self.running.child.id(), signal);
        let code = self.running.child.wait().map_or(1, child::code);
        self.running.drain();
        let _ = fs::remove_dir_all(&self.generation);
        code
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
    let paths = watched_roots(&project);
    let mut live: Option<Live> = None;
    let mut seen = None;
    loop {
        let now = stamp(&paths);
        if now != seen {
            seen = now;
            if let Ok(generation) = build_generation(&project, options, global) {
                if let Some(old) = live.take() {
                    report::log(global, "dev: stopping the running version");
                    let code = old.stop(SIGTERM as usize);
                    // 143: interrupted and cleaned up; 0: it had ended
                    if code != 143 && code != 0 {
                        let _ = fs::remove_dir_all(&generation);
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
                live = launch(&generation, &options.args, global).map(|running| Live {
                    generation,
                    running,
                });
            } else if live.is_some() {
                report::log(global, "dev: the build failed; the running version stays");
            }
        }
        let signal = signals.load(Ordering::Relaxed);
        if signal != 0 {
            return ExitCode::from(
                live.map_or(128 + u8::try_from(signal).unwrap_or(0), |l| l.stop(signal)),
            );
        }
        if let Some(l) = &mut live
            && let Ok(Some(status)) = l.running.child.try_wait()
        {
            let code = child::code(status);
            report::log(
                global,
                &format!("dev: the app ended with exit code {code}; waiting for a change"),
            );
            let ended = live.take().expect("live");
            let _ = fs::remove_dir_all(&ended.generation);
        }
        thread::sleep(Duration::from_millis(200));
    }
}
