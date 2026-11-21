//! Context/service provision and tracing span `Channel.Control` IR helpers.

use super::super::expr_builder::ExprBuilder;
use super::super::std_observe_trace::{end_span_expr, open_span_expr, with_parent_span_expr};
use super::super::std_schema_rt::rt;
use super::super::{Expr, Stmt};

pub(super) fn ch_provide_ctx_helper(b: &ExprBuilder) -> Expr {
    let init_pull = b.app2(
        rt(b, "Layer.ProvideContext"),
        b.loc("ctx"),
        b.task(b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
    );
    let pull = b.lam(
        &["_u"],
        b.app2(
            rt(b, "Layer.ProvideContext"),
            b.loc("ctx"),
            b.task(b.app(b.loc("pull"), b.unit())),
        ),
    );
    b.lam(
        &["ctx", "self", "up", "own"],
        b.block([b.let_("pull", init_pull)], pull),
    )
}

pub(super) fn ch_provide_helper(b: &ExprBuilder) -> Expr {
    let build_ctx = b.app2(
        rt(b, "Layer.OwnerRun"),
        b.loc("own"),
        b.task(b.app(rt(b, "Layer.Build"), b.loc("lay"))),
    );
    let provide = b.app4(
        rt(b, "Helper.ch_provide_ctx"),
        b.loc("ctx"),
        b.loc("self"),
        b.loc("up"),
        b.loc("own"),
    );
    b.lam(
        &["lay", "self", "up", "own"],
        b.block([b.let_("ctx", build_ctx)], provide),
    )
}

pub(super) fn ch_provide_service_helper(b: &ExprBuilder) -> Expr {
    let ctx = b.app2(rt(b, "Layer.ContextMake"), b.loc("tag"), b.loc("impl"));
    b.lam(
        &["tag", "impl", "self"],
        b.app2(rt(b, "Helper.ch_provide_ctx"), ctx, b.loc("self")),
    )
}

pub(super) fn ch_provide_service_eff_helper(b: &ExprBuilder) -> Expr {
    let provide = b.app4(
        rt(b, "Helper.ch_provide_ctx"),
        b.loc("ctx"),
        b.loc("self"),
        b.loc("up"),
        b.loc("own"),
    );
    b.lam(
        &["tag", "t", "self", "up", "own"],
        b.block(
            [
                b.let_("impl", b.await_(b.loc("t"))),
                b.let_(
                    "ctx",
                    b.app2(rt(b, "Layer.ContextMake"), b.loc("tag"), b.loc("impl")),
                ),
            ],
            provide,
        ),
    )
}

pub(super) fn ch_update_service_helper(b: &ExprBuilder) -> Expr {
    let updated = b.app(b.loc("f"), b.loc("old"));
    let provide = b.app4(
        rt(b, "Helper.ch_provide_ctx"),
        b.loc("ctx"),
        b.loc("self"),
        b.loc("up"),
        b.loc("own"),
    );
    b.lam(
        &["tag", "f", "self", "up", "own"],
        b.block(
            [
                b.let_("old", b.app(rt(b, "Layer.LookupService"), b.loc("tag"))),
                b.let_(
                    "ctx",
                    b.app2(rt(b, "Layer.ContextMake"), b.loc("tag"), updated),
                ),
            ],
            provide,
        ),
    )
}

pub(super) fn ch_with_span_helper(b: &ExprBuilder) -> Expr {
    let end_fin = b.app2(
        rt(b, "Layer.OwnerRun"),
        b.loc("forked"),
        b.task(b.app(
            b.global("Scope", "add_finalizer"),
            b.task(end_span_expr(b, b.loc("sp"))),
        )),
    );
    let init_pull = with_parent_span_expr(
        b,
        b.loc("sp"),
        b.task(b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
    );
    let pull = b.lam(
        &["_u"],
        with_parent_span_expr(b, b.loc("sp"), b.task(b.app(b.loc("pull"), b.unit()))),
    );
    let build = b.lam(
        &["up", "own", "forked"],
        b.block(
            [
                b.let_("sp", open_span_expr(b, b.loc("nm"), b.loc("opts"))),
                Stmt::Expr(end_fin),
                b.let_("pull", init_pull),
            ],
            pull,
        ),
    );
    b.lam(
        &["nm", "opts", "self"],
        b.app(rt(b, "Helper.ch_bracket"), build),
    )
}
