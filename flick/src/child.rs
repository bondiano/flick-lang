//! Child processes: Node running a program, pnpm. Their stdout passes
//! through; their stderr passes through in human mode and becomes events of
//! one kind in JSON mode. SIGINT and SIGTERM sent to `flick` reach the
//! child, and `flick` ends only after the child has, with its exit code.

use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use std::{io, thread};

use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::flag;

use crate::args::{DiagnosticsFormat, GlobalArguments};
use crate::{report, timings};

/// The last signal `flick` received, `0` for none.
pub fn signals() -> Arc<AtomicUsize> {
    let last = Arc::new(AtomicUsize::new(0));
    for signal in [SIGINT, SIGTERM] {
        // registration fails only for forbidden signals, which these are not
        let value = usize::try_from(signal).expect("signal numbers are positive");
        let _ = flag::register_usize(signal, Arc::clone(&last), value);
    }
    last
}

/// Sends `signal` to the process `pid`.
pub fn send(pid: u32, signal: usize) {
    let name = if signal == SIGINT as usize {
        "INT"
    } else {
        "TERM"
    };
    let _ = Command::new("kill")
        .args(["-s", name, &pid.to_string()])
        .stderr(Stdio::null())
        .status();
}

/// Starts `command` with its stderr reported as `kind` events in JSON mode.
pub fn spawn(
    command: &mut Command,
    global: &GlobalArguments,
    kind: &'static str,
) -> io::Result<Running> {
    start(command, global, kind, false)
}

/// Starts a tool whose stdout is service output too: both its streams go
/// to stderr, as `kind` events in JSON mode.
pub fn spawn_tool(
    command: &mut Command,
    global: &GlobalArguments,
    kind: &'static str,
) -> io::Result<Running> {
    start(command, global, kind, true)
}

fn start(
    command: &mut Command,
    global: &GlobalArguments,
    kind: &'static str,
    stdout_too: bool,
) -> io::Result<Running> {
    // what the process takes is its own phase, not the end of the build's
    timings::stage("Running a child process");
    let json = matches!(global.diagnostics, DiagnosticsFormat::Json);
    if json {
        command.stderr(Stdio::piped());
    }
    if stdout_too {
        command.stdout(Stdio::piped());
    }
    let mut child = command.spawn()?;
    let mut relays = Vec::new();
    if let Some(stderr) = child.stderr.take() {
        let global = global.clone();
        relays.push(thread::spawn(move || {
            relay(stderr, &global, kind, "stderr");
        }));
    }
    if let Some(stdout) = child.stdout.take() {
        let global = global.clone();
        relays.push(thread::spawn(move || {
            relay(stdout, &global, kind, "stdout");
        }));
    }
    Ok(Running { child, relays })
}

pub struct Running {
    pub child: Child,
    relays: Vec<thread::JoinHandle<()>>,
}

impl Running {
    /// Waits until the relay has written all of the child's stderr.
    pub fn drain(&mut self) {
        for relay in self.relays.drain(..) {
            let _ = relay.join();
        }
    }

    /// Waits for the child, forwarding the signals `flick` receives.
    pub fn wait(mut self, last: &AtomicUsize) -> io::Result<ExitStatus> {
        let mut forwarded = 0;
        let status = loop {
            if let Some(status) = self.child.try_wait()? {
                break status;
            }
            let signal = last.load(Ordering::Relaxed);
            if signal != 0 && signal != forwarded {
                send(self.child.id(), signal);
                forwarded = signal;
            }
            thread::sleep(Duration::from_millis(20));
        };
        self.drain();
        Ok(status)
    }
}

fn relay(stream: impl Read, global: &GlobalArguments, kind: &str, channel: &str) {
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { break };
        report::event(global, kind, channel, &line);
    }
}

/// The exit code of a child: its own, or `128 + n` when signal `n` ended it.
pub fn code(status: ExitStatus) -> u8 {
    if let Some(code) = status.code() {
        return u8::try_from(code).unwrap_or(1);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return u8::try_from(128 + signal).unwrap_or(1);
        }
    }
    1
}
