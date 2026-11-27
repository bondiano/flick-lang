//! Tier-neutral CLI command specification, argument parsing, help rendering,
//! shell completion generation, and resumable command execution.
//!
//! Both tier 0 and tier 1 delegate every [`CliIntrinsic`] directly to this
//! module over [`crate::tier0::Heap`].

use flick_core::ir::memory::CliIntrinsic;

use crate::stdlib::hooks::{Fuel, VmHooks, charge};
use crate::tier0::{Data, Error, Heap, Value};

mod args;
mod completions;
mod completions_posix;
mod completions_ps;
mod help;
mod runner;

pub(crate) use runner::{CliRunState, CliStep, resume_run, start_run};

/// Multiplicity of a CLI parameter (`Cli.required`, `Cli.optional`, `Cli.many`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CliMultiplicity {
    Required,
    Optional,
    Many,
}

/// One flag or positional argument specification inside a [`CliSpec`].
#[derive(Debug)]
pub(crate) struct CliParam {
    pub(crate) flag: bool,
    pub(crate) label: String,
    pub(crate) aliases: Vec<String>,
    pub(crate) help: String,
    pub(crate) multi: CliMultiplicity,
    pub(crate) dictionary: Value,
}

/// Immutable CLI command specification owning its parameter parser dictionaries
/// and optional handler closure as reference-counted heap [`Value`] tokens.
#[derive(Debug)]
pub(crate) struct CliSpec {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) params: Vec<CliParam>,
    pub(crate) subcommands: Vec<(Self, String)>,
    pub(crate) handler: Option<Value>,
}

impl CliSpec {
    /// Pushes every owned heap [`Value`] contained in this spec tree into `out`.
    pub(crate) fn drain_values(self, out: &mut Vec<Value>) {
        for param in self.params {
            out.push(param.dictionary);
        }
        if let Some(handler) = self.handler {
            out.push(handler);
        }
        for (sub, _) in self.subcommands {
            sub.drain_values(out);
        }
    }

    /// Deterministic inline byte-size estimate for heap limit accounting.
    #[must_use]
    pub(crate) fn byte_size(&self) -> usize {
        let mut total = self.name.len().saturating_add(self.description.len());
        for param in &self.params {
            total = total
                .saturating_add(param.label.len())
                .saturating_add(param.help.len())
                .saturating_add(param.aliases.iter().map(String::len).sum())
                .saturating_add(32);
        }
        if self.handler.is_some() {
            total = total.saturating_add(16);
        }
        for (sub, help) in &self.subcommands {
            total = total
                .saturating_add(help.len())
                .saturating_add(sub.byte_size());
        }
        total
    }
}

/// Evaluates any pure CLI builder intrinsic (all [`CliIntrinsic`] variants
/// other than [`CliIntrinsic::RunVersion`]).
pub(crate) fn eval_builder(
    heap: &mut Heap,
    op: CliIntrinsic,
    args: &[Value],
    hooks: Fuel<'_>,
) -> Result<Value, Error> {
    heap.admit_values(args)?;
    charge(hooks, 1)?;
    match op {
        CliIntrinsic::Command => {
            let name = heap.string(&args[0])?.to_owned();
            charge(hooks, name.len())?;
            heap.try_allocate(Data::CliSpec(Box::new(CliSpec {
                name,
                description: String::new(),
                params: Vec::new(),
                subcommands: Vec::new(),
                handler: None,
            })))
        }
        CliIntrinsic::Description => {
            let description = heap.string(&args[0])?.to_owned();
            charge(hooks, description.len())?;
            let mut spec = clone_spec_value(heap, &args[1], hooks)?;
            spec.description = description;
            heap.try_allocate(Data::CliSpec(Box::new(spec)))
        }
        CliIntrinsic::Flag => eval_flag(heap, args, hooks),
        CliIntrinsic::Arg => eval_arg(heap, args, hooks),
        CliIntrinsic::Subcommand => {
            let help_text = heap.string(&args[1])?.to_owned();
            charge(hooks, help_text.len())?;
            let child = clone_spec_value(heap, &args[0], hooks)?;
            let mut parent = match clone_spec_value(heap, &args[2], hooks) {
                Ok(spec) => spec,
                Err(err) => {
                    drop_spec(heap, child)?;
                    return Err(err);
                }
            };
            parent.subcommands.push((child, help_text));
            heap.try_allocate(Data::CliSpec(Box::new(parent)))
        }
        CliIntrinsic::Handle => {
            let mut spec = clone_spec_value(heap, &args[1], hooks)?;
            let handler = match heap.dup(&args[0].representation()) {
                Ok(v) => v,
                Err(err) => {
                    drop_spec(heap, spec)?;
                    return Err(err);
                }
            };
            if let Some(old) = spec.handler.replace(handler) {
                heap.drop_value(old)?;
            }
            heap.try_allocate(Data::CliSpec(Box::new(spec)))
        }
        CliIntrinsic::Group => {
            let _ = expect_spec(heap, &args[0])?;
            heap.dup(&args[0].representation())
        }
        CliIntrinsic::Required => heap.try_allocate(Data::CliMulti(CliMultiplicity::Required)),
        CliIntrinsic::Optional => heap.try_allocate(Data::CliMulti(CliMultiplicity::Optional)),
        CliIntrinsic::Many => heap.try_allocate(Data::CliMulti(CliMultiplicity::Many)),
        CliIntrinsic::RunVersion => Err(Error::NeedsProgram),
    }
}

