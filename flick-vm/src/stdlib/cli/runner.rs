//! Resumable CLI runner state machine shared between tier 0 and tier 1.
//!
//! All argument parsing, `Cli.Value` dictionary method resolution, `Result`
//! unwrapping, options-record construction, and `CliError` formatting live
//! here. The calling tier only drives `CliStep::Call` and
//! `CliStep::InvokeHandler` on its own frame stack.

use std::collections::VecDeque;
use std::mem;

use flick_core::ir::Lit;

use crate::stdlib::hooks::{VmHooks, charge};
use crate::tier0::{Data, DictionaryValue, Error, Heap, Value};

use super::args::{self, Action};
use super::{CliMultiplicity, expect_spec, read_string_list};

/// One step returned by [`start_run`] or [`resume_run`].
pub(crate) enum CliStep {
    /// Invoke `callee(arg)` and pass the returned `Value` back via [`resume_run`].
    Call {
        callee: Value,
        arg: Value,
        state: Box<CliRunState>,
    },
    /// All parameters parsed; invoke the command's `handler(options)` directly.
    InvokeHandler { handler: Value, options: Value },
    /// Completed normally with `Value` (`Value::Unit`).
    Return(Value),
    /// Completed with a typed `Cli.CliError` raise payload.
    Raise(Value),
}

struct PendingParam {
    flag: bool,
    label: String,
    multi: CliMultiplicity,
    dictionary: Value,
    raw_values: VecDeque<String>,
    parsed: Vec<Value>,
}

impl PendingParam {
    fn drain_roots(self, roots: &mut Vec<Value>) {
        roots.push(self.dictionary);
        roots.extend(self.parsed);
    }
}

enum RunPhase {
    Idle,
    AwaitingGetter { raw: String },
    AwaitingParse { raw: String },
}

/// Resumable state for one in-flight `Cli.run_version` invocation.
pub(crate) struct CliRunState {
    help: String,
    handler: Option<Value>,
    pending_params: VecDeque<PendingParam>,
    active_param: Option<PendingParam>,
    built_fields: Vec<(String, Value)>,
    phase: RunPhase,
}

impl CliRunState {
    /// Moves every owned heap [`Value`] held by this state into `roots` for
    /// leak-free teardown on unwind, discard, or execution cleanup.
    pub(crate) fn drain_roots(self, roots: &mut Vec<Value>) {
        if let Some(handler) = self.handler {
            roots.push(handler);
        }
        if let Some(active) = self.active_param {
            active.drain_roots(roots);
        }
        for param in self.pending_params {
            param.drain_roots(roots);
        }
        for (_, val) in self.built_fields {
            roots.push(val);
        }
    }

    fn drop_all(self, heap: &mut Heap) -> Result<(), Error> {
        let mut roots = Vec::new();
        self.drain_roots(&mut roots);
        heap.drop_values(roots)
    }
}

/// Starts `Cli.run_version(version, spec, argv)` from borrowed arguments.
pub(crate) fn start_run(
    heap: &mut Heap,
    args: &[Value],
    hooks: &mut dyn VmHooks,
) -> Result<CliStep, Error> {
    heap.admit_values(args)?;
    charge(hooks, 1)?;
    let version = heap.string(&args[0])?.to_owned();
    let cli_args = read_string_list(heap, &args[2], hooks)?;
    charge(hooks, version.len())?;

    let action = {
        let spec = expect_spec(heap, &args[1])?;
        args::parse(spec, &version, &cli_args)
    };
    let (path_indices, raw_values, help_text) = match action {
        Ok(Action::Display(text)) => {
            hooks.write_stdout(&text);
            return Ok(CliStep::Return(Value::Unit));
        }
        Ok(Action::Wizard { help }) => {
            return reject_cli(heap, &help, "Wizard requires a terminal", hooks);
        }
        Err(error) => {
            return reject_cli(heap, &error.help, &error.message, hooks);
        }
        Ok(Action::Invoke {
            path_indices,
            values,
            help,
        }) => (path_indices, values, help),
    };

    let (param_specs, handler_rep) = {
        let mut selected = expect_spec(heap, &args[1])?;
        for idx in path_indices {
            selected = &selected.subcommands[idx].0;
        }
        let params: Vec<_> = selected
            .params
            .iter()
            .zip(raw_values)
            .map(|(p, raws)| {
                (
                    p.flag,
                    p.label.clone(),
                    p.multi,
                    p.dictionary.representation(),
                    raws,
                )
            })
            .collect();
        let handler_rep = selected.handler.as_ref().map(Value::representation);
        (params, handler_rep)
    };

    let mut pending_params = VecDeque::with_capacity(param_specs.len());
    for (flag, label, multi, dict_rep, raws) in param_specs {
        match heap.dup(&dict_rep) {
            Ok(dictionary) => pending_params.push_back(PendingParam {
                flag,
                label,
                multi,
                dictionary,
                raw_values: raws.into(),
                parsed: Vec::new(),
            }),
            Err(err) => {
                let mut roots = Vec::new();
                for p in pending_params {
                    p.drain_roots(&mut roots);
                }
                heap.drop_values(roots)?;
                return Err(err);
            }
        }
    }
    let handler = match handler_rep {
        Some(rep) => match heap.dup(&rep) {
            Ok(h) => Some(h),
            Err(err) => {
                let mut roots = Vec::new();
                for p in pending_params {
                    p.drain_roots(&mut roots);
                }
                heap.drop_values(roots)?;
                return Err(err);
            }
        },
        None => None,
    };

    let state = CliRunState {
        help: help_text,
        handler,
        pending_params,
        active_param: None,
        built_fields: Vec::new(),
        phase: RunPhase::Idle,
    };
    advance(state, heap, hooks)
}

