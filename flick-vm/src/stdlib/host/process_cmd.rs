//! Pure immutable `ChildProcess.Command` builders and pipeline decoding.

use std::io;
use std::mem;
use std::process::{Child, Command, Stdio};

use flick_core::ir::Lit;

use crate::tier0::{Error, Heap, Value};

use super::error_map::{
    list_slice, opt_none, opt_some, option_some, string_list_to_vec, vec_to_string_list,
};

const CMD_SINGLE_TAG: &str = "$vm_cmd_single";
const CMD_PIPE_TAG: &str = "$vm_cmd_pipe";

#[derive(Clone, Debug)]
pub(super) struct SingleCmd {
    pub(super) program: String,
    pub(super) args: Vec<String>,
    pub(super) cwd: Option<String>,
    pub(super) env: Vec<(String, String)>,
    pub(super) stdin: Option<String>,
    pub(super) inherit: bool,
}

pub(super) fn cmd_make(heap: &mut Heap, prog: &Value, args: &Value) -> Result<Value, Error> {
    let p = heap.dup(prog)?;
    let a = heap.dup(args)?;
    let none_cwd = opt_none(heap)?;
    let empty_env = heap.construct("$list", vec![])?;
    let none_stdin = opt_none(heap)?;
    heap.construct(
        CMD_SINGLE_TAG,
        vec![p, a, none_cwd, empty_env, none_stdin, Value::Bool(false)],
    )
}

fn map_cmd_all(
    heap: &mut Heap,
    cmd: &Value,
    f: &dyn Fn(&mut Heap, &Value) -> Result<Value, Error>,
) -> Result<Value, Error> {
    let tag = heap.tag(cmd)?.to_owned();
    if tag == CMD_PIPE_TAG {
        let left = heap.project(cmd, 0)?;
        let right = heap.project(cmd, 1)?;
        let nl = map_cmd_all(heap, &left, f);
        heap.drop_value(left)?;
        let nl = match nl {
            Ok(v) => v,
            Err(e) => {
                heap.drop_value(right)?;
                return Err(e);
            }
        };
        let nr = map_cmd_all(heap, &right, f);
        heap.drop_value(right)?;
        let nr = match nr {
            Ok(v) => v,
            Err(e) => {
                heap.drop_value(nl)?;
                return Err(e);
            }
        };
        heap.construct(CMD_PIPE_TAG, vec![nl, nr])
    } else {
        f(heap, cmd)
    }
}

fn map_cmd_edge(
    heap: &mut Heap,
    cmd: &Value,
    first: bool,
    f: &dyn Fn(&mut Heap, &Value) -> Result<Value, Error>,
) -> Result<Value, Error> {
    let tag = heap.tag(cmd)?.to_owned();
    if tag == CMD_PIPE_TAG {
        let left = heap.project(cmd, 0)?;
        let right = heap.project(cmd, 1)?;
        if first {
            let nl = map_cmd_edge(heap, &left, true, f);
            heap.drop_value(left)?;
            let nl = match nl {
                Ok(v) => v,
                Err(e) => {
                    heap.drop_value(right)?;
                    return Err(e);
                }
            };
            heap.construct(CMD_PIPE_TAG, vec![nl, right])
        } else {
            let nr = map_cmd_edge(heap, &right, false, f);
            heap.drop_value(right)?;
            let nr = match nr {
                Ok(v) => v,
                Err(e) => {
                    heap.drop_value(left)?;
                    return Err(e);
                }
            };
            heap.construct(CMD_PIPE_TAG, vec![left, nr])
        }
    } else {
        f(heap, cmd)
    }
}

fn clone_single_fields(heap: &mut Heap, cmd: &Value) -> Result<Vec<Value>, Error> {
    let mut fields = Vec::with_capacity(6);
    for i in 0..6 {
        match heap.project(cmd, i) {
            Ok(v) => fields.push(v),
            Err(e) => {
                heap.drop_values(fields)?;
                return Err(e);
            }
        }
    }
    Ok(fields)
}

pub(super) fn cmd_set_cwd(heap: &mut Heap, cwd: &Value, cmd: &Value) -> Result<Value, Error> {
    let cwd_str = heap.string_lossy(cwd)?;
    map_cmd_all(heap, cmd, &|heap, single| {
        let mut fields = clone_single_fields(heap, single)?;
        let old = mem::replace(&mut fields[2], Value::Unit);
        heap.drop_value(old)?;
        let s = heap.literal(&Lit::String(cwd_str.clone()))?;
        fields[2] = opt_some(heap, s)?;
        heap.construct(CMD_SINGLE_TAG, fields)
    })
}

pub(super) fn cmd_set_env(heap: &mut Heap, env_val: &Value, cmd: &Value) -> Result<Value, Error> {
    let mut pairs = Vec::new();
    for item in list_slice(heap, env_val)? {
        let k = heap.string_lossy(heap.field(item, 0)?)?;
        let v = heap.string_lossy(heap.field(item, 1)?)?;
        pairs.push((k, v));
    }
    map_cmd_all(heap, cmd, &|heap, single| {
        let mut fields = clone_single_fields(heap, single)?;
        let existing_items: Vec<Value> = {
            let slice = list_slice(heap, &fields[3])?;
            slice.iter().map(Value::representation).collect()
        };
        let mut merged = Vec::with_capacity(existing_items.len() + pairs.len());
        for rep in &existing_items {
            merged.push(heap.dup(rep)?);
        }
        for (k, v) in &pairs {
            let kv = heap.literal(&Lit::String(k.clone()))?;
            let vv = heap.literal(&Lit::String(v.clone()))?;
            merged.push(heap.construct("$tuple", vec![kv, vv])?);
        }
        let old = mem::replace(&mut fields[3], heap.construct("$list", merged)?);
        heap.drop_value(old)?;
        heap.construct(CMD_SINGLE_TAG, fields)
    })
}