/// Evaluates [`CliIntrinsic::RunVersion`] via the shared resumable runner.
pub(crate) fn eval_run_version(
    heap: &mut Heap,
    args: &[Value],
    hooks: &mut dyn VmHooks,
) -> Result<CliStep, Error> {
    start_run(heap, args, hooks)
}

fn eval_flag(heap: &mut Heap, args: &[Value], hooks: Fuel<'_>) -> Result<Value, Error> {
    let label = heap.label_str(&args[3])?.to_owned();
    let aliases = read_string_list(heap, &args[4], hooks)?;
    let help_text = heap.string(&args[5])?.to_owned();
    let multi = expect_multi(heap, &args[6])?;
    charge(hooks, label.len().saturating_add(help_text.len()))?;
    let mut spec = clone_spec_value(heap, &args[7], hooks)?;
    let dictionary = match heap.dup(&args[0].representation()) {
        Ok(v) => v,
        Err(err) => {
            drop_spec(heap, spec)?;
            return Err(err);
        }
    };
    spec.params.push(CliParam {
        flag: true,
        label,
        aliases,
        help: help_text,
        multi,
        dictionary,
    });
    heap.try_allocate(Data::CliSpec(Box::new(spec)))
}

fn eval_arg(heap: &mut Heap, args: &[Value], hooks: Fuel<'_>) -> Result<Value, Error> {
    let label = heap.label_str(&args[3])?.to_owned();
    let help_text = heap.string(&args[4])?.to_owned();
    let multi = expect_multi(heap, &args[5])?;
    charge(hooks, label.len().saturating_add(help_text.len()))?;
    let mut spec = clone_spec_value(heap, &args[6], hooks)?;
    let dictionary = match heap.dup(&args[0].representation()) {
        Ok(v) => v,
        Err(err) => {
            drop_spec(heap, spec)?;
            return Err(err);
        }
    };
    spec.params.push(CliParam {
        flag: false,
        label,
        aliases: Vec::new(),
        help: help_text,
        multi,
        dictionary,
    });
    heap.try_allocate(Data::CliSpec(Box::new(spec)))
}

pub(super) fn expect_spec<'a>(heap: &'a Heap, value: &Value) -> Result<&'a CliSpec, Error> {
    let Value::Object(token) = value else {
        return Err(Error::UnsupportedPrimitive("expected a CLI spec".into()));
    };
    let Data::CliSpec(spec) = &heap.object(token.id)?.data else {
        return Err(Error::UnsupportedPrimitive("expected a CLI spec".into()));
    };
    Ok(spec)
}

fn expect_multi(heap: &Heap, value: &Value) -> Result<CliMultiplicity, Error> {
    let Value::Object(token) = value else {
        return Err(Error::UnsupportedPrimitive(
            "expected a CLI multiplicity".into(),
        ));
    };
    let Data::CliMulti(multi) = &heap.object(token.id)?.data else {
        return Err(Error::UnsupportedPrimitive(
            "expected a CLI multiplicity".into(),
        ));
    };
    Ok(*multi)
}

