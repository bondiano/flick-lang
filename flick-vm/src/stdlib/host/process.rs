//! Native Rust `Host.Process` and `ChildProcess` execution, pipe streaming,
//! and scoped cleanup.

use std::io::{self, Read, Write as _};
use std::process::{Child, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use flick_core::ir::Lit;
use flick_core::ir::memory::HostServiceIntrinsic;

use crate::stdlib::hooks::VmHooks;
use crate::stdlib::primitives::duration_sleep_nanos;
use crate::tier0::{Error, Heap, Value};

use super::HostMode;
use super::error_map::{
    handle_id, int_list_from_bytes, ok, opt_none, opt_some, option_some, platform_sys_err_tag,
    record_field, system_error_tag, vec_to_string_list,
};
use super::process_cmd::{
    SingleCmd, cmd_inherit_output, cmd_make, cmd_pipe_to, cmd_prefix, cmd_set_cwd, cmd_set_env,
    cmd_set_stdin, collect_commands, format_cmd_text, send_signal, split_stream_lines,
};

const PROC_HANDLE_TAG: &str = "$vm_process";

#[derive(Debug)]
pub(crate) struct ProcessEntry {
    cmds: Vec<Child>,
    last_pid: i32,
    cmd_text: String,
    stdout_rx: Option<Receiver<Vec<u8>>>,
    stderr_rx: Option<Receiver<Vec<u8>>>,
    stdout_thread: Option<JoinHandle<()>>,
    stderr_thread: Option<JoinHandle<()>>,
    inherit_stdout: Option<JoinHandle<Vec<u8>>>,
    inherit_stderr: Option<JoinHandle<Vec<u8>>>,
    status: Option<ExitStatus>,
}

impl ProcessEntry {
    pub(crate) fn force_terminate(&mut self) {
        self.stdout_rx = None;
        self.stderr_rx = None;
        for child in self.cmds.iter_mut().rev() {
            let _ = send_signal(child, "SIGKILL");
            let _ = child.wait();
        }
        if let Some(t) = self.stdout_thread.take() {
            let _ = t.join();
        }
        if let Some(t) = self.stderr_thread.take() {
            let _ = t.join();
        }
        if let Some(t) = self.inherit_stdout.take() {
            let _ = t.join();
        }
        if let Some(t) = self.inherit_stderr.take() {
            let _ = t.join();
        }
    }

    fn is_running(&mut self) -> bool {
        if self.status.is_some() {
            return false;
        }
        if let Some(last) = self.cmds.last_mut() {
            match last.try_wait() {
                Ok(Some(st)) => {
                    self.status = Some(st);
                    false
                }
                Ok(None) => true,
                Err(_) => false,
            }
        } else {
            false
        }
    }

    fn wait_all(&mut self) -> io::Result<(Option<ExitStatus>, Vec<u8>, Vec<u8>)> {
        self.stdout_rx = None;
        self.stderr_rx = None;
        let status = if let Some(st) = self.status {
            Some(st)
        } else {
            let count = self.cmds.len();
            let mut last_status = None;
            for (idx, child) in self.cmds.iter_mut().enumerate() {
                let st = child.wait()?;
                if idx + 1 == count {
                    last_status = Some(st);
                }
            }
            if let Some(t) = self.stdout_thread.take() {
                let _ = t.join();
            }
            if let Some(t) = self.stderr_thread.take() {
                let _ = t.join();
            }
            self.status = last_status;
            last_status
        };
        let out_bytes = self
            .inherit_stdout
            .take()
            .and_then(|t| t.join().ok())
            .unwrap_or_default();
        let err_bytes = self
            .inherit_stderr
            .take()
            .and_then(|t| t.join().ok())
            .unwrap_or_default();
        Ok((status, out_bytes, err_bytes))
    }

    fn terminate(&mut self, signal: &str, force_after: Option<Duration>) -> (Vec<u8>, Vec<u8>) {
        for child in self.cmds.iter_mut().rev() {
            let _ = send_signal(child, signal);
        }
        if let Some(limit) = force_after {
            let deadline = Instant::now() + limit;
            while self.is_running() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
            if self.is_running() {
                for child in self.cmds.iter_mut().rev() {
                    let _ = send_signal(child, "SIGKILL");
                }
            }
        }
        match self.wait_all() {
            Ok((_, out_bytes, err_bytes)) => (out_bytes, err_bytes),
            Err(_) => (Vec::new(), Vec::new()),
        }
    }
}

pub(super) fn run(
    heap: &mut Heap,
    op: HostServiceIntrinsic,
    args: &[Value],
    hooks: &mut dyn VmHooks,
) -> Result<Value, Error> {
    use HostServiceIntrinsic as H;
    match op {
        H::ProcessMake => cmd_make(heap, &args[0], &args[1]),
        H::ProcessSetCwd => cmd_set_cwd(heap, &args[0], &args[1]),
        H::ProcessSetEnv => cmd_set_env(heap, &args[0], &args[1]),
        H::ProcessPrefix => cmd_prefix(heap, &args[0], &args[1], &args[2]),
        H::ProcessPipeTo => cmd_pipe_to(heap, &args[0], &args[1]),
        H::ProcessSetStdin => cmd_set_stdin(heap, &args[0], &args[1]),
        H::ProcessInheritOutput => cmd_inherit_output(heap, &args[0]),
        H::ProcessSpawn => {
            if hooks.host_mode() == HostMode::Deterministic {
                return Err(Error::UnsupportedOperation("a child process"));
            }
            spawn(heap, &args[0], hooks)
        }
        H::ProcessClose => proc_close(heap, &args[0], hooks),
        H::ProcessPid => proc_pid(heap, &args[0], hooks),
        H::ProcessWait => proc_wait(heap, &args[0], hooks),
        H::ProcessIsRunning => proc_is_running(heap, &args[0], hooks),
        H::ProcessKill => proc_kill(heap, &args[0], None, hooks),
        H::ProcessKillWith => proc_kill(heap, &args[1], Some(&args[0]), hooks),
        H::ProcessReadStdout => proc_read_pipe(heap, &args[0], true, hooks),
        H::ProcessReadStderr => proc_read_pipe(heap, &args[0], false, hooks),
        H::ProcessOutputString => proc_output_string(heap, &args[0], hooks),
        H::ProcessOutputLines => proc_output_lines(heap, &args[0], hooks),
        _ => Err(Error::UnsupportedPrimitive("unexpected process op".into())),
    }
}

fn spawn_pipe_reader<R: Read + Send + 'static>(
    mut reader: R,
) -> (Receiver<Vec<u8>>, JoinHandle<()>) {
    let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(64);
    let handle = thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        while matches!(reader.read(&mut buf), Ok(k) if k > 0) {}
                        break;
                    }
                }
            }
        }
    });
    (rx, handle)
}

