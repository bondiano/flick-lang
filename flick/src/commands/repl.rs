//! `flick repl`: interactive REPL session driven by a backend-neutral [`ReplEngine`].

use std::process::ExitCode;

use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::source::SourceMap;

use crate::args::{GlobalArguments, ReplArguments};
#[cfg(feature = "vm")]
use crate::repl::{self, vm_engine::VmReplEngine};
use crate::report;
use crate::vm;

/// Runs `flick repl`.
pub fn repl(options: &ReplArguments, global: &GlobalArguments) -> ExitCode {
    if let Some(target) = options.target.as_deref()
        && target != "vm"
    {
        let diag = Diagnostic::new(
            codes::P012,
            Severity::Error,
            format!(
                "`flick repl --target {target}` is not supported yet; use `--target vm` (default)"
            ),
        );
        report::render(global, &SourceMap::default(), &[diag]);
        return ExitCode::from(2);
    }

    #[cfg(not(feature = "vm"))]
    {
        let _ = options;
        let diag = vm::disabled_diagnostic();
        report::render(global, &SourceMap::default(), &[diag]);
        ExitCode::from(2)
    }

    #[cfg(feature = "vm")]
    {
        run_vm_repl(options, global)
    }
}

#[cfg(feature = "vm")]
fn run_vm_repl(options: &ReplArguments, global: &GlobalArguments) -> ExitCode {
    use std::io::{self, IsTerminal as _};
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    use signal_hook::consts::SIGINT;
    use signal_hook::flag;

    match vm::resolve_tier(options.vm_tier.as_deref(), None) {
        Ok(vm::VmTier::Tier0) => {}
        Ok(vm::VmTier::Tier1) => {
            let diag = Diagnostic::new(
                codes::P012,
                Severity::Error,
                "`flick repl --vm-tier 1` is not supported yet; use `--vm-tier 0` (default)",
            );
            report::render(global, &SourceMap::default(), &[diag]);
            return ExitCode::from(2);
        }
        Err(diagnostics) => {
            report::render(global, &SourceMap::default(), &diagnostics);
            return report::exit_code(&diagnostics);
        }
    }

    let interrupt = Arc::new(AtomicBool::new(false));
    let _ = flag::register(SIGINT, Arc::clone(&interrupt));

    let mut engine = VmReplEngine::new();
    let stdin = io::stdin();
    let interactive = stdin.is_terminal();
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();

    match repl::run_loop(
        &mut engine,
        stdin.lock(),
        &mut stdout,
        &mut stderr,
        interactive,
        &interrupt,
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("repl I/O error: {err}");
            ExitCode::from(1)
        }
    }
}
