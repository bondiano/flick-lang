//! Explicit opt-in lowering of foreign bindings to [`Operation::ForeignCall`].
//!
//! Only reachable through [`super::lower_roots_with_foreign_bridge`]; the
//! closed-world entry points (`lower`, `lower_with_declarations`,
//! `lower_roots_with_declarations`) keep `allow_foreign_bridge == false` and
//! continue to reject arbitrary foreign bindings at lowering time.

use std::collections::{BTreeMap, BTreeSet};

use flick_ir::memory::{Block, Exit, ForeignCallSpec, Instruction, Operation, Program, WireHint};
use flick_ir::{Dict, Expr, ExprKind, Foreign, ForeignMode, Item, ItemKind};

use crate::codegen::EffectBridge;
use crate::effects::{
    bindings, effect_specifier, on_kernel_with_bridge, owned, rt_specifier, selected_on_kernel,
    ui_family, web_family,
};

use super::{Code, Env, Local, Lower, LowerError, Name, Span};

/// Optional single-owner Effect-TS family overrides for bridged executions.
///
/// Closed-world lowering (`allow_foreign_bridge == false`) always keeps every
/// flag `false`. When a bridged program uses host Effect-TS consumers/producers
/// for observability, config, streams, or queues, the matching family routes
/// uniformly to `@flick-lang/effect-ts/*` instead of splitting one execution
/// across disconnected Rust-local and Effect-TS runtimes.
#[expect(
    clippy::struct_excessive_bools,
    reason = "five independent single-owner Effect-TS family override flags"
)]
#[derive(Default, Clone, Copy)]
pub(super) struct BridgeFamilies {
    pub observe: bool,
    pub config: bool,
    pub stream: bool,
    pub queue: bool,
    pub schedule: bool,
}

pub(super) fn detect_families(sources: &BTreeMap<Name, &Item>, roots: &[&Name]) -> BridgeFamilies {
    let mut visited = BTreeSet::new();
    let mut stack: Vec<Name> = roots.iter().map(|n| (*n).clone()).collect();
    let mut families = BridgeFamilies::default();
    while let Some(name) = stack.pop() {
        if !visited.insert(name.clone()) {
            continue;
        }
        let Some(item) = sources.get(&name) else {
            continue;
        };
        match &item.kind {
            ItemKind::Foreign(f) => {
                if let Some(from) = f.binding.from.as_deref() {
                    mark_family_triggers(&mut families, from, &f.binding.name);
                }
                if let Some(catch_expr) = &f.catch {
                    collect_expr_globals(catch_expr, &mut stack);
                }
            }
            ItemKind::Value { body, .. } | ItemKind::Destructure { body, .. } => {
                collect_expr_globals(body, &mut stack);
            }
            ItemKind::Instance { methods, .. } => {
                for (_, expr) in methods {
                    collect_expr_globals(expr, &mut stack);
                }
            }
            ItemKind::Constructor(_) | ItemKind::Service { .. } => {}
        }
    }
    families
}

fn mark_family_triggers(f: &mut BridgeFamilies, from: &str, name: &str) {
    if from.starts_with("flick:http/")
        || from.starts_with("flick:http-api/")
        || from.starts_with("flick:rpc/")
        || matches!(
            from,
            "flick:observability/Otlp"
                | "flick:devtools/DevTools"
                | "flick:devtools/DevToolsNode"
                | "flick:cli"
                | "flick:ui/tracing"
        )
        || (from == "flick:Logger" && name == "flickToFile")
    {
        f.observe = true;
    }
    if matches!(
        from,
        "flick:cli"
            | "flick:observability/Otlp"
            | "flick:devtools/DevTools"
            | "flick:devtools/DevToolsNode"
    ) {
        f.config = true;
    }
    if from.starts_with("flick:http/")
        || from.starts_with("flick:http-api/")
        || from.starts_with("flick:rpc/")
    {
        f.schedule = true;
    }
    if from.starts_with("flick:http/")
        || from.starts_with("flick:http-api/")
        || from.starts_with("flick:rpc/")
        || matches!(
            from,
            "flick:FileSystem" | "flick:ChildProcess" | "flick:Stdio" | "flick:Terminal"
        )
        || (from == "flick:ui/server" && name == "uiServerStream")
        || (from == "flick:rt" && name == "atomQueryStream")
        || (from == "flick:Stream"
            && matches!(
                name,
                "fromReadableStream"
                    | "toReadableStream"
                    | "toReadableStreamRuntime"
                    | "fromAsyncIterable"
                    | "fromEventListener"
                    | "flickFromReadableStream"
                    | "flickToReadableStream"
                    | "flickFromEventListener"
                    | "flickFromWritableStream"
                    | "flickToWritableStream"
            ))
        || (from == "flick:Channel"
            && matches!(
                name,
                "fromReadableStream"
                    | "toReadableStream"
                    | "fromTransformStream"
                    | "fromWritableStream"
                    | "toWritableStream"
            ))
    {
        f.stream = true;
        f.queue = true;
    }
    if from == "flick:cli" {
        f.queue = true;
    }
}

