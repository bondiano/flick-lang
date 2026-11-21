//! Metric snapshot, dump formatting, and histogram boundary templates.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_metric::{fadd, fmul, sort_pairs_expr};
use super::super::{Expr, ExprKind, Pat};

fn metric_state_con(b: &ExprBuilder, name: &str, arg: Expr) -> Expr {
    b.app(
        b.at(ExprKind::Con(ExprBuilder::con("Metric", "State", name, 1))),
        arg,
    )
}

fn wrap_state_expr(b: &ExprBuilder, kind: Expr, raw: Expr) -> Expr {
    let hist_or_sum = b.if_(
        b.bin(BinOp::Eq, b.loc("sk"), b.str_("Histogram")),
        metric_state_con(b, "HistogramValue", b.loc("sr")),
        metric_state_con(b, "SummaryValue", b.loc("sr")),
    );
    let freq_or = b.if_(
        b.bin(BinOp::Eq, b.loc("sk"), b.str_("Frequency")),
        metric_state_con(b, "FrequencyValue", b.loc("sr")),
        hist_or_sum,
    );
    let gauge_or = b.if_(
        b.bin(BinOp::Eq, b.loc("sk"), b.str_("Gauge")),
        metric_state_con(b, "GaugeValue", b.loc("sr")),
        freq_or,
    );
    let body = b.if_(
        b.bin(BinOp::Eq, b.loc("sk"), b.str_("Counter")),
        metric_state_con(b, "CounterValue", b.loc("sr")),
        gauge_or,
    );
    b.block([b.let_("sk", kind), b.let_("sr", raw)], body)
}

pub(super) fn snapshot_template(b: &ExprBuilder) -> Expr {
    let entry_rec = b.at(ExprKind::Record(vec![
        ("id".into(), b.loc("eid")),
        ("description".into(), b.loc("edesc")),
        ("attributes".into(), b.loc("eattrs")),
        (
            "state".into(),
            wrap_state_expr(b, b.loc("ekind"), b.app(b.loc("gh"), b.unit())),
        ),
    ]));
    let map_entry = b.lam(
        &["e"],
        b.match_(
            b.loc("e"),
            [arm(
                b.tup_pat(&["_k", "eid", "ekind", "edesc", "eattrs", "gh", "_u", "_m"]),
                entry_rec,
            )],
        ),
    );
    let reg_entries = b.app2(
        b.global("List", "map"),
        map_entry,
        b.rget(b.global("$rt", "Metric.Registry")),
    );
    let rt_counter = b.at(ExprKind::Record(vec![
        ("count".into(), b.loc("rf")),
        ("incremental".into(), b.bool_(true)),
    ]));
    let rt_entry = b.at(ExprKind::Record(vec![
        ("id".into(), b.str_("child_fibers_started")),
        (
            "description".into(),
            b.some(b.str_("The total number of child fibers that have been started")),
        ),
        ("attributes".into(), b.list(vec![])),
        (
            "state".into(),
            metric_state_con(b, "CounterValue", rt_counter),
        ),
    ]));
    let with_rt = b.app2(
        b.global("List.Core", "append"),
        b.loc("snaps"),
        b.list([rt_entry]),
    );
    b.lam(
        &["_u"],
        b.block(
            [
                b.let_("snaps", reg_entries),
                b.let_("rf", b.app(b.loc("metric_fibers_started"), b.unit())),
            ],
            b.if_(
                b.bin(BinOp::Gt, b.loc("rf"), b.float(0.0)),
                with_rt,
                b.loc("snaps"),
            ),
        ),
    )
}

pub(super) fn dump_template(b: &ExprBuilder) -> Expr {
    let format_kv = b.lam(
        &["kv"],
        b.match_(
            b.loc("kv"),
            [arm(
                b.tup_pat(&["k", "v"]),
                b.bin(
                    BinOp::Concat,
                    b.loc("k"),
                    b.bin(BinOp::Concat, b.str_(": "), b.loc("v")),
                ),
            )],
        ),
    );
    let joined_attrs = b.bin(
        BinOp::Concat,
        b.app2(b.global("String", "join"), b.str_(", "), b.loc("parts")),
        b.str_("]"),
    );
    let format_attrs = b.block(
        [
            b.let_("sp", sort_pairs_expr(b, b.field(b.loc("sn"), "attributes"))),
            b.let_(
                "parts",
                b.app2(b.global("List", "map"), format_kv, b.loc("sp")),
            ),
        ],
        b.bin(BinOp::Concat, b.str_("attributes=["), joined_attrs),
    );
    let desc_expr = b.match_(
        b.field(b.loc("sn"), "description"),
        [arm(b.some_pat("d"), b.loc("d")), arm(Pat::Wild, b.str_(""))],
    );
    let desc_and_attrs = b.bin(
        BinOp::Concat,
        b.loc("desc"),
        b.bin(BinOp::Concat, b.str_("  "), b.loc("atxt")),
    );
    let line_body = b.bin(
        BinOp::Concat,
        b.str_("name="),
        b.bin(
            BinOp::Concat,
            b.field(b.loc("sn"), "id"),
            b.bin(BinOp::Concat, b.str_("  description="), desc_and_attrs),
        ),
    );
    let format_line = b.lam(
        &["sn"],
        b.block(
            [b.let_("desc", desc_expr), b.let_("atxt", format_attrs)],
            line_body,
        ),
    );
    b.lam(
        &["_u"],
        b.app2(
            b.global("String", "join"),
            b.str_("\n"),
            b.app2(
                b.global("List", "map"),
                format_line,
                b.app(b.global("Metric", "snapshot"), b.unit()),
            ),
        ),
    )
}

