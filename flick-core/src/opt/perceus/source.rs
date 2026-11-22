//! Checked compiler IR to closure-converted, explicitly owned ANF.
//!
//! Both debug and optimized modules use this same entry point. Unsupported
//! forms are errors with their original source span, never a fallback to
//! the substitution model. Generated branch helpers are transparent code
//! boundaries, not source-function return boundaries.

mod app_fail;
mod core_lists;
mod core_lists_array;
mod core_lists_cb;
mod core_lists_ops;
mod core_records;
mod core_records_ops;
mod core_scalars;
mod core_strings;
mod derived;
mod dictionaries;
mod eff_combinators;
mod expr_builder;
mod failures;
mod foreign_bridge;
mod free;
mod handlers;
mod layers;
mod lazy_scope;
mod list_collections;
mod list_range;
mod list_show;
mod loops;
mod matches;
mod numeric_primitives;
mod option_map2;
mod records;
mod result_map2;
mod scalar_context;
mod seq;
mod std_arbitrary;
mod std_arbitrary_entries;
mod std_arbitrary_model;
mod std_arbitrary_node;
mod std_arbitrary_run;
mod std_arbitrary_schema;
mod std_async;
mod std_cache;
mod std_channel_control;
mod std_channel_core;
mod std_channel_run;
mod std_channel_text;
mod std_channel_transform;
mod std_check;
mod std_check_props;
mod std_chunk;
mod std_chunk_ops;
mod std_codec;
mod std_codec_meta;
mod std_codec_ops;
mod std_codec_std;
mod std_combine;
mod std_config;
mod std_config_prov;
mod std_crypto;
mod std_datetime_cron;
mod std_duration;
mod std_effect_coll;
mod std_effect_ops;
mod std_events_rep;
mod std_ext_rt;
mod std_functional;
mod std_graph;
mod std_graph_ops;
mod std_hash_map;
mod std_hash_map_ops;
mod std_hash_set;
mod std_json_decimal_regexp;
mod std_layer;
mod std_layer_map;
mod std_metric;
mod std_metric_ops;
mod std_observe_log;
mod std_observe_refs;
mod std_observe_span;
mod std_observe_trace;
mod std_outcome;
mod std_part_sem;
mod std_path;
mod std_pool;
mod std_pubsub;
mod std_queue;
mod std_rc;
mod std_request;
mod std_request_ops;
mod std_sched_plan;
mod std_schedule;
mod std_schedule_comb;
mod std_schedule_run;
mod std_schema;
mod std_schema_compound;
mod std_schema_enum;
mod std_schema_route;
mod std_schema_rt;
mod std_schema_rt_adt;
mod std_schema_rt_block;
mod std_schema_rt_gen;
mod std_schema_rt_gen_kind;
mod std_schema_rt_more;
mod std_schema_rt_obj;
mod std_schema_rt_route;
mod std_schema_rt_route_match;
mod std_schema_rt_route_try;
mod std_schema_rt_tree;
mod std_schema_rt_val;
mod std_scoped_cache;
mod std_scoped_res;
mod std_sink_core;
mod std_sink_folds;
mod std_sql;
mod std_stream_combine;
mod std_stream_core;
mod std_stream_errors;
mod std_stream_pipe;
mod std_stream_split;
mod std_stream_transform;
mod std_stream_window;
mod std_sync;
mod std_test_gen;
mod std_tx;
mod std_tx_coll;
mod std_tx_lock;
mod std_tx_queue;
mod strings;
mod structural;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::{error, fmt};

use flick_ir::{Constructor, Dict, Foreign, ForeignMode, InstanceInfo, Shape};

use crate::iface::Interface;
use crate::ir::memory::{
    self, Block, CodeId, Exit, Function, Global, HostPrimitive, Instruction, Intrinsic, Local,
    Name, Operation, Program,
};
use crate::ir::{Arm, BinOp, Expr, ExprKind, Item, ItemKind, Lit, Module, Param, Pat, Stage, Stmt};
use crate::source::Span;

use super::{Error as OwnershipError, insert};
use free::{free, given};

type Env = BTreeMap<String, Local>;

#[derive(Debug)]
pub enum LowerError {
    Declarations(String),
    Unsupported { form: String, span: Span },
    UnboundLocal { name: String, span: Span },
    UnknownGlobal { name: Name, span: Option<Span> },
    DuplicateGlobal(Name),
    Limit(Span),
    Ownership { code: CodeId, error: OwnershipError },
    Pattern { error: memory::Error, span: Span },
    Program(memory::Error),
}