pub(super) fn cmd_prefix(
    heap: &mut Heap,
    prog_val: &Value,
    args_val: &Value,
    cmd: &Value,
) -> Result<Value, Error> {
    let new_prog = heap.string_lossy(prog_val)?;
    let prefix_args = string_list_to_vec(heap, args_val)?;
    map_cmd_edge(heap, cmd, true, &|heap, single| {
        let mut fields = clone_single_fields(heap, single)?;
        let old_prog = heap.string_lossy(&fields[0])?;
        let old_args = string_list_to_vec(heap, &fields[1])?;
        let mut combined = prefix_args.clone();
        combined.push(old_prog);
        combined.extend(old_args);
        let p = heap.literal(&Lit::String(new_prog.clone()))?;
        let a = vec_to_string_list(heap, combined)?;
        let old_p = mem::replace(&mut fields[0], p);
        let old_a = mem::replace(&mut fields[1], a);
        heap.drop_values(vec![old_p, old_a])?;
        heap.construct(CMD_SINGLE_TAG, fields)
    })
}

pub(super) fn cmd_pipe_to(heap: &mut Heap, that: &Value, self_cmd: &Value) -> Result<Value, Error> {
    let left = heap.dup(self_cmd)?;
    let right = heap.dup(that)?;
    heap.construct(CMD_PIPE_TAG, vec![left, right])
}

pub(super) fn cmd_set_stdin(
    heap: &mut Heap,
    text_val: &Value,
    cmd: &Value,
) -> Result<Value, Error> {
    let text = heap.string_lossy(text_val)?;
    map_cmd_edge(heap, cmd, true, &|heap, single| {
        let mut fields = clone_single_fields(heap, single)?;
        let old = mem::replace(&mut fields[4], Value::Unit);
        heap.drop_value(old)?;
        let s = heap.literal(&Lit::String(text.clone()))?;
        fields[4] = opt_some(heap, s)?;
        heap.construct(CMD_SINGLE_TAG, fields)
    })
}

pub(super) fn cmd_inherit_output(heap: &mut Heap, cmd: &Value) -> Result<Value, Error> {
    map_cmd_edge(heap, cmd, false, &|heap, single| {
        let mut fields = clone_single_fields(heap, single)?;
        fields[5] = Value::Bool(true);
        heap.construct(CMD_SINGLE_TAG, fields)
    })
}

pub(super) fn collect_commands(
    heap: &Heap,
    cmd: &Value,
    out: &mut Vec<SingleCmd>,
) -> Result<(), Error> {
    let tag = heap.tag(cmd)?;
    if tag == CMD_PIPE_TAG {
        collect_commands(heap, heap.field(cmd, 0)?, out)?;
        collect_commands(heap, heap.field(cmd, 1)?, out)?;
        return Ok(());
    }
    let program = heap.string_lossy(heap.field(cmd, 0)?)?;
    let args = string_list_to_vec(heap, heap.field(cmd, 1)?)?;
    let cwd = match option_some(heap, heap.field(cmd, 2)?)? {
        Some(v) => Some(heap.string_lossy(v)?),
        None => None,
    };
    let mut env = Vec::new();
    for item in list_slice(heap, heap.field(cmd, 3)?)? {
        let k = heap.string_lossy(heap.field(item, 0)?)?;
        let v = heap.string_lossy(heap.field(item, 1)?)?;
        env.push((k, v));
    }
    let stdin = match option_some(heap, heap.field(cmd, 4)?)? {
        Some(v) => Some(heap.string_lossy(v)?),
        None => None,
    };
    let inherit = matches!(heap.field(cmd, 5)?, Value::Bool(true));
    out.push(SingleCmd {
        program,
        args,
        cwd,
        env,
        stdin,
        inherit,
    });
    Ok(())
}

pub(super) fn format_cmd_text(cmds: &[SingleCmd]) -> String {
    cmds.iter()
        .map(|c| format!("{} {}", c.program, c.args.join(" ")))
        .collect::<Vec<_>>()
        .join(" | ")
}

pub(super) fn split_stream_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut start = 0usize;
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            let end = if i > start && bytes[i - 1] == b'\r' {
                i - 1
            } else {
                i
            };
            out.push(text[start..end].to_owned());
            start = i + 1;
        }
        i += 1;
    }
    if start < bytes.len() {
        out.push(text[start..].to_owned());
    }
    out
}

pub(super) fn send_signal(child: &mut Child, signal: &str) -> io::Result<()> {
    #[cfg(unix)]
    {
        let pid = child.id();
        let sig_name = signal.strip_prefix("SIG").unwrap_or(signal);
        let group = format!("-{pid}");
        let status = Command::new("/bin/kill")
            .args(["-s", sig_name, "--", &group])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if status.is_ok_and(|s| s.success()) {
            return Ok(());
        }
        let direct = Command::new("/bin/kill")
            .args(["-s", sig_name, "--", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if direct.is_ok_and(|s| s.success()) {
            return Ok(());
        }
        child.kill()
    }
    #[cfg(not(unix))]
    {
        let _ = signal;
        child.kill()
    }
}
