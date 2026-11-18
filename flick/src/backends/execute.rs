//! Runs the toolchain steps of a validated generation in their order. Each
//! step runs in a fresh directory beside the output directory, holding only
//! its declared inputs; its declared outputs become artifacts. Nothing is
//! published here, so a failed or cancelled step leaves the output as it was.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};
use std::{fs, io, thread};

use flick_backend::host::runtime::Cancellation;
use flick_backend::{BuildResult, InputRef, Limits, ToolchainStep, validate_output_paths};

use super::tools::Tool;
use crate::artifact::{Artifact, validate_ancestors};
use crate::publish::Beside;

/// The bytes of a tool's output a failure keeps.
const TAIL: usize = 64 << 10;

pub(super) enum Failure {
    Cancelled,
    Step(String),
}

/// The outputs of every step of `result`, run in `order`. `sources` holds
/// the request's side inputs by id; `out` is the output directory.
pub(super) fn run(
    result: &BuildResult,
    order: &[usize],
    sources: &BTreeMap<String, Vec<u8>>,
    tools: &[Tool],
    out: &Path,
    cancel: &Cancellation,
) -> Result<Vec<Artifact>, Failure> {
    let mut produced: BTreeMap<&str, Artifact> = BTreeMap::new();
    let mut outputs = Vec::new();
    for &i in order {
        let step = &result.steps[i];
        let work = Beside::new(out).map_err(|e| failed(step, &e.to_string()))?;
        for input in &step.inputs {
            let (path, bytes) =
                input_file(input, result, &produced, sources).map_err(|why| failed(step, &why))?;
            write(&work.path().join(path), bytes).map_err(|e| failed(step, &e.to_string()))?;
        }
        execute(step, tools, work.path(), cancel)?;
        for output in &step.outputs {
            let file = read_output(work.path(), &output.path).map_err(|why| failed(step, &why))?;
            let artifact = Artifact {
                path: output.path.clone(),
                contents: file,
                kind: output.kind,
            };
            outputs.push(artifact.clone());
            produced.insert(&output.id, artifact);
        }
    }
    Ok(outputs)
}

fn failed(step: &ToolchainStep, why: &str) -> Failure {
    Failure::Step(format!(
        "step `{}` of `{}`: {why}",
        step.id, step.toolchain.id
    ))
}

/// The path and bytes an input has in the directory of a step: an artifact
/// at its own path, a side input at `.flick-inputs/<id>`.
fn input_file<'a>(
    input: &'a InputRef,
    result: &'a BuildResult,
    produced: &'a BTreeMap<&str, Artifact>,
    sources: &'a BTreeMap<String, Vec<u8>>,
) -> Result<(String, &'a [u8]), String> {
    match input {
        InputRef::Artifact(id) => produced
            .get(id.as_str())
            .map(|a| (a.path.clone(), a.contents.as_slice()))
            .or_else(|| {
                (result.artifacts.iter())
                    .find(|a| &a.id == id)
                    .map(|a| (a.path.clone(), a.bytes.as_slice()))
            })
            .ok_or_else(|| format!("no artifact `{id}`")),
        InputRef::Source(id) => {
            let path = format!(".flick-inputs/{id}");
            validate_output_paths([path.as_str()], Limits::default())
                .map_err(|why| format!("side input `{id}` has no portable file name: {why}"))?;
            let bytes = sources
                .get(id)
                .ok_or_else(|| format!("no side input `{id}`"))?;
            Ok((path, bytes))
        }
    }
}

fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, bytes)
}

/// A declared output must be a regular file inside the directory; a tool
/// that leaves a symlink or a directory there fails.
fn read_output(dir: &Path, path: &str) -> Result<Vec<u8>, String> {
    validate_ancestors(dir, Path::new(path))?;
    let full = dir.join(path);
    let metadata = fs::symlink_metadata(&full)
        .map_err(|_| format!("the tool wrote no declared output `{path}`"))?;
    if !metadata.is_file() {
        return Err(format!(
            "the declared output `{path}` is not a regular file"
        ));
    }
    let limit = Limits::default().artifact_bytes;
    if usize::try_from(metadata.len()).is_ok_and(|n| n > limit) {
        return Err(format!(
            "the declared output `{path}` exceeds its size limit"
        ));
    }
    fs::read(full).map_err(|e| e.to_string())
}