impl fmt::Display for LowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Declarations(message) => {
                write!(f, "invalid counted declaration context: {message}")
            }
            Self::Unsupported { form, span } => {
                write!(f, "memory lowering does not yet support {form} at {span:?}")
            }
            Self::UnboundLocal { name, span } => {
                write!(f, "unbound memory source local {name} at {span:?}")
            }
            Self::UnknownGlobal { name, .. } => write!(
                f,
                "unknown memory source global {}.{}",
                name.module, name.name
            ),
            Self::DuplicateGlobal(name) => write!(
                f,
                "duplicate memory source global {}.{}",
                name.module, name.name
            ),
            Self::Limit(span) => write!(
                f,
                "memory lowering exceeds 2^32 code/local identities at {span:?}"
            ),
            Self::Ownership { code, error } => {
                write!(f, "ownership insertion in memory code {code}: {error}")
            }
            Self::Pattern { error, span } => {
                write!(f, "invalid memory source pattern at {span:?}: {error}")
            }
            Self::Program(error) => error.fmt(f),
        }
    }
}

impl error::Error for LowerError {}

fn unsupported(form: impl Into<String>, span: Span) -> LowerError {
    LowerError::Unsupported {
        form: form.into(),
        span,
    }
}

/// Lowers the entry-reachable checked definitions, then inserts ownership
/// operations in every generated function. Supply builtins as well as the
/// compiled modules, as the other reference-model entry point does.
/// This is an explicit memory-lowering API, not a change to JS/Go codegen.
pub fn lower<'a>(
    modules: impl IntoIterator<Item = &'a Module>,
    entry: &Name,
) -> Result<Program, LowerError> {
    lower_in(modules, &[entry], None, false, BTreeMap::new())
}

/// Adds matching checked compiler declaration facts, including private/open
/// representations. This is not validation of arbitrarily forged IR+interfaces.
/// Overloaded scalar bindings fail closed without these facts.
pub fn lower_with_declarations<'a>(
    modules: impl IntoIterator<Item = &'a Module>,
    declarations: impl IntoIterator<Item = &'a Interface>,
    entry: &Name,
) -> Result<Program, LowerError> {
    lower_roots_with_declarations(modules, declarations, &[entry])
}

/// [`lower_with_declarations`] of every definition one of `roots` reaches:
/// the entry and what a runner evaluates beside it, such as the layer of the
/// services it provides.
pub fn lower_roots_with_declarations<'a>(
    modules: impl IntoIterator<Item = &'a Module>,
    declarations: impl IntoIterator<Item = &'a Interface>,
    roots: &[&Name],
) -> Result<Program, LowerError> {
    let declarations =
        scalar_context::ScalarContext::new(declarations).map_err(LowerError::Declarations)?;
    lower_in(modules, roots, Some(declarations), false, BTreeMap::new())
}

/// Explicitly opts into lowering unadapted `foreign` declarations to
/// [`Operation::ForeignCall`] for callers that supply a counted host bridge.
/// `module_dirs` maps module names to their source directory for resolving
/// relative `./*.mjs` foreign modules.
pub fn lower_roots_with_foreign_bridge<'a>(
    modules: impl IntoIterator<Item = &'a Module>,
    declarations: impl IntoIterator<Item = &'a Interface>,
    roots: &[&Name],
    module_dirs: BTreeMap<String, String>,
) -> Result<Program, LowerError> {
    let declarations =
        scalar_context::ScalarContext::new(declarations).map_err(LowerError::Declarations)?;
    lower_in(modules, roots, Some(declarations), true, module_dirs)
}

