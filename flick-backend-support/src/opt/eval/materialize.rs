//! Fallible value-to-IR materialization and call-site profitability decision.
//!
//! Evaluation, materialization, and profitability are kept separate:
//! - a value may be valid inside the evaluator (closure, dictionary, lone-surrogate
//!   UTF-16 string, opaque external constructor) yet decline materialization;
//! - a materialized IR tree may still be declined when it exceeds residual node
//!   or byte growth budgets compared to the original call.

use std::collections::HashSet;
use std::rc::Rc;

use flick_ir::{Constructor, Expr, ExprKind, Lang, Lit, Pat, Span, Stage, Stmt};

use super::value::{FIELD_BYTES, SLOT_BYTES, Value};
use super::{EvalConfig, StopReason};
use crate::Known;

/// Result of successfully materializing a [`Value`] into portable Flick IR.
#[derive(Debug, Clone)]
pub struct Materialized {
    pub expr: Expr,
    pub nodes: u32,
    pub bytes: u32,
}

/// Limits enforced before allocating each node/payload during materialization.
#[derive(Debug, Clone, Copy)]
pub struct MaterializeLimits {
    pub max_nodes: u32,
    pub max_bytes: u32,
    pub max_depth: u32,
}

impl MaterializeLimits {
    pub fn from_config(config: &EvalConfig) -> Self {
        MaterializeLimits {
            max_nodes: config.max_materialized_nodes,
            max_bytes: config.max_materialized_bytes,
            max_depth: config.max_frame_depth,
        }
    }
}

/// Context telling [`materialize`] which external constructors may be emitted
/// directly in the target module without introducing unexported imports.
pub struct MaterializeContext<'a> {
    pub current_module: &'a str,
    pub lang: &'a Known<'a>,
    /// External constructors already referenced in `current_module`: `(module, ty, name)`.
    pub visible_external_ctors: &'a HashSet<(String, String, String)>,
}

impl MaterializeContext<'_> {
    fn can_emit_ctor(&self, ctor: &Constructor) -> bool {
        if ctor.module == self.current_module {
            return true;
        }
        if self.lang.is(Lang::Some, &ctor.module, &ctor.name)
            || self.lang.is(Lang::None, &ctor.module, &ctor.name)
            || self.lang.is(Lang::Ok, &ctor.module, &ctor.name)
            || self.lang.is(Lang::Err, &ctor.module, &ctor.name)
        {
            return true;
        }
        self.visible_external_ctors
            .iter()
            .any(|(m, t, n)| m == &ctor.module && t == &ctor.ty && n == &ctor.name)
    }
}

enum MatFrame<'a> {
    Tuple {
        done: Vec<Expr>,
        rest: &'a [Value<'a>],
        index: usize,
    },
    List {
        done: Vec<Expr>,
        rest: &'a [Value<'a>],
        index: usize,
    },
    Record {
        done: Vec<(String, Expr)>,
        rest: &'a [(Rc<str>, Value<'a>)],
        index: usize,
    },
    Con {
        ctor: Rc<Constructor>,
        done: Vec<Expr>,
        rest: &'a [Value<'a>],
        index: usize,
    },
}