/// Resumes `state` after a `CliStep::Call` callback returned `returned`.
pub(crate) fn resume_run(
    mut state: CliRunState,
    heap: &mut Heap,
    returned: Value,
    hooks: &mut dyn VmHooks,
) -> Result<CliStep, Error> {
    let phase = mem::replace(&mut state.phase, RunPhase::Idle);
    match phase {
        RunPhase::Idle => {
            heap.drop_value(returned)?;
            state.drop_all(heap)?;
            Err(Error::UnsupportedPrimitive(
                "CLI runner resumed while idle".into(),
            ))
        }
        RunPhase::AwaitingGetter { raw } => {
            let text_val = match heap.literal(&Lit::String(raw.clone())) {
                Ok(v) => v,
                Err(err) => {
                    heap.drop_value(returned)?;
                    state.drop_all(heap)?;
                    return Err(err);
                }
            };
            state.phase = RunPhase::AwaitingParse { raw };
            Ok(CliStep::Call {
                callee: returned,
                arg: text_val,
                state: Box::new(state),
            })
        }
        RunPhase::AwaitingParse { raw } => handle_parse_result(state, heap, returned, &raw, hooks),
    }
}

fn handle_parse_result(
    mut state: CliRunState,
    heap: &mut Heap,
    returned: Value,
    raw: &str,
    hooks: &mut dyn VmHooks,
) -> Result<CliStep, Error> {
    let parsed_kind = {
        let res = inspect_result(heap, &returned);
        match res {
            Ok(is_ok) => is_ok,
            Err(err) => {
                heap.drop_value(returned)?;
                state.drop_all(heap)?;
                return Err(err);
            }
        }
    };
    let payload = match heap.project(&returned, 0) {
        Ok(p) => p,
        Err(err) => {
            heap.drop_value(returned)?;
            state.drop_all(heap)?;
            return Err(err);
        }
    };
    heap.drop_value(returned)?;

    if parsed_kind {
        state
            .active_param
            .as_mut()
            .expect("active CLI param")
            .parsed
            .push(payload);
        advance(state, heap, hooks)
    } else {
        let err_msg = match heap.string_lossy(&payload) {
            Ok(msg) => msg,
            Err(err) => {
                heap.drop_value(payload)?;
                state.drop_all(heap)?;
                return Err(err);
            }
        };
        heap.drop_value(payload)?;
        let active = state.active_param.as_ref().expect("active CLI param");
        let message = args::invalid_value(active.flag, &active.label, raw, &err_msg);
        let help_text = mem::take(&mut state.help);
        state.drop_all(heap)?;
        reject_cli(heap, &help_text, &message, hooks)
    }
}

fn advance(
    mut state: CliRunState,
    heap: &mut Heap,
    hooks: &mut dyn VmHooks,
) -> Result<CliStep, Error> {
    loop {
        if state.active_param.is_none() {
            let Some(next) = state.pending_params.pop_front() else {
                return finish_parsing(state, heap);
            };
            state.active_param = Some(next);
        }

        let next_raw = state
            .active_param
            .as_mut()
            .expect("active CLI param")
            .raw_values
            .pop_front();
        if let Some(raw) = next_raw {
            return invoke_param_parser(state, heap, raw, hooks);
        }

        let param = state.active_param.take().expect("active CLI param");
        let label = param.label;
        let multi = param.multi;
        let parsed = param.parsed;
        heap.drop_value(param.dictionary)?;

        let value = match package_multi(heap, multi, parsed) {
            Ok(v) => v,
            Err(err) => {
                state.drop_all(heap)?;
                return Err(err);
            }
        };
        state.built_fields.push((label, value));
    }
}