fn spawn_inherit_collector<R: Read + Send + 'static>(mut reader: R) -> JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = reader.read_to_end(&mut buf);
        buf
    })
}

fn flush_inherited(hooks: &mut dyn VmHooks, out_bytes: &[u8], err_bytes: &[u8]) {
    if !out_bytes.is_empty() {
        let text = String::from_utf8_lossy(out_bytes);
        hooks.write_stdout(&text);
    }
    if !err_bytes.is_empty() {
        let text = String::from_utf8_lossy(err_bytes);
        hooks.write_stderr(&text);
    }
}

fn spawn_pipeline(specs: &[SingleCmd]) -> Result<Vec<Child>, io::Error> {
    let mut spawned: Vec<Child> = Vec::with_capacity(specs.len());
    let mut prev_stdout: Option<ChildStdout> = None;

    for (idx, spec) in specs.iter().enumerate() {
        let last = idx + 1 == specs.len();
        let mut cmd = Command::new(&spec.program);
        cmd.args(&spec.args);
        if let Some(dir) = &spec.cwd {
            cmd.current_dir(dir);
        }
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt as _;
            cmd.process_group(0);
        }
        if spec.stdin.is_some() {
            cmd.stdin(Stdio::piped());
        } else if let Some(prev) = prev_stdout.take() {
            cmd.stdin(Stdio::from(prev));
        } else {
            cmd.stdin(Stdio::null());
        }
        cmd.stdout(Stdio::piped());
        cmd.stderr(if last { Stdio::piped() } else { Stdio::null() });
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(err) => {
                for started in spawned.iter_mut().rev() {
                    let _ = send_signal(started, "SIGKILL");
                    let _ = started.wait();
                }
                return Err(err);
            }
        };
        if let Some(text) = &spec.stdin
            && let Some(mut stdin_pipe) = child.stdin.take()
        {
            let bytes = text.as_bytes().to_vec();
            thread::spawn(move || {
                let _ = stdin_pipe.write_all(&bytes);
            });
        }
        if !last {
            prev_stdout = child.stdout.take();
        }
        spawned.push(child);
    }
    Ok(spawned)
}