fn execute(
    step: &ToolchainStep,
    tools: &[Tool],
    dir: &Path,
    cancel: &Cancellation,
) -> Result<(), Failure> {
    // the binary is checked again: it may have changed since the build began
    let tool = (tools.iter())
        .find(|t| t.pin == step.toolchain)
        .ok_or_else(|| "the host resolved no such tool".to_owned())
        .and_then(Tool::recheck)
        .map_err(|why| failed(step, &why))?;
    let mut command = tool.step(dir);
    command
        .args(&step.arguments)
        .envs(step.environment.iter().map(|v| (&v.key, &v.value)))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // its own group, so cancellation reaches what the tool starts
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(|e| failed(step, &e.to_string()))?;
    let tails = [
        tail(
            child
                .stdout
                .take()
                .map(|s| Box::new(s) as Box<dyn Read + Send>),
        ),
        tail(
            child
                .stderr
                .take()
                .map(|s| Box::new(s) as Box<dyn Read + Send>),
        ),
    ];
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if cancel.is_cancelled() => {
                stop(&mut child);
                return Err(Failure::Cancelled);
            }
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(e) => {
                stop(&mut child);
                return Err(failed(step, &e.to_string()));
            }
        }
    };
    // what the tool started ends with it, and with it the pipes it holds
    end_group(&child);
    if status.success() {
        return Ok(());
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while tails.iter().any(|t| !t.is_finished()) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    let output: Vec<String> = (tails.into_iter())
        .filter(thread::JoinHandle::is_finished)
        .map(|t| t.join().unwrap_or_default())
        .map(|bytes| String::from_utf8_lossy(&bytes).trim_end().to_owned())
        .filter(|text| !text.is_empty())
        .collect();
    Err(failed(
        step,
        &format!("exited with {status}\n{}", output.join("\n")),
    ))
}

/// The last bytes of a stream, read on a thread of their own.
fn tail(stream: Option<Box<dyn Read + Send>>) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let Some(mut stream) = stream else {
            return Vec::new();
        };
        let mut kept = Vec::new();
        let mut buffer = [0; 8192];
        while let Ok(n @ 1..) = stream.read(&mut buffer) {
            kept.extend_from_slice(&buffer[..n]);
            if kept.len() > 2 * TAIL {
                kept.drain(..kept.len() - TAIL);
            }
        }
        let start = kept.len().saturating_sub(TAIL);
        kept.split_off(start)
    })
}

/// Ends what is left of the process group of a tool that has exited.
fn end_group(child: &Child) {
    #[cfg(unix)]
    {
        use rustix::process::{Pid, Signal, kill_process_group};
        let _ = kill_process_group(Pid::from_child(child), Signal::KILL);
    }
    // shortcut: no process groups off Unix, so a descendant of a tool may outlive it; end it through a job object if a Windows host needs that
    #[cfg(not(unix))]
    let _ = child;
}

/// Asks the tool's process group to end, and ends it after two seconds.
/// Off Unix only the tool itself ends (see [`end_group`]).
fn stop(child: &mut Child) {
    #[cfg(unix)]
    {
        use rustix::process::{Pid, Signal, kill_process_group};
        let group = Pid::from_child(child);
        let _ = kill_process_group(group, Signal::TERM);
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if child.try_wait().is_ok_and(|s| s.is_some()) {
                // what the tool started may outlive it
                let _ = kill_process_group(group, Signal::KILL);
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
        let _ = kill_process_group(group, Signal::KILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(all(test, unix))]
#[path = "execute_tests.rs"]
mod tests;
