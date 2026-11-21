//! `Stream.Combine` cross, zip, zipLatest, race, interleave, and halt/interrupt templates.

use super::super::Expr;
use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{pair_pat, rt};

fn cross_template(b: &ExprBuilder, params: &[&str], elem: Expr) -> Expr {
    let map_r = b.lam(
        &["rs"],
        b.gen_strict(b.each("r", b.loc("rs"), b.yield_(elem))),
    );
    let per_l = b.lam(
        &["l"],
        b.app2(rt(b, "Helper.ch_map"), map_r, b.loc("right")),
    );
    let flat = b.app(rt(b, "Helper.ch_flatten_array"), b.loc("self"));
    b.lam(params, b.app2(rt(b, "Helper.ch_flat_map"), per_l, flat))
}

pub(super) fn stream_zip_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("cross", 2) => Some(cross_template(
            b,
            &["right", "self"],
            b.tuple([b.loc("l"), b.loc("r")]),
        )),
        ("crossWith", 3) => Some(cross_template(
            b,
            &["right", "f", "self"],
            b.app2(b.loc("f"), b.loc("l"), b.loc("r")),
        )),
        ("zip", 2) => {
            let pair = b.lam(&["a", "c"], b.tuple([b.loc("a"), b.loc("c")]));
            let body = b.app3(
                rt(b, "Helper.st_zip_with"),
                b.loc("that"),
                pair,
                b.loc("self"),
            );
            Some(b.lam(&["that", "self"], body))
        }
        ("zipWith", 3) => Some(rt(b, "Helper.st_zip_with")),
        ("zipWithArray", 3) => Some(rt(b, "Helper.st_zip_with_array")),
        ("zipLeft", 2) => {
            let pick = b.lam(&["a", "_c"], b.loc("a"));
            let body = b.app3(
                rt(b, "Helper.st_zip_with"),
                b.loc("right"),
                pick,
                b.loc("self"),
            );
            Some(b.lam(&["right", "self"], body))
        }
        ("zipRight", 2) => {
            let pick = b.lam(&["_a", "c"], b.loc("c"));
            let body = b.app3(
                rt(b, "Helper.st_zip_with"),
                b.loc("right"),
                pick,
                b.loc("self"),
            );
            Some(b.lam(&["right", "self"], body))
        }
        ("zipFlatten", 2) => {
            let flat3 = b.lam(
                &["ab", "c"],
                b.match_(
                    b.loc("ab"),
                    [arm(
                        pair_pat(b, "a", "b_"),
                        b.tuple([b.loc("a"), b.loc("b_"), b.loc("c")]),
                    )],
                ),
            );
            let body = b.app3(
                rt(b, "Helper.st_zip_with"),
                b.loc("that"),
                flat3,
                b.loc("self"),
            );
            Some(b.lam(&["that", "self"], body))
        }
        ("zipWithIndex", 1) => Some(rt(b, "Helper.st_zip_with_index")),
        ("zipWithNext", 1) => Some(rt(b, "Helper.st_zip_with_next")),
        ("zipWithPrevious", 1) => Some(rt(b, "Helper.st_zip_with_previous")),
        ("zipWithPreviousAndNext", 1) => Some(rt(b, "Helper.st_zip_with_prev_next")),
        _ => stream_latest_race_template(b, bind, stages),
    }
}

fn stream_latest_race_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("zipLatestAll", 1) => Some(rt(b, "Helper.st_zip_latest_all")),
        ("zipLatest", 2) => {
            let tup = b.tuple([
                b.app2(rt(b, "Helper.list_at"), b.int(0), b.loc("p")),
                b.app2(rt(b, "Helper.list_at"), b.int(1), b.loc("p")),
            ]);
            let map_xs = b.lam(
                &["xs"],
                b.gen_strict(b.each("p", b.loc("xs"), b.yield_(tup))),
            );
            let all = b.app(
                rt(b, "Helper.st_zip_latest_all"),
                b.list([b.loc("self"), b.loc("that")]),
            );
            Some(b.lam(
                &["that", "self"],
                b.app2(rt(b, "Helper.ch_map"), map_xs, all),
            ))
        }
        ("zipLatestWith", 3) => {
            let applied = b.app2(
                b.loc("f"),
                b.app2(rt(b, "Helper.list_at"), b.int(0), b.loc("p")),
                b.app2(rt(b, "Helper.list_at"), b.int(1), b.loc("p")),
            );
            let map_xs = b.lam(
                &["xs"],
                b.gen_strict(b.each("p", b.loc("xs"), b.yield_(applied))),
            );
            let all = b.app(
                rt(b, "Helper.st_zip_latest_all"),
                b.list([b.loc("self"), b.loc("that")]),
            );
            Some(b.lam(
                &["that", "f", "self"],
                b.app2(rt(b, "Helper.ch_map"), map_xs, all),
            ))
        }
        ("raceAll", 1) => Some(rt(b, "Helper.st_race_all")),
        ("race", 2) => Some(b.lam(
            &["that", "self"],
            b.app(
                rt(b, "Helper.st_race_all"),
                b.list([b.loc("self"), b.loc("that")]),
            ),
        )),
        ("interleave", 2) => {
            let alt = b.app(
                rt(b, "Helper.ch_from_list"),
                b.list([b.list([b.bool_(true), b.bool_(false)])]),
            );
            let dec = b.app(rt(b, "Helper.ch_forever"), alt);
            let body = b.app3(
                rt(b, "Helper.st_interleave_with"),
                b.loc("that"),
                dec,
                b.loc("self"),
            );
            Some(b.lam(&["that", "self"], body))
        }
        ("interleaveWith", 3) => Some(rt(b, "Helper.st_interleave_with")),
        ("interruptWhen", 2) => Some(rt(b, "Helper.ch_interrupt_when")),
        ("haltWhen", 2) => Some(rt(b, "Helper.ch_halt_when")),
        _ => None,
    }
}