fn invoke_param_parser(
    mut state: CliRunState,
    heap: &mut Heap,
    raw: String,
    hooks: &mut dyn VmHooks,
) -> Result<CliStep, Error> {
    charge(hooks, 1)?;
    let dict_rep = state
        .active_param
        .as_ref()
        .expect("active CLI param")
        .dictionary
        .representation();
    let method_lookup = lookup_parse_method(heap, &dict_rep);
    let (fn_rep, is_getter) = match method_lookup {
        Ok(found) => found,
        Err(err) => {
            state.drop_all(heap)?;
            return Err(err);
        }
    };
    let callee = match heap.dup(&fn_rep) {
        Ok(v) => v,
        Err(err) => {
            state.drop_all(heap)?;
            return Err(err);
        }
    };
    if is_getter {
        let dict_dup = match heap.dup(&dict_rep) {
            Ok(v) => v,
            Err(err) => {
                heap.drop_value(callee)?;
                state.drop_all(heap)?;
                return Err(err);
            }
        };
        state.phase = RunPhase::AwaitingGetter { raw };
        Ok(CliStep::Call {
            callee,
            arg: dict_dup,
            state: Box::new(state),
        })
    } else {
        let text_val = match heap.literal(&Lit::String(raw.clone())) {
            Ok(v) => v,
            Err(err) => {
                heap.drop_value(callee)?;
                state.drop_all(heap)?;
                return Err(err);
            }
        };
        state.phase = RunPhase::AwaitingParse { raw };
        Ok(CliStep::Call {
            callee,
            arg: text_val,
            state: Box::new(state),
        })
    }
}

fn finish_parsing(mut state: CliRunState, heap: &mut Heap) -> Result<CliStep, Error> {
    let mut fields = mem::take(&mut state.built_fields);
    fields.sort_by(|(a, _), (b, _)| a.encode_utf16().cmp(b.encode_utf16()));
    let handler = state.handler.take();
    let options = match heap.record(fields) {
        Ok(v) => v,
        Err(err) => {
            if let Some(h) = handler {
                heap.drop_value(h)?;
            }
            return Err(err);
        }
    };
    if let Some(handler) = handler {
        Ok(CliStep::InvokeHandler { handler, options })
    } else {
        heap.drop_value(options)?;
        Ok(CliStep::Return(Value::Unit))
    }
}

fn package_multi(
    heap: &mut Heap,
    multi: CliMultiplicity,
    mut parsed: Vec<Value>,
) -> Result<Value, Error> {
    match multi {
        CliMultiplicity::Required => {
            if parsed.len() == 1 {
                Ok(parsed.pop().expect("one required value"))
            } else {
                heap.drop_values(parsed)?;
                Err(Error::UnsupportedPrimitive(
                    "expected valid CLI required multiplicity".into(),
                ))
            }
        }
        CliMultiplicity::Optional => match parsed.len() {
            0 => heap.construct("Prelude.Option.None", vec![]),
            1 => heap.construct("Prelude.Option.Some", parsed),
            _ => {
                heap.drop_values(parsed)?;
                Err(Error::UnsupportedPrimitive(
                    "expected valid CLI optional multiplicity".into(),
                ))
            }
        },
        CliMultiplicity::Many => heap.construct("$list", parsed),
    }
}

fn lookup_parse_method(heap: &Heap, dict: &Value) -> Result<(Value, bool), Error> {
    let Value::Object(token) = dict else {
        return Err(Error::ExpectedDictionary);
    };
    let Data::Dictionary(entries) = &heap.object(token.id)?.data else {
        return Err(Error::ExpectedDictionary);
    };
    for (name, entry) in entries {
        if name == "parse" {
            return Ok((
                entry.value().representation(),
                matches!(entry, DictionaryValue::Getter(_)),
            ));
        }
    }
    Err(Error::MissingMethod("parse".into()))
}

fn inspect_result(heap: &Heap, value: &Value) -> Result<bool, Error> {
    let Value::Object(token) = value else {
        return Err(Error::ExpectedResult);
    };
    let Data::Tagged {
        tag,
        ordinary_fields,
        fields,
        ..
    } = &heap.object(token.id)?.data
    else {
        return Err(Error::ExpectedResult);
    };
    if *ordinary_fields != 1 || fields.len() != 1 {
        return Err(Error::ExpectedResult);
    }
    match tag.as_str() {
        "Prelude.Result.Ok" => Ok(true),
        "Prelude.Result.Err" => Ok(false),
        _ => Err(Error::ExpectedResult),
    }
}

fn reject_cli(
    heap: &mut Heap,
    help_text: &str,
    message: &str,
    hooks: &mut dyn VmHooks,
) -> Result<CliStep, Error> {
    hooks.write_stdout(help_text);
    let msg_val = heap.literal(&Lit::String(message.to_owned()))?;
    let payload = heap.construct("Cli.CliError.CliError", vec![msg_val])?;
    Ok(CliStep::Raise(payload))
}