pub(super) fn read_string_list(
    heap: &Heap,
    value: &Value,
    hooks: Fuel<'_>,
) -> Result<Vec<String>, Error> {
    let (backing_id, offset, len) = heap.list_view(value)?;
    charge(hooks, len.saturating_add(1))?;
    let Data::Tagged { fields, .. } = &heap.object(backing_id)?.data else {
        return Err(Error::PatternKind("list"));
    };
    let mut out = Vec::with_capacity(len);
    for item in &fields[offset..offset + len] {
        let text = heap.string(item)?;
        charge(hooks, text.len())?;
        out.push(text.to_owned());
    }
    Ok(out)
}

struct ParamShape {
    flag: bool,
    label: String,
    aliases: Vec<String>,
    help: String,
    multi: CliMultiplicity,
    dictionary: Value,
}

struct SpecShape {
    name: String,
    description: String,
    params: Vec<ParamShape>,
    subcommands: Vec<(Self, String)>,
    handler: Option<Value>,
}

impl SpecShape {
    fn from_spec(spec: &CliSpec) -> (Self, usize) {
        let mut cost = 1usize;
        let params = spec
            .params
            .iter()
            .map(|p| {
                cost = cost.saturating_add(1);
                ParamShape {
                    flag: p.flag,
                    label: p.label.clone(),
                    aliases: p.aliases.clone(),
                    help: p.help.clone(),
                    multi: p.multi,
                    dictionary: p.dictionary.representation(),
                }
            })
            .collect();
        let subcommands = spec
            .subcommands
            .iter()
            .map(|(sub, help)| {
                let (shape, sub_cost) = Self::from_spec(sub);
                cost = cost.saturating_add(sub_cost);
                (shape, help.clone())
            })
            .collect();
        let handler = spec.handler.as_ref().map(Value::representation);
        (
            Self {
                name: spec.name.clone(),
                description: spec.description.clone(),
                params,
                subcommands,
                handler,
            },
            cost,
        )
    }

    fn materialize(self, heap: &mut Heap) -> Result<CliSpec, Error> {
        let mut params = Vec::with_capacity(self.params.len());
        for p in self.params {
            match heap.dup(&p.dictionary) {
                Ok(dictionary) => params.push(CliParam {
                    flag: p.flag,
                    label: p.label,
                    aliases: p.aliases,
                    help: p.help,
                    multi: p.multi,
                    dictionary,
                }),
                Err(err) => {
                    let mut roots = Vec::new();
                    for built in params {
                        roots.push(built.dictionary);
                    }
                    heap.drop_values(roots)?;
                    return Err(err);
                }
            }
        }
        let mut subcommands = Vec::with_capacity(self.subcommands.len());
        for (sub_shape, help) in self.subcommands {
            match sub_shape.materialize(heap) {
                Ok(sub) => subcommands.push((sub, help)),
                Err(err) => {
                    let mut roots = Vec::new();
                    for built in params {
                        roots.push(built.dictionary);
                    }
                    for (sub, _) in subcommands {
                        sub.drain_values(&mut roots);
                    }
                    heap.drop_values(roots)?;
                    return Err(err);
                }
            }
        }
        let handler = match self.handler {
            Some(rep) => match heap.dup(&rep) {
                Ok(h) => Some(h),
                Err(err) => {
                    let mut roots = Vec::new();
                    for built in params {
                        roots.push(built.dictionary);
                    }
                    for (sub, _) in subcommands {
                        sub.drain_values(&mut roots);
                    }
                    heap.drop_values(roots)?;
                    return Err(err);
                }
            },
            None => None,
        };
        Ok(CliSpec {
            name: self.name,
            description: self.description,
            params,
            subcommands,
            handler,
        })
    }
}

fn clone_spec_value(heap: &mut Heap, value: &Value, hooks: Fuel<'_>) -> Result<CliSpec, Error> {
    let (shape, cost) = {
        let spec = expect_spec(heap, value)?;
        SpecShape::from_spec(spec)
    };
    charge(hooks, cost)?;
    shape.materialize(heap)
}

fn drop_spec(heap: &mut Heap, spec: CliSpec) -> Result<(), Error> {
    let mut roots = Vec::new();
    spec.drain_values(&mut roots);
    heap.drop_values(roots)
}