pub(super) fn spawn(
    heap: &mut Heap,
    cmd_val: &Value,
    hooks: &mut dyn VmHooks,
) -> Result<Value, Error> {
    let mut specs = Vec::new();
    collect_commands(heap, cmd_val, &mut specs)?;
    let cmd_text = format_cmd_text(&specs);
    let inherit_output = specs.last().is_some_and(|s| s.inherit);
    let mut spawned = match spawn_pipeline(&specs) {
        Ok(children) => children,
        Err(err) => {
            return platform_sys_err_tag(
                heap,
                system_error_tag(&err),
                "ChildProcess",
                "spawn",
                None,
                None,
                Some(&cmd_text),
            );
        }
    };

    let (
        stdout_rx,
        stdout_thread,
        stderr_rx,
        stderr_thread,
        inherit_stdout,
        inherit_stderr,
        last_pid,
    ) = if let Some(last_child) = spawned.last_mut() {
        #[expect(clippy::cast_possible_wrap, reason = "PID fits i32 on POSIX/Windows")]
        let pid = last_child.id() as i32;
        if inherit_output {
            let out_t = last_child.stdout.take().map(spawn_inherit_collector);
            let err_t = last_child.stderr.take().map(spawn_inherit_collector);
            (None, None, None, None, out_t, err_t, pid)
        } else {
            let (out_rx, out_t) = match last_child.stdout.take() {
                Some(pipe) => {
                    let (rx, t) = spawn_pipe_reader(pipe);
                    (Some(rx), Some(t))
                }
                None => (None, None),
            };
            let (err_rx, err_t) = match last_child.stderr.take() {
                Some(pipe) => {
                    let (rx, t) = spawn_pipe_reader(pipe);
                    (Some(rx), Some(t))
                }
                None => (None, None),
            };
            (out_rx, out_t, err_rx, err_t, None, None, pid)
        }
    } else {
        (None, None, None, None, None, None, 0)
    };

    let svc = hooks
        .host_services()
        .ok_or(Error::UnsupportedOperation("host services unavailable"))?;
    let id = svc.alloc_handle();
    svc.processes.insert(
        id,
        ProcessEntry {
            cmds: spawned,
            last_pid,
            cmd_text,
            stdout_rx,
            stderr_rx,
            stdout_thread,
            stderr_thread,
            inherit_stdout,
            inherit_stderr,
            status: None,
        },
    );
    let handle = heap.construct(PROC_HANDLE_TAG, vec![Value::Int64(id)])?;
    ok(heap, handle)
}

fn proc_close(heap: &Heap, handle_val: &Value, hooks: &mut dyn VmHooks) -> Result<Value, Error> {
    let id = handle_id(heap, handle_val, PROC_HANDLE_TAG)?;
    let inherited = if let Some(svc) = hooks.host_services()
        && let Some(mut entry) = svc.processes.remove(&id)
    {
        if entry.is_running() || entry.status.is_none_or(|st| !st.success()) {
            Some(entry.terminate("SIGTERM", Some(Duration::from_millis(500))))
        } else {
            entry.wait_all().ok().map(|(_, o, e)| (o, e))
        }
    } else {
        None
    };
    if let Some((out_bytes, err_bytes)) = inherited {
        flush_inherited(hooks, &out_bytes, &err_bytes);
    }
    Ok(Value::Unit)
}

fn proc_pid(heap: &Heap, handle_val: &Value, hooks: &mut dyn VmHooks) -> Result<Value, Error> {
    let id = handle_id(heap, handle_val, PROC_HANDLE_TAG)?;
    let svc = hooks
        .host_services()
        .ok_or(Error::UnsupportedOperation("host services unavailable"))?;
    let pid = svc.processes.get(&id).map_or(0, |p| p.last_pid);
    Ok(Value::Int(pid))
}

fn proc_is_running(
    heap: &mut Heap,
    handle_val: &Value,
    hooks: &mut dyn VmHooks,
) -> Result<Value, Error> {
    let id = handle_id(heap, handle_val, PROC_HANDLE_TAG)?;
    let svc = hooks
        .host_services()
        .ok_or(Error::UnsupportedOperation("host services unavailable"))?;
    let running = svc
        .processes
        .get_mut(&id)
        .is_some_and(ProcessEntry::is_running);
    ok(heap, Value::Bool(running))
}