fn lower_in<'a>(
    modules: impl IntoIterator<Item = &'a Module>,
    roots: &[&Name],
    declarations: Option<scalar_context::ScalarContext<'a>>,
    allow_foreign_bridge: bool,
    module_dirs: BTreeMap<String, String>,
) -> Result<Program, LowerError> {
    let mut sources = BTreeMap::new();
    for module in modules {
        for item in &module.items {
            let names = match &item.kind {
                ItemKind::Value { name, .. } => vec![name.as_str()],
                ItemKind::Foreign(value) => vec![value.name.as_str()],
                ItemKind::Destructure { pattern, .. } => {
                    let mut names = Vec::new();
                    pattern.binders(&mut names);
                    names
                }
                ItemKind::Instance { info, .. } => {
                    let name = Name::new(&info.module, &format!("instance {}", info.name));
                    if sources.insert(name.clone(), item).is_some() {
                        return Err(LowerError::DuplicateGlobal(name));
                    }
                    continue;
                }
                ItemKind::Constructor(_) | ItemKind::Service { .. } => continue,
            };
            for name in names {
                let name = Name::new(&module.name, name);
                if sources.insert(name.clone(), item).is_some() {
                    return Err(LowerError::DuplicateGlobal(name));
                }
            }
        }
    }
    let bridge_families = if allow_foreign_bridge {
        foreign_bridge::detect_families(&sources, roots)
    } else {
        foreign_bridge::BridgeFamilies::default()
    };
    let mut lower = Lower {
        declarations,
        allow_foreign_bridge,
        bridge_families,
        module_dirs,
        sources,
        initializers: BTreeMap::new(),
        program: Program::default(),
        next: 0,
        loop_counter: 0,
        iterable_loops: BTreeMap::new(),
        active_for_loop: None,
        in_for_each_body: BTreeSet::new(),
    };
    for root in roots {
        lower.global(root, None)?;
    }
    if allow_foreign_bridge {
        foreign_bridge::resolve_routes(&mut lower.program, lower.bridge_families);
    }
    memory::validate_program(&lower.program).map_err(LowerError::Program)?;
    Ok(lower.program)
}

struct Lower<'a> {
    declarations: Option<scalar_context::ScalarContext<'a>>,
    allow_foreign_bridge: bool,
    bridge_families: foreign_bridge::BridgeFamilies,
    module_dirs: BTreeMap<String, String>,
    sources: BTreeMap<Name, &'a Item>,
    initializers: BTreeMap<Name, CodeId>,
    program: Program,
    next: CodeId,
    loop_counter: usize,
    iterable_loops: BTreeMap<String, String>,
    active_for_loop: Option<String>,
    in_for_each_body: BTreeSet<String>,
}

/// One ANF function under construction. Branches get their own builders,
/// never copies of the caller's continuation.
struct Code {
    params: Vec<Local>,
    next: Local,
    instructions: Vec<Instruction>,
}

impl Code {
    fn new(params: Vec<Local>) -> Self {
        let next = params
            .iter()
            .copied()
            .max()
            .map_or(0, |last| last.saturating_add(1));
        Self {
            params,
            next,
            instructions: Vec::new(),
        }
    }

    fn emit(&mut self, value: Operation, span: Span) -> Result<Local, LowerError> {
        let binding = self.next;
        self.next = self.next.checked_add(1).ok_or(LowerError::Limit(span))?;
        self.instructions.push(Instruction::Let { binding, value });
        Ok(binding)
    }

    fn finish(self, value: Local) -> Function {
        Function {
            params: self.params,
            body: Block {
                instructions: self.instructions,
                exit: Exit::Return(value),
            },
        }
    }

    /// A successor inherits the allocated local namespace, but not the
    /// instructions or continuation of its predecessor.
    fn successor(&self) -> Self {
        Self {
            params: self.params.clone(),
            next: self.next,
            instructions: vec![],
        }
    }
}

include!("source/global_lowering.rs");
include!("source/expression_lowering.rs");
include!("source/branch_lowering.rs");

