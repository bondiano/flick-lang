//! Fallible value-to-IR materialization and call-site profitability decision.
//!
//! Evaluation, materialization, and profitability are kept separate:
//! - a value may be valid inside the evaluator (closure, dictionary, lone-surrogate
//!   UTF-16 string, opaque external constructor) yet decline materialization;
//! - a materialized IR tree may still be declined when it exceeds residual node
//!   or byte growth budgets compared to the original call.

use std::collections::HashSet;
use std::rc::Rc;

use flick_ir::{Constructor, Expr, ExprKind, Lang, Lit, Pat, Span, Stage};

use super::defs::join_nul;
use super::value::{FIELD_BYTES, SLOT_BYTES, Value, bigint_byte_len, each_expr_pat, walk_pat};
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

/// Encodes an external constructor identity `(module, ty, name)` into a compact
/// NUL-delimited key so visibility checks reuse `HashSet<String>`.
pub(super) fn ctor_key(module: &str, ty: &str, name: &str) -> String {
    join_nul(&[module, ty, name])
}

/// Context telling [`materialize`] which external constructors may be emitted
/// directly in the target module without introducing unexported imports.
pub struct MaterializeContext<'a> {
    pub current_module: &'a str,
    pub lang: &'a Known<'a>,
    /// External constructors already referenced in `current_module`, encoded by [`ctor_key`].
    pub visible_external_ctors: &'a HashSet<String>,
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
            .contains(ctor_key(&ctor.module, &ctor.ty, &ctor.name).as_str())
    }
}

enum MatSeqKind {
    Tuple,
    List,
    Con(Rc<Constructor>),
}

enum MatFrame<'a> {
    Seq {
        kind: MatSeqKind,
        done: Vec<Expr>,
        rest: &'a [Value<'a>],
    },
    Record {
        label: &'a str,
        done: Vec<(String, Expr)>,
        rest: &'a [(Rc<str>, Value<'a>)],
    },
}

struct MatAcc {
    nodes: u32,
    bytes: u32,
    limits: MaterializeLimits,
}

impl MatAcc {
    #[inline(never)]
    fn charge(&mut self, add_nodes: u32, add_bytes: u32) -> Result<(), StopReason> {
        let next_nodes = self.nodes.saturating_add(add_nodes);
        let next_bytes = self.bytes.saturating_add(add_bytes);
        if next_nodes > self.limits.max_nodes || next_bytes > self.limits.max_bytes {
            return Err(StopReason::ResidualGrowth);
        }
        self.nodes = next_nodes;
        self.bytes = next_bytes;
        Ok(())
    }

    #[inline(never)]
    fn push_frame<'a>(
        &self,
        frames: &mut Vec<MatFrame<'a>>,
        frame: MatFrame<'a>,
    ) -> Result<(), StopReason> {
        if u32::try_from(frames.len()).unwrap_or(u32::MAX) >= self.limits.max_depth {
            return Err(StopReason::FrameDepth);
        }
        frames.push(frame);
        Ok(())
    }
}

fn finish_seq(kind: MatSeqKind, done: Vec<Expr>, span: Span) -> Expr {
    match kind {
        MatSeqKind::Tuple => Expr::new(ExprKind::Tuple(done), span),
        MatSeqKind::List => Expr::new(ExprKind::List(done), span),
        MatSeqKind::Con(ctor) => {
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
            built
        }
    }
}