pub(super) fn boundaries_from_iterable_template(b: &ExprBuilder) -> Expr {
    let push_out = b.if_(
        b.bin(BinOp::Gt, b.loc("n"), b.float(0.0)),
        b.app2(
            b.global("List.Core", "append"),
            b.var("out"),
            b.list([b.loc("n")]),
        ),
        b.var("out"),
    );
    let record_num = b.block(
        [
            b.assign(
                "seen",
                b.app2(
                    b.global("List.Core", "append"),
                    b.var("seen"),
                    b.list([b.loc("n")]),
                ),
            ),
            b.assign("out", push_out),
        ],
        b.unit(),
    );
    let is_dup = b.app2(
        b.global("List.Core", "any"),
        b.lam(&["s"], b.bin(BinOp::Eq, b.loc("s"), b.loc("n"))),
        b.var("seen"),
    );
    let check_dup = b.block(
        [b.let_("dup", is_dup)],
        b.if_(b.loc("dup"), b.unit(), record_num),
    );
    let step = b.if_(
        b.app(b.global("Float", "is_nan"), b.loc("n")),
        b.unit(),
        check_dup,
    );
    b.lam(
        &["xs"],
        b.block(
            [
                b.var_decl("seen", b.list(vec![])),
                b.var_decl("out", b.list(vec![])),
                b.let_("_bl", b.for_ctrl("bfi", "n", b.loc("xs"), step)),
            ],
            b.app2(
                b.global("List.Core", "append"),
                b.var("out"),
                b.list([b.global("Float", "infinity")]),
            ),
        ),
    )
}

pub(super) fn linear_boundaries_template(b: &ExprBuilder) -> Expr {
    let scaled = fmul(
        b,
        b.app(b.global("Int", "to_float"), b.var("i")),
        b.field(b.loc("o"), "width"),
    );
    let loop_body = b.block(
        [
            b.let_("v", fadd(b, b.field(b.loc("o"), "start"), scaled)),
            b.assign("i", b.add_i(b.var("i"), b.int(1))),
        ],
        b.yield_(b.loc("v")),
    );
    let loop_expr = b.while_(b.bin(BinOp::Lt, b.var("i"), b.loc("lim")), loop_body);
    let lim = b.if_(
        b.bin(BinOp::Lt, b.loc("cm"), b.int(1)),
        b.int(1),
        b.loc("cm"),
    );
    b.lam(
        &["o"],
        b.block(
            [
                b.let_("cm", b.sub_i(b.field(b.loc("o"), "count"), b.int(1))),
                b.let_("lim", lim),
                b.var_decl("i", b.int(0)),
            ],
            b.app(
                b.global("Metric", "boundaries_from_iterable"),
                b.gen_strict(loop_expr),
            ),
        ),
    )
}

pub(super) fn exponential_boundaries_template(b: &ExprBuilder) -> Expr {
    let loop_body = b.block(
        [
            b.let_("v", b.var("cur")),
            b.assign("cur", fmul(b, b.var("cur"), b.field(b.loc("o"), "factor"))),
            b.assign("i", b.add_i(b.var("i"), b.int(1))),
        ],
        b.yield_(b.loc("v")),
    );
    let loop_expr = b.while_(b.bin(BinOp::Lt, b.var("i"), b.loc("lim")), loop_body);
    let lim = b.if_(
        b.bin(BinOp::Lt, b.loc("cm"), b.int(1)),
        b.int(1),
        b.loc("cm"),
    );
    b.lam(
        &["o"],
        b.block(
            [
                b.let_("cm", b.sub_i(b.field(b.loc("o"), "count"), b.int(1))),
                b.let_("lim", lim),
                b.var_decl("i", b.int(0)),
                b.var_decl("cur", b.field(b.loc("o"), "start")),
            ],
            b.app(
                b.global("Metric", "boundaries_from_iterable"),
                b.gen_strict(loop_expr),
            ),
        ),
    )
}