/// Fallibly materializes `value` into portable IR at `span` using an explicit
/// iterative stack (safe on native and browser WASM).
#[allow(
    clippy::too_many_lines,
    reason = "iterative value-to-IR materialization loop"
)]
pub fn materialize(
    value: &Value<'_>,
    span: Span,
    ctx: &MaterializeContext<'_>,
    limits: MaterializeLimits,
) -> Result<Materialized, StopReason> {
    let mut nodes: u32 = 0;
    let mut bytes: u32 = 0;
    let mut frames: Vec<MatFrame<'_>> = Vec::new();
    let mut current: Option<&Value<'_>> = Some(value);
    let mut ready: Option<Expr> = None;

    let charge = |nodes: &mut u32,
                  bytes: &mut u32,
                  add_nodes: u32,
                  add_bytes: u32|
     -> Result<(), StopReason> {
        let next_nodes = nodes.saturating_add(add_nodes);
        let next_bytes = bytes.saturating_add(add_bytes);
        if next_nodes > limits.max_nodes || next_bytes > limits.max_bytes {
            return Err(StopReason::ResidualGrowth);
        }
        *nodes = next_nodes;
        *bytes = next_bytes;
        Ok(())
    };

    loop {
        if let Some(val) = current.take() {
            match val {
                Value::Unit => {
                    charge(&mut nodes, &mut bytes, 1, 0)?;
                    ready = Some(Expr::new(ExprKind::Lit(Lit::Unit), span));
                }
                Value::Bool(b) => {
                    charge(&mut nodes, &mut bytes, 1, 1)?;
                    ready = Some(Expr::new(ExprKind::Lit(Lit::Bool(*b)), span));
                }
                Value::Int(n) => {
                    charge(&mut nodes, &mut bytes, 1, 4)?;
                    ready = Some(Expr::new(ExprKind::Lit(Lit::Int(*n)), span));
                }
                Value::Float(x) => {
                    charge(&mut nodes, &mut bytes, 1, 8)?;
                    ready = Some(Expr::new(ExprKind::Lit(Lit::Float(*x)), span));
                }
                Value::BigInt(n) => {
                    let est = bigint_byte_len(*n);
                    charge(&mut nodes, &mut bytes, 1, est)?;
                    ready = Some(Expr::new(ExprKind::Lit(Lit::BigInt(n.to_string())), span));
                }
                Value::String(s) => {
                    let len = u32::try_from(s.len()).unwrap_or(u32::MAX);
                    charge(&mut nodes, &mut bytes, 1, len)?;
                    ready = Some(Expr::new(ExprKind::Lit(Lit::String(s.to_string())), span));
                }
                Value::Utf16(units) => {
                    // Validate UTF-16 -> UTF-8 before charging residual growth so
                    // lone surrogates always report `Unmaterializable`.
                    let decoded =
                        String::from_utf16(units).map_err(|_| StopReason::Unmaterializable)?;
                    let len = u32::try_from(decoded.len()).unwrap_or(u32::MAX);
                    charge(&mut nodes, &mut bytes, 1, len)?;
                    ready = Some(Expr::new(ExprKind::Lit(Lit::String(decoded)), span));
                }
                Value::Tuple(items) => {
                    let len = u32::try_from(items.len()).unwrap_or(u32::MAX);
                    charge(&mut nodes, &mut bytes, 1, len.saturating_mul(SLOT_BYTES))?;
                    if items.is_empty() {
                        ready = Some(Expr::new(ExprKind::Tuple(Vec::new()), span));
                    } else {
                        if u32::try_from(frames.len()).unwrap_or(u32::MAX) >= limits.max_depth {
                            return Err(StopReason::FrameDepth);
                        }
                        frames.push(MatFrame::Tuple {
                            done: Vec::with_capacity(items.len()),
                            rest: items,
                            index: 1,
                        });
                        current = Some(&items[0]);
                    }
                }
                Value::List(items) => {
                    let len = u32::try_from(items.len()).unwrap_or(u32::MAX);
                    charge(&mut nodes, &mut bytes, 1, len.saturating_mul(SLOT_BYTES))?;
                    if items.is_empty() {
                        ready = Some(Expr::new(ExprKind::List(Vec::new()), span));
                    } else {
                        if u32::try_from(frames.len()).unwrap_or(u32::MAX) >= limits.max_depth {
                            return Err(StopReason::FrameDepth);
                        }
                        frames.push(MatFrame::List {
                            done: Vec::with_capacity(items.len()),
                            rest: items,
                            index: 1,
                        });
                        current = Some(&items[0]);
                    }
                }
                Value::Record(fields) => {
                    let len = u32::try_from(fields.len()).unwrap_or(u32::MAX);
                    let mut field_bytes = len.saturating_mul(FIELD_BYTES);
                    for (label, _) in fields.iter() {
                        let label_len = u32::try_from(label.len()).unwrap_or(u32::MAX);
                        field_bytes = field_bytes.saturating_add(label_len);
                    }
                    charge(&mut nodes, &mut bytes, 1, field_bytes)?;
                    if fields.is_empty() {
                        ready = Some(Expr::new(ExprKind::Record(Vec::new()), span));
                    } else {
                        if u32::try_from(frames.len()).unwrap_or(u32::MAX) >= limits.max_depth {
                            return Err(StopReason::FrameDepth);
                        }
                        frames.push(MatFrame::Record {
                            done: Vec::with_capacity(fields.len()),
                            rest: fields,
                            index: 1,
                        });
                        current = Some(&fields[0].1);
                    }
                }
                Value::Con { ctor, args, dicts } => {
                    if args.len() != ctor.arity
                        || !dicts.is_empty()
                        || ctor.dicts != 0
                        || !ctx.can_emit_ctor(ctor)
                    {
                        return Err(StopReason::Unmaterializable);
                    }
                    let arity = u32::try_from(args.len()).unwrap_or(u32::MAX);
                    // One `Con` node plus one `App` node per argument.
                    charge(
                        &mut nodes,
                        &mut bytes,
                        1_u32.saturating_add(arity),
                        arity.saturating_mul(SLOT_BYTES),
                    )?;
                    if args.is_empty() {
                        ready = Some(Expr::new(ExprKind::Con((**ctor).clone()), span));
                    } else {
                        if u32::try_from(frames.len()).unwrap_or(u32::MAX) >= limits.max_depth {
                            return Err(StopReason::FrameDepth);
                        }
                        frames.push(MatFrame::Con {
                            ctor: Rc::clone(ctor),
                            done: Vec::with_capacity(args.len()),
                            rest: args,
                            index: 1,
                        });
                        current = Some(&args[0]);
                    }
                }
                Value::Closure(_)
                | Value::Dict(_)
                | Value::Method { .. }
                | Value::Primitive { .. }
                | Value::OpaqueGlobal { .. } => {
                    return Err(StopReason::Unmaterializable);
                }
            }
            continue;
        }

        let Some(expr) = ready.take() else {
            return Err(StopReason::UnsafeContext);
        };
        let Some(frame) = frames.pop() else {
            return Ok(Materialized { expr, nodes, bytes });
        };
        match frame {
            MatFrame::Tuple {
                mut done,
                rest,
                index,
            } => {
                done.push(expr);
                if index < rest.len() {
                    current = Some(&rest[index]);
                    frames.push(MatFrame::Tuple {
                        done,
                        rest,
                        index: index + 1,
                    });
                } else {
                    ready = Some(Expr::new(ExprKind::Tuple(done), span));
                }
            }
            MatFrame::List {
                mut done,
                rest,
                index,
            } => {
                done.push(expr);
                if index < rest.len() {
                    current = Some(&rest[index]);
                    frames.push(MatFrame::List {
                        done,
                        rest,
                        index: index + 1,
                    });
                } else {
                    ready = Some(Expr::new(ExprKind::List(done), span));
                }
            }
            MatFrame::Record {
                mut done,
                rest,
                index,
            } => {
                let label = rest[index - 1].0.to_string();
                done.push((label, expr));
                if index < rest.len() {
                    current = Some(&rest[index].1);
                    frames.push(MatFrame::Record {
                        done,
                        rest,
                        index: index + 1,
                    });
                } else {
                    ready = Some(Expr::new(ExprKind::Record(done), span));
                }
            }
            MatFrame::Con {
                ctor,
                mut done,
                rest,
                index,
            } => {
                done.push(expr);
                if index < rest.len() {
                    current = Some(&rest[index]);
                    frames.push(MatFrame::Con {
                        ctor,
                        done,
                        rest,
                        index: index + 1,
                    });
                } else {
                    let mut built = Expr::new(ExprKind::Con((*ctor).clone()), span);
                    for arg in done {
                        built = Expr::new(
                            ExprKind::App {
                                func: Box::new(built),
                                arg: Box::new(arg),
                                stage: Stage::Pure,
                                arg_first: false,
                            },
                            span,
                        );
                    }
                    ready = Some(built);
                }
            }
        }
    }
}

