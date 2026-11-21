//! Audited `Channel.Core` pure IR adapters + shared pull/scope helpers (Stage 4E).

mod lifecycle;
mod sources;

use flick_ir::{Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::std_channel_control::channel_control_template;
use super::std_channel_run::channel_run_template;
use super::std_channel_text::{channel_schema_template, channel_text_template};
use super::std_channel_transform::channel_transform_template;
use super::std_schema_rt::rt;
use super::{Code, Env, Expr, Local, Lower, LowerError, Pat, Span};

pub(super) const DEFAULT_CHUNK_SIZE: i32 = 4096;

impl Lower<'_> {
    pub(super) fn std_channel_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 || !has_standard_shape(value) {
            return None;
        }
        let from = value.binding.from.as_deref()?;
        let bind = value.binding.name.as_str();
        let stages = value.stages.len();
        let b = ExprBuilder::new(span, "$ch");
        let expr = match from {
            "flick:ChannelSchema" if value.mode == ForeignMode::Pure => {
                channel_schema_template(&b, bind, stages)?
            }
            "flick:Channel" if value.mode == ForeignMode::Effect => {
                channel_run_template(&b, bind, stages)?
            }
            "flick:Channel" if value.mode == ForeignMode::Pure => {
                channel_core_template(&b, bind, stages)
                    .or_else(|| channel_transform_template(&b, bind, stages))
                    .or_else(|| channel_text_template(&b, bind, stages))
                    .or_else(|| channel_control_template(&b, bind, stages))?
            }
            _ => return None,
        };
        Some(self.expr(&expr, &Env::new(), block))
    }
}

pub(super) fn channel_core_helper(name: &str, span: Span) -> Option<Expr> {
    let b = ExprBuilder::new(span, "$chc");
    match name {
        "ch_empty_pull" => Some(b.lam(&["_u"], b.err(b.unit()))),
        "ch_run_scoped" => Some(lifecycle::ch_run_scoped_helper(&b)),
        "ch_fork_scope" => Some(lifecycle::ch_fork_scope_helper(&b)),
        "ch_bracket" => Some(lifecycle::ch_bracket_helper(&b)),
        "ch_from_list" => Some(sources::ch_from_list_helper(&b)),
        "ch_from_list_chunked" => Some(sources::ch_from_list_chunked_helper(&b)),
        "ch_from_effect" => Some(sources::ch_from_effect_helper(&b)),
        "ch_from_queue" => Some(sources::ch_from_queue_helper(&b)),
        "ch_from_sub" => Some(sources::ch_from_sub_helper(&b)),
        "ch_from_pubsub" => Some(sources::ch_from_pubsub_helper(&b)),
        "ch_sched_step" => Some(sources::ch_sched_step_helper(&b)),
        "ch_from_schedule" => Some(sources::ch_from_schedule_helper(&b)),
        "ch_unwrap" => Some(sources::ch_unwrap_helper(&b)),
        "ch_callback" => Some(lifecycle::ch_callback_helper(&b)),
        "ch_acquire_release" => Some(lifecycle::ch_acquire_release_helper(&b)),
        "ch_acquire_use_release" => Some(lifecycle::ch_acquire_use_release_helper(&b)),
        "ch_make_pubsub" => Some(sources::ch_make_pubsub_helper(&b)),
        _ => None,
    }
}

fn from_chunk_tpl(b: &ExprBuilder) -> Expr {
    let pat = Pat::Con(
        ExprBuilder::con("Chunk.Core", "Chunk", "$Chunk", 1),
        vec![Pat::Bind(b.name("xs"))],
    );
    let unpack = b.match_(b.loc("c"), [arm(pat, b.loc("xs"))]);
    b.lam(&["c"], b.app(rt(b, "Helper.ch_from_list"), unpack))
}

fn const_pull_ch(b: &ExprBuilder, params: &[&str], pull_body: Expr) -> Expr {
    b.lam(params, b.lam(&["_u"], pull_body))
}