fn collect_dict_globals(dict: &Dict, out: &mut Vec<Name>) {
    match dict {
        Dict::Instance {
            module, name, args, ..
        } => {
            out.push(Name::new(module, &format!("instance {name}")));
            for arg in args {
                collect_dict_globals(arg, out);
            }
        }
        Dict::Structural { fields, .. } => {
            for field in fields {
                collect_dict_globals(field, out);
            }
        }
        Dict::Super { dict: inner, .. } => collect_dict_globals(inner, out),
        Dict::Value { module, name, .. } => out.push(Name::new(module, name)),
        Dict::Given(_)
        | Dict::Field(_)
        | Dict::Opened { .. }
        | Dict::Primitive { .. }
        | Dict::Site { .. }
        | Dict::Error => {}
    }
}

fn collect_expr_globals(expr: &Expr, out: &mut Vec<Name>) {
    match &expr.kind {
        ExprKind::Global { module, name, .. } => out.push(Name::new(module, name)),
        ExprKind::Method { dict, .. } => collect_dict_globals(dict, out),
        ExprKind::Dicts(inner, dicts) => {
            collect_expr_globals(inner, out);
            for d in dicts {
                collect_dict_globals(d, out);
            }
        }
        _ => expr.each_child(&mut |child| collect_expr_globals(child, out)),
    }
}

impl Lower<'_> {
    pub(super) fn foreign_bridge(
        &mut self,
        value: &Foreign,
        name: &Name,
        span: Span,
        block: &mut Code,
    ) -> Result<Local, LowerError> {
        let taken = value.taken();
        let runtime = taken
            .checked_sub(1)
            .and_then(|i| value.stages.get(i))
            .copied()
            .unwrap_or(false);
        let source = Name::new(
            &name.module,
            value.source_name.as_deref().unwrap_or(&value.name),
        );
        let (arg_hints, result_hint) = self.declarations.as_ref().map_or_else(
            || (vec![WireHint::Any; value.arity], WireHint::Any),
            |decls| decls.foreign_wire_hints(&source, value.arity, taken),
        );
        let spec = ForeignCallSpec {
            module: name.module.clone(),
            source_dir: self.module_dirs.get(&name.module).cloned(),
            from: value.binding.from.clone(),
            name: value.binding.name.clone(),
            mode: value.mode,
            dicts: value.dicts,
            arity: value.arity,
            taken,
            runtime,
            has_catch: value.catch.is_some(),
            adapter: value.adapter.clone(),
            arg_hints,
            result_hint,
            remaining_stages: value
                .stages
                .get(taken..)
                .map(<[bool]>::to_vec)
                .unwrap_or_default(),
        };
        let total = value
            .dicts
            .checked_add(taken)
            .ok_or(LowerError::Limit(span))?;
        if total == 0 && value.mode == ForeignMode::Pure {
            let mut args = Vec::new();
            if let Some(catch_expr) = &value.catch {
                args.push(self.expr(catch_expr, &Env::new(), block)?);
            }
            return block.emit(
                Operation::ForeignCall {
                    spec: Box::new(spec),
                    args,
                },
                span,
            );
        }
        self.foreign_bridge_stage(value, &spec, total, vec![], block, span)
    }

    fn foreign_bridge_stage(
        &mut self,
        value: &Foreign,
        spec: &ForeignCallSpec,
        total: usize,
        collected: Vec<Local>,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        if collected.len() == total {
            let mut call_args = Vec::with_capacity(spec.expected_args());
            call_args.extend_from_slice(&collected[..spec.dicts]);
            if spec.arity > 0 {
                call_args.extend_from_slice(&collected[spec.dicts..spec.dicts + spec.arity]);
            }
            if let Some(catch_expr) = &value.catch {
                // Lowered inside the call stage so mutually/forward-declared
                // catch mappings are evaluated on call, not at module init.
                call_args.push(self.expr(catch_expr, &Env::new(), block)?);
            }
            return block.emit(
                Operation::ForeignCall {
                    spec: Box::new(spec.clone()),
                    args: call_args,
                },
                span,
            );
        }
        let code = self.reserve(span)?;
        let mut function = Code::new(vec![0, 1]);
        let mut inside = Vec::with_capacity(collected.len() + 1);
        for field in 0..collected.len() {
            inside.push(function.emit(Operation::Project { object: 0, field }, span)?);
        }
        inside.push(1);
        let result = self.foreign_bridge_stage(value, spec, total, inside, &mut function, span)?;
        self.finish(code, &function.finish(result))?;
        block.emit(
            Operation::Closure {
                code,
                captures: collected,
            },
            span,
        )
    }
}