/// Estimates decimal string length of an `i128` without allocating.
fn bigint_byte_len(n: i128) -> u32 {
    if n == 0 {
        return 1;
    }
    let sign = u32::from(n < 0);
    let mut mag = n.unsigned_abs();
    let mut digits = 0_u32;
    while mag > 0 {
        mag /= 10;
        digits += 1;
    }
    sign + digits
}

/// Cost summary of an existing IR expression, computed iteratively (no host recursion).
#[derive(Debug, Clone, Copy)]
pub struct ExprCost {
    pub nodes: u32,
    pub bytes: u32,
}

/// Iteratively computes node count and literal payload bytes of `expr`.
pub fn expr_cost(expr: &Expr) -> ExprCost {
    let mut nodes: u32 = 0;
    let mut bytes: u32 = 0;
    let mut stack: Vec<&Expr> = vec![expr];
    while let Some(e) = stack.pop() {
        nodes = nodes.saturating_add(1);
        match &e.kind {
            ExprKind::Lit(Lit::String(s) | Lit::BigInt(s)) => {
                bytes = bytes.saturating_add(u32::try_from(s.len()).unwrap_or(u32::MAX));
            }
            ExprKind::Lit(Lit::Float(_)) => bytes = bytes.saturating_add(8),
            ExprKind::Lit(Lit::Int(_)) => bytes = bytes.saturating_add(4),
            ExprKind::Lit(Lit::Bool(_)) => bytes = bytes.saturating_add(1),
            ExprKind::Match(_, arms) => {
                for arm in arms {
                    nodes = nodes.saturating_add(pat_nodes(&arm.pat));
                }
            }
            ExprKind::Handle(h) => {
                for arm in &h.errors {
                    nodes = nodes.saturating_add(pat_nodes(&arm.pat));
                }
            }
            ExprKind::Each { pat, .. } => {
                nodes = nodes.saturating_add(pat_nodes(pat));
            }
            ExprKind::Block(stmts, _) => {
                for stmt in stmts {
                    if let Stmt::Let(pat, _) = stmt {
                        nodes = nodes.saturating_add(pat_nodes(pat));
                    }
                }
            }
            _ => {}
        }
        e.each_child(&mut |child| stack.push(child));
    }
    ExprCost { nodes, bytes }
}