fn proc_wait(heap: &mut Heap, handle_val: &Value, hooks: &mut dyn VmHooks) -> Result<Value, Error> {
    let id = handle_id(heap, handle_val, PROC_HANDLE_TAG)?;
    let (cmd_text, wait_res) = {
        let svc = hooks
            .host_services()
            .ok_or(Error::UnsupportedOperation("host services unavailable"))?;
        let Some(entry) = svc.processes.get_mut(&id) else {
            return platform_sys_err_tag(
                heap,
                "BadResource",
                "ChildProcess",
                "exitCode",
                None,
                None,
                None,
            );
        };
        let cmd_text = entry.cmd_text.clone();
        let res = entry.wait_all();
        (cmd_text, res)
    };
    match wait_res {
        Ok((Some(st), out_bytes, err_bytes)) => {
            flush_inherited(hooks, &out_bytes, &err_bytes);
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt as _;
                if let Some(sig) = st.signal() {
                    let msg = format!("Process interrupted due to receipt of signal: '{sig}'");
                    return platform_sys_err_tag(
                        heap,
                        "UnknownError",
                        "ChildProcess",
                        "exitCode",
                        Some(&msg),
                        None,
                        Some(&cmd_text),
                    );
                }
            }
            ok(heap, Value::Int(st.code().unwrap_or(0)))
        }
        Ok((None, out_bytes, err_bytes)) => {
            flush_inherited(hooks, &out_bytes, &err_bytes);
            ok(heap, Value::Int(0))
        }
        Err(err) => {
            let msg = err.to_string();
            platform_sys_err_tag(
                heap,
                "UnknownError",
                "ChildProcess",
                "exitCode",
                Some(&msg),
                None,
                Some(&cmd_text),
            )
        }
    }
}

fn proc_kill(
    heap: &mut Heap,
    handle_val: &Value,
    opts_val: Option<&Value>,
    hooks: &mut dyn VmHooks,
) -> Result<Value, Error> {
    let id = handle_id(heap, handle_val, PROC_HANDLE_TAG)?;
    let (signal, force_after) = if let Some(opts) = opts_val {
        let sig = heap.string_lossy(record_field(heap, opts, "signal")?)?;
        let after = match option_some(heap, record_field(heap, opts, "force_after")?)? {
            Some(dur) => duration_sleep_nanos(heap, dur)?.map(Duration::from_nanos),
            None => None,
        };
        (sig, after)
    } else {
        ("SIGTERM".to_owned(), None)
    };
    let (out_bytes, err_bytes) = {
        let svc = hooks
            .host_services()
            .ok_or(Error::UnsupportedOperation("host services unavailable"))?;
        let Some(entry) = svc.processes.get_mut(&id) else {
            return ok(heap, Value::Unit);
        };
        entry.terminate(&signal, force_after)
    };
    flush_inherited(hooks, &out_bytes, &err_bytes);
    ok(heap, Value::Unit)
}

fn proc_read_pipe(
    heap: &mut Heap,
    handle_val: &Value,
    is_stdout: bool,
    hooks: &mut dyn VmHooks,
) -> Result<Value, Error> {
    let id = handle_id(heap, handle_val, PROC_HANDLE_TAG)?;
    let svc = hooks
        .host_services()
        .ok_or(Error::UnsupportedOperation("host services unavailable"))?;
    let Some(entry) = svc.processes.get_mut(&id) else {
        let none = opt_none(heap)?;
        return ok(heap, none);
    };
    let rx_slot = if is_stdout {
        &mut entry.stdout_rx
    } else {
        &mut entry.stderr_rx
    };
    let chunk = if let Some(Ok(bytes)) = rx_slot.as_ref().map(Receiver::recv) {
        Some(bytes)
    } else {
        *rx_slot = None;
        None
    };
    if let Some(bytes) = chunk {
        let list = int_list_from_bytes(heap, &bytes)?;
        let some = opt_some(heap, list)?;
        ok(heap, some)
    } else {
        let none = opt_none(heap)?;
        ok(heap, none)
    }
}

fn collect_stdout_string(
    heap: &Heap,
    handle_val: &Value,
    hooks: &mut dyn VmHooks,
) -> Result<String, Error> {
    let id = handle_id(heap, handle_val, PROC_HANDLE_TAG)?;
    let svc = hooks
        .host_services()
        .ok_or(Error::UnsupportedOperation("host services unavailable"))?;
    let Some(entry) = svc.processes.get_mut(&id) else {
        return Ok(String::new());
    };
    entry.stderr_rx = None;
    let mut out = Vec::new();
    if let Some(rx) = entry.stdout_rx.take() {
        while let Ok(chunk) = rx.recv() {
            out.extend_from_slice(&chunk);
        }
    }
    let _ = entry.wait_all();
    Ok(String::from_utf8_lossy(&out).into_owned())
}

fn proc_output_string(
    heap: &mut Heap,
    handle_val: &Value,
    hooks: &mut dyn VmHooks,
) -> Result<Value, Error> {
    let text = collect_stdout_string(heap, handle_val, hooks)?;
    let val = heap.literal(&Lit::String(text))?;
    ok(heap, val)
}

fn proc_output_lines(
    heap: &mut Heap,
    handle_val: &Value,
    hooks: &mut dyn VmHooks,
) -> Result<Value, Error> {
    let text = collect_stdout_string(heap, handle_val, hooks)?;
    let lines = split_stream_lines(&text);
    let list = vec_to_string_list(heap, lines)?;
    ok(heap, list)
}