/// Resolves every bridged `flick:*` foreign specifier onto the exact runtime or
/// Effect-TS adapter module selected by the compiler's single-owner UI/web rules,
/// matching `flick_core::build::javascript_effect_bridge`.
pub(super) fn resolve_routes(program: &mut Program, families: BridgeFamilies) {
    let mut ui = false;
    let mut web = false;
    for func in program.functions.values() {
        scan_block_families(&func.body, &mut ui, &mut web);
    }
    let bridge = EffectBridge::owning(ui, web);
    for func in program.functions.values_mut() {
        rewrite_block_routes(&mut func.body, bridge, families);
    }
}

fn scan_block_families(block: &Block, ui: &mut bool, web: &mut bool) {
    for inst in &block.instructions {
        if let Instruction::Let {
            value: Operation::ForeignCall { spec, .. },
            ..
        } = inst
            && let Some(source) = spec.from.as_deref()
        {
            if bindings::UI_REQUIRED
                .iter()
                .any(|line| line.split_once(' ') == Some((source, spec.name.as_str())))
            {
                *ui = true;
            }
            if let Some(rt) = rt_specifier(source)
                && ui_family(&rt, &spec.name)
                && !on_kernel_with_bridge(&rt, &spec.name, true)
            {
                *ui = true;
            }
            if web_family(source)
                && !rt_specifier(source)
                    .is_some_and(|rt| on_kernel_with_bridge(&rt, &spec.name, true))
            {
                *web = true;
            }
        }
    }
    if let Exit::Branch { then_, else_, .. } = &block.exit {
        scan_block_families(then_, ui, web);
        scan_block_families(else_, ui, web);
    }
}

fn force_effect_adapter(source: &str, name: &str, families: BridgeFamilies) -> bool {
    if families.observe {
        if matches!(
            source,
            "flick:Tracer"
                | "flick:Logger"
                | "flick:Metric"
                | "flick:References"
                | "flick:LogLevel"
        ) {
            return true;
        }
        if source == "flick:Effect"
            && matches!(
                name,
                "log"
                    | "logFatal"
                    | "logError"
                    | "logWarning"
                    | "logInfo"
                    | "logDebug"
                    | "logTrace"
                    | "flickLogWithLevel"
                    | "withLogger"
                    | "annotateLogs"
                    | "annotateLogsScoped"
                    | "withLogSpan"
                    | "flickTracer"
                    | "annotateSpans"
                    | "annotateCurrentSpan"
                    | "flickCurrentSpan"
                    | "flickCurrentParentSpan"
                    | "flickSpanLinksNow"
                    | "flickLinkSpans"
                    | "flickMakeSpan"
                    | "flickMakeSpanScoped"
                    | "flickUseSpan"
                    | "flickWithSpanScoped"
                    | "withParentSpan"
            )
        {
            return true;
        }
        if source == "flick:Layer"
            && matches!(
                name,
                "flickSpan" | "parentSpan" | "flickWithSpan" | "withParentSpan"
            )
        {
            return true;
        }
    }
    if families.config && matches!(source, "flick:Config" | "flick:ConfigProvider") {
        return true;
    }
    if families.stream
        && matches!(
            source,
            "flick:Stream" | "flick:Sink" | "flick:Channel" | "flick:ChannelSchema"
        )
    {
        return true;
    }
    if families.queue && matches!(source, "flick:Queue" | "flick:PubSub") {
        return true;
    }
    if families.schedule && source == "flick:Schedule" {
        return true;
    }
    false
}

fn rewrite_block_routes(block: &mut Block, bridge: EffectBridge, families: BridgeFamilies) {
    for inst in &mut block.instructions {
        if let Instruction::Let {
            value: Operation::ForeignCall { spec, .. },
            ..
        } = inst
            && let Some(source) = spec.from.as_deref()
            && owned(source)
        {
            let resolved = if force_effect_adapter(source, &spec.name, families) {
                effect_specifier(source)
            } else {
                match rt_specifier(source) {
                    Some(rt) if selected_on_kernel(&rt, &spec.name, bridge) => rt,
                    _ => effect_specifier(source),
                }
            };
            spec.from = Some(resolved);
        }
    }
    if let Exit::Branch { then_, else_, .. } = &mut block.exit {
        rewrite_block_routes(then_, bridge, families);
        rewrite_block_routes(else_, bridge, families);
    }
}