fn pat_nodes(root: &Pat) -> u32 {
    let mut nodes: u32 = 0;
    let mut stack: Vec<&Pat> = vec![root];
    while let Some(pat) = stack.pop() {
        match pat {
            Pat::Wild | Pat::Bind(_) => {}
            Pat::Lit(_) => nodes = nodes.saturating_add(1),
            Pat::Tuple(items) | Pat::Con(_, items) | Pat::Or(items) => {
                nodes = nodes.saturating_add(1);
                for item in items {
                    stack.push(item);
                }
            }
            Pat::List(items, rest) => {
                nodes = nodes.saturating_add(1);
                for item in items {
                    stack.push(item);
                }
                if let Some(rest) = rest {
                    stack.push(rest);
                }
            }
            Pat::Record(fields) => {
                nodes = nodes.saturating_add(1);
                for (_, item) in fields {
                    stack.push(item);
                }
            }
        }
    }
    nodes
}

/// Decides whether replacing `orig` with `mat` is profitable under `config`,
/// crediting `removed_def_nodes` when eliminating the call also removes a
/// single-use private top-level definition.
pub fn check_profitability(
    orig: ExprCost,
    mat: &Materialized,
    removed_def_nodes: u32,
    config: &EvalConfig,
) -> Result<(), StopReason> {
    let allowed_nodes = orig
        .nodes
        .saturating_add(removed_def_nodes.min(config.max_materialized_nodes))
        .saturating_add(config.max_residual_node_growth);
    let allowed_bytes = orig.bytes.saturating_add(config.max_residual_byte_growth);
    if mat.nodes > allowed_nodes
        || mat.nodes > config.max_materialized_nodes
        || mat.bytes > allowed_bytes
        || mat.bytes > config.max_materialized_bytes
    {
        return Err(StopReason::ResidualGrowth);
    }
    Ok(())
}