fn materialize_scalar(val: &Value<'_>, acc: &mut MatAcc) -> Result<Option<Lit>, StopReason> {
    let lit = match val {
        Value::Unit => {
            acc.charge(1, 0)?;
            Lit::Unit
        }
        Value::Bool(b) => {
            acc.charge(1, 1)?;
            Lit::Bool(*b)
        }
        Value::Int(n) => {
            acc.charge(1, 4)?;
            Lit::Int(*n)
        }
        Value::Float(x) => {
            acc.charge(1, 8)?;
            Lit::Float(*x)
        }
        Value::BigInt(n) => {
            acc.charge(1, bigint_byte_len(*n))?;
            Lit::BigInt(n.to_string())
        }
        Value::String(s) => {
            acc.charge(1, u32::try_from(s.len()).unwrap_or(u32::MAX))?;
            Lit::String((**s).to_owned())
        }
        Value::Utf16(units) => {
            // Validate UTF-16 and sum exact UTF-8 byte length without
            // allocating, preserving `Unmaterializable` for lone surrogates
            // while enforcing `max_nodes`/`max_bytes` before allocation.
            let utf8_len = decode_utf16(units, None)?;
            acc.charge(1, utf8_len)?;
            let mut decoded = String::with_capacity(utf8_len as usize);
            let _ = decode_utf16(units, Some(&mut decoded))?;
            Lit::String(decoded)
        }
        _ => return Ok(None),
    };
    Ok(Some(lit))
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
    let mut acc = MatAcc {
        nodes: 0,
        bytes: 0,
        limits,
    };
    let mut frames: Vec<MatFrame<'_>> = Vec::new();
    let mut current: Option<&Value<'_>> = Some(value);
    let mut ready: Option<Expr> = None;

    loop {
        if let Some(val) = current.take() {
            if let Some(lit) = materialize_scalar(val, &mut acc)? {
                ready = Some(Expr::new(ExprKind::Lit(lit), span));
                continue;
            }
            let seq = match val {
                Value::Tuple(items) => Some((MatSeqKind::Tuple, items.as_slice(), 1_u32)),
                Value::List(items) => Some((MatSeqKind::List, items.as_slice(), 1_u32)),
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
                    Some((
                        MatSeqKind::Con(Rc::clone(ctor)),
                        args.as_slice(),
                        1_u32.saturating_add(arity),
                    ))
                }
                Value::Record(fields) => {
                    let len = u32::try_from(fields.len()).unwrap_or(u32::MAX);
                    let mut field_bytes = len.saturating_mul(FIELD_BYTES);
                    for (label, _) in fields.iter() {
                        field_bytes = field_bytes
                            .saturating_add(u32::try_from(label.len()).unwrap_or(u32::MAX));
                    }
                    acc.charge(1, field_bytes)?;
                    if let Some(((label, first), rest)) = fields.split_first() {
                        acc.push_frame(
                            &mut frames,
                            MatFrame::Record {
                                label,
                                done: Vec::new(),
                                rest,
                            },
                        )?;
                        current = Some(first);
                    } else {
                        ready = Some(Expr::new(ExprKind::Record(Vec::new()), span));
                    }
                    None
                }
                _ => return Err(StopReason::Unmaterializable),
            };
            if let Some((kind, items, add_nodes)) = seq {
                let len = u32::try_from(items.len()).unwrap_or(u32::MAX);
                acc.charge(add_nodes, len.saturating_mul(SLOT_BYTES))?;
                if let Some((first, rest)) = items.split_first() {
                    acc.push_frame(
                        &mut frames,
                        MatFrame::Seq {
                            kind,
                            done: Vec::new(),
                            rest,
                        },
                    )?;
                    current = Some(first);
                } else {
                    ready = Some(finish_seq(kind, Vec::new(), span));
                }
            }
            continue;
        }

        let Some(expr) = ready.take() else {
            return Err(StopReason::UnsafeContext);
        };
        let Some(frame) = frames.pop() else {
            return Ok(Materialized {
                expr,
                nodes: acc.nodes,
                bytes: acc.bytes,
            });
        };
        match frame {
            MatFrame::Seq {
                kind,
                mut done,
                rest,
            } => {
                done.push(expr);
                if let Some((next, tail)) = rest.split_first() {
                    current = Some(next);
                    frames.push(MatFrame::Seq {
                        kind,
                        done,
                        rest: tail,
                    });
                } else {
                    ready = Some(finish_seq(kind, done, span));
                }
            }
            MatFrame::Record {
                label,
                mut done,
                rest,
            } => {
                done.push((label.to_owned(), expr));
                if let Some(((next_label, next_val), tail)) = rest.split_first() {
                    current = Some(next_val);
                    frames.push(MatFrame::Record {
                        label: next_label,
                        done,
                        rest: tail,
                    });
                } else {
                    ready = Some(Expr::new(ExprKind::Record(done), span));
                }
            }
        }
    }
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
            _ => {}
        }
        each_expr_pat(e, false, &mut |pat| {
            nodes = nodes.saturating_add(pat_nodes(pat));
        });
        e.each_child(&mut |child| stack.push(child));
    }
    ExprCost { nodes, bytes }
}

/// Validates UTF-16 code units, computes their exact UTF-8 byte length, and
/// optionally appends decoded characters into `out` (avoiding `String::from_utf16`
/// stdlib iterator monomorphization in browser WASM).
fn decode_utf16(units: &[u16], mut out: Option<&mut String>) -> Result<u32, StopReason> {
    let mut len = 0_u32;
    let mut i = 0;
    while i < units.len() {
        let u = units[i];
        let cp = if (0xD800..=0xDBFF).contains(&u) {
            let Some(&low) = units.get(i + 1) else {
                return Err(StopReason::Unmaterializable);
            };
            if !(0xDC00..=0xDFFF).contains(&low) {
                return Err(StopReason::Unmaterializable);
            }
            i += 2;
            0x10000 + ((u32::from(u - 0xD800) << 10) | u32::from(low - 0xDC00))
        } else if (0xDC00..=0xDFFF).contains(&u) {
            return Err(StopReason::Unmaterializable);
        } else {
            i += 1;
            u32::from(u)
        };
        let Some(ch) = char::from_u32(cp) else {
            return Err(StopReason::Unmaterializable);
        };
        len = len.saturating_add(u32::try_from(ch.len_utf8()).unwrap_or(4));
        if let Some(buf) = out.as_deref_mut() {
            buf.push(ch);
        }
    }
    Ok(len)
}

fn pat_nodes(root: &Pat) -> u32 {
    let mut nodes: u32 = 0;
    walk_pat(root, &mut |pat| {
        if !matches!(pat, Pat::Wild | Pat::Bind(_)) {
            nodes = nodes.saturating_add(1);
        }
    });
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