fn channel_core_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("DefaultChunkSize", 0) => Some(b.int(DEFAULT_CHUNK_SIZE)),
        ("fromArray", 1) => Some(rt(b, "Helper.ch_from_list")),
        ("flickFromListChunked", 2) => Some(rt(b, "Helper.ch_from_list_chunked")),
        ("fromChunk", 1) => Some(from_chunk_tpl(b)),
        ("succeed", 1) => Some(b.lam(
            &["v"],
            b.app(rt(b, "Helper.ch_from_list"), b.list([b.loc("v")])),
        )),
        ("sync", 1) => Some(b.lam(
            &["f"],
            b.app(
                rt(b, "Helper.ch_from_effect"),
                b.task(b.app(b.loc("f"), b.unit())),
            ),
        )),
        ("end", 1) => Some(const_pull_ch(b, &["d", "_up", "_own"], b.err(b.loc("d")))),
        ("endSync", 1) => Some(const_pull_ch(
            b,
            &["f", "_up", "_own"],
            b.err(b.app(b.loc("f"), b.unit())),
        )),
        ("empty", 0) => Some(const_pull_ch(b, &["_up", "_own"], b.err(b.unit()))),
        ("never", 0) => Some(const_pull_ch(
            b,
            &["_up", "_own"],
            b.app(rt(b, "Async.EffectNever"), b.unit()),
        )),
        ("fail", 1) => Some(const_pull_ch(
            b,
            &["e", "_up", "_own"],
            b.app(b.global("Fail", "fail"), b.loc("e")),
        )),
        ("failSync", 1) => Some(const_pull_ch(
            b,
            &["f", "_up", "_own"],
            b.app(b.global("Fail", "fail"), b.app(b.loc("f"), b.unit())),
        )),
        ("flickFailCause", 1) => Some(const_pull_ch(
            b,
            &["c", "_up", "_own"],
            b.app(b.global("Failure", "rethrow"), b.loc("c")),
        )),
        ("flickFailCauseSync", 1) => Some(const_pull_ch(
            b,
            &["f", "_up", "_own"],
            b.app(b.global("Failure", "rethrow"), b.app(b.loc("f"), b.unit())),
        )),
        ("fromEffect", 1) => Some(rt(b, "Helper.ch_from_effect")),
        ("fromEffectDone", 1) => Some(const_pull_ch(
            b,
            &["t", "_up", "_own"],
            b.err(b.await_(b.loc("t"))),
        )),
        ("fromEffectDrain", 1) => Some(const_pull_ch(
            b,
            &["t", "_up", "_own"],
            b.block([b.let_("_x", b.await_(b.loc("t")))], b.err(b.unit())),
        )),
        ("fromQueue", 1) => Some(b.app(rt(b, "Helper.ch_from_queue"), b.bool_(false))),
        ("fromQueueArray", 1) => Some(b.app(rt(b, "Helper.ch_from_queue"), b.bool_(true))),
        ("fromSubscription", 1) => Some(b.app(rt(b, "Helper.ch_from_sub"), b.bool_(false))),
        ("fromSubscriptionArray", 1) => Some(b.app(rt(b, "Helper.ch_from_sub"), b.bool_(true))),
        ("fromPubSub", 1) => Some(b.app(rt(b, "Helper.ch_from_pubsub"), b.bool_(false))),
        ("fromPubSubArray", 1) => Some(b.app(rt(b, "Helper.ch_from_pubsub"), b.bool_(true))),
        ("fromSchedule", 1) => Some(rt(b, "Helper.ch_from_schedule")),
        ("suspend", 1) => Some(b.lam(
            &["f", "up", "own"],
            b.app2(b.app(b.loc("f"), b.unit()), b.loc("up"), b.loc("own")),
        )),
        ("unwrap", 1) => Some(rt(b, "Helper.ch_unwrap")),
        ("identity", 1) => Some(b.lam(&["_u", "up", "_own"], b.loc("up"))),
        ("flickCallback", 2) => Some(b.app(rt(b, "Helper.ch_callback"), b.bool_(false))),
        ("flickCallbackArray", 2) => Some(b.app(rt(b, "Helper.ch_callback"), b.bool_(true))),
        ("flickAcquireRelease", 2) => Some(rt(b, "Helper.ch_acquire_release")),
        ("flickAcquireUseRelease", 3) => Some(rt(b, "Helper.ch_acquire_use_release")),
        _ => None,
    }
}
