//! `RequestResolver.with_span` tracing span linking combinator.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_request::{entry_pat, res_pat};
use super::super::{Expr, ExprKind, Pat};
use super::clone_resolver;

fn add_unique_span_link(b: &ExprBuilder) -> Expr {
    let link_rec = b.at(ExprKind::Record(vec![
        ("span".into(), b.loc("s")),
        ("attributes".into(), b.list(vec![])),
    ]));
    let add_link = b.block(
        [
            b.assign(
                "seen",
                b.app2(
                    b.global("List.Core", "append"),
                    b.var("seen"),
                    b.list([b.loc("sid")]),
                ),
            ),
            b.assign(
                "lks",
                b.app2(
                    b.global("List.Core", "append"),
                    b.var("lks"),
                    b.list([link_rec]),
                ),
            ),
        ],
        b.unit(),
    );
    let is_dup = b.app2(
        b.global("List.Core", "any"),
        b.lam(&["x"], b.bin(BinOp::Eq, b.loc("x"), b.loc("sid"))),
        b.var("seen"),
    );
    b.block(
        [
            b.let_("sid", b.app(b.global("Tracer", "span_id"), b.loc("s"))),
            b.let_("dup", is_dup),
        ],
        b.if_(b.loc("dup"), b.unit(), add_link),
    )
}

fn build_links_expr(b: &ExprBuilder) -> Expr {
    let on_span = b.match_(
        b.loc("e_sp"),
        [
            arm(b.some_pat("s"), add_unique_span_link(b)),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let step = b.match_(b.loc("e"), [arm(entry_pat(b), on_span)]);
    let loop_expr = b.for_ctrl("wsl", "e", b.loc("es"), step);
    b.block(
        [
            b.var_decl("seen", b.list(vec![])),
            b.var_decl("lks", b.field(b.loc("opts"), "links")),
            b.let_("_ll", loop_expr),
        ],
        b.var("lks"),
    )
}

pub(super) fn with_span_template(b: &ExprBuilder) -> Expr {
    let batch_size_str = b.app(
        b.global("Int", "to_string"),
        b.app(b.global("List.Core", "length"), b.loc("es")),
    );
    let attrs = b.app2(
        b.global("List.Core", "append"),
        b.list([b.tuple([b.str_("batchSize"), batch_size_str])]),
        b.field(b.loc("opts"), "attributes"),
    );
    let sopts = b.at(ExprKind::Record(vec![
        ("attributes".into(), b.loc("attrs")),
        ("links".into(), b.loc("links")),
        ("parent".into(), b.field(b.loc("opts"), "parent")),
        ("root".into(), b.field(b.loc("opts"), "root")),
        ("kind_".into(), b.field(b.loc("opts"), "kind_")),
        ("sampled".into(), b.field(b.loc("opts"), "sampled")),
        ("level".into(), b.field(b.loc("opts"), "level")),
        (
            "captureStackTrace".into(),
            b.field(b.loc("opts"), "captureStackTrace"),
        ),
        (
            "disablePropagation".into(),
            b.field(b.loc("opts"), "disablePropagation"),
        ),
    ]));
    let run_spanned = b.app3(
        b.global("Tracer", "with_span"),
        b.loc("nm"),
        b.loc("sopts"),
        b.task(b.app2(b.loc("r_run"), b.loc("es"), b.loc("k"))),
    );
    let spanned_run = b.lam(
        &["es", "k"],
        b.block(
            [
                b.let_("attrs", attrs),
                b.let_("links", build_links_expr(b)),
                b.let_("sopts", sopts),
            ],
            run_spanned,
        ),
    );
    b.lam(
        &["nm", "opts", "self_r"],
        b.match_(
            b.loc("self_r"),
            [arm(
                res_pat(b),
                clone_resolver(b, None, None, None, None, None, Some(spanned_run)),
            )],
        ),
    )
}