fn foreign_intrinsic(value: &Foreign) -> Option<Intrinsic> {
    use Intrinsic::Host;
    match (
        value.binding.from.as_deref(),
        value.binding.name.as_str(),
        value.mode,
    ) {
        (Some("flick:rt"), "stdoutLine", ForeignMode::Sync) => Some(Intrinsic::PrintLine),
        (Some("flick:rt"), "stderrLine", ForeignMode::Sync) => {
            Some(Host(HostPrimitive::StderrLine))
        }
        (Some("flick:rt"), "stdoutWrite", ForeignMode::Sync) => {
            Some(Host(HostPrimitive::StdoutWrite))
        }
        (Some("flick:rt"), "platformEnv", ForeignMode::Sync) => Some(Host(HostPrimitive::Env)),
        (Some("flick:rt"), "platformVars", ForeignMode::Sync) => Some(Host(HostPrimitive::Vars)),
        (Some("flick:rt"), "platformArgs", ForeignMode::Sync) => Some(Host(HostPrimitive::Args)),
        (Some("flick:Clock"), "flickCurrentTimeMillis", ForeignMode::Effect) => {
            Some(Host(HostPrimitive::CurrentTimeMillis))
        }
        (Some("flick:Clock"), "flickCurrentTimeNanos", ForeignMode::Effect) => {
            Some(Host(HostPrimitive::CurrentTimeNanos))
        }
        (Some("flick:Clock"), "flickMonotonicTimeNanos", ForeignMode::Effect) => {
            Some(Host(HostPrimitive::MonotonicTimeNanos))
        }
        (Some("flick:Random"), "flickNext", ForeignMode::Effect) => {
            Some(Host(HostPrimitive::RandomNext))
        }
        (Some("flick:rt"), "processSpawn", ForeignMode::Effect) => {
            Some(Host(HostPrimitive::ProcessSpawn))
        }
        (Some("flick:rt"), "show", ForeignMode::Pure) => Some(Intrinsic::Show),
        (Some("flick:rt"), "refMake", ForeignMode::Sync) => Some(Intrinsic::RefMake),
        (Some("flick:rt"), "refGet", ForeignMode::Sync) => Some(Intrinsic::RefGet),
        (Some("flick:rt"), "refSet", ForeignMode::Sync) => Some(Intrinsic::RefSet),
        (Some("flick:rt"), "refModify", ForeignMode::Sync) => Some(Intrinsic::RefModify),
        (Some("flick:rt"), "refSame", ForeignMode::Pure) => Some(Intrinsic::RefSame),
        (Some("flick:rt"), "scopeRun", ForeignMode::Effect) => Some(Intrinsic::ScopeRun),
        (Some("flick:rt"), "scopeCurrent", ForeignMode::Effect) => Some(Intrinsic::ScopeCurrent),
        (Some("flick:rt"), "arenaMake", ForeignMode::Effect) => Some(Intrinsic::ArenaMake),
        (Some("flick:rt"), "arenaAlloc", ForeignMode::Sync) => Some(Intrinsic::ArenaAlloc),
        (Some("flick:rt"), "arenaGet", ForeignMode::Sync) => Some(Intrinsic::ArenaGet),
        _ => None,
    }
}

fn lookup(env: &Env, name: &str, span: Span) -> Result<Local, LowerError> {
    env.get(name)
        .copied()
        .ok_or_else(|| LowerError::UnboundLocal {
            name: name.into(),
            span,
        })
}

fn abstract_params(names: &[String]) -> Vec<Param> {
    names
        .iter()
        .map(|name| Param {
            pat: Pat::Bind(given(name)),
            stage: Stage::Pure,
            ty: None,
            binder_types: Vec::new(),
        })
        .collect()
}

fn dictionary(value: &Dict, span: Span) -> Result<Expr, LowerError> {
    let kind = match value {
        Dict::Given(name) => ExprKind::Local(given(name)),
        Dict::Field(name) => ExprKind::Local(name.clone()),
        Dict::Opened { binder, field } => ExprKind::Field(
            Box::new(Expr::new(ExprKind::Local(binder.clone()), span)),
            field.clone(),
        ),
        Dict::Value { module, name } => ExprKind::Global {
            module: module.clone(),
            name: name.clone(),
            type_args: Vec::new(),
        },
        Dict::Instance { module, name, args } => {
            let instance = Expr::new(
                ExprKind::Global {
                    module: module.clone(),
                    name: format!("instance {name}"),
                    type_args: Vec::new(),
                },
                span,
            );
            ExprKind::Dicts(Box::new(instance), args.clone())
        }
        Dict::Super { dict, class } => ExprKind::Method {
            dict: (**dict).clone(),
            name: format!("${class}"),
            type_args: Vec::new(),
        },
        Dict::Site { place, definition } => ExprKind::Dictionary(vec![
            (
                "definition".into(),
                Expr::new(ExprKind::Lit(Lit::String(definition.clone())), span),
            ),
            (
                "place".into(),
                Expr::new(ExprKind::Lit(Lit::String(place.clone())), span),
            ),
        ]),
        Dict::Primitive { labels, .. } => ExprKind::List(
            labels
                .iter()
                .map(|label| Expr::new(ExprKind::Lit(Lit::String(label.clone())), span))
                .collect(),
        ),
        Dict::Structural { .. } => {
            // A dictionary used as a value (for example a superclass) keeps
            // the ordinary dictionary-application ABI. The direct lowering
            // above constructs its implementation, not this identity closure.
            let name = "$memory$dictionary";
            let identity = Expr::new(
                ExprKind::Lambda {
                    params: vec![Param {
                        pat: Pat::Bind(name.into()),
                        stage: Stage::Pure,
                        ty: None,
                        binder_types: Vec::new(),
                    }],
                    body: Box::new(Expr::new(ExprKind::Local(name.into()), span)),
                },
                span,
            );
            ExprKind::Dicts(Box::new(identity), vec![value.clone()])
        }
        Dict::Error => {
            return Err(unsupported("error dictionary", span));
        }
    };
    Ok(Expr::new(kind, span))
}
