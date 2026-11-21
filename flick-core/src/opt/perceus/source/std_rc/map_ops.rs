//! `RcMap` construction and management templates (`flickMake`, `flickKeys`, `has`, `invalidate`, `touch`).

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, Pat, Stmt};
use super::map_get::{rc_map_find, rc_map_get, rc_map_get_option, rc_map_remove_entry};
use super::rc_retire_entry;

fn rc_map_make(b: &ExprBuilder) -> Expr {
    let retire_one = b.match_(
        b.loc("p"),
        [arm(b.tup_pat(&["_k", "e"]), rc_retire_entry(b, b.loc("e")))],
    );
    let close_all = b.block(
        [
            b.let_("all", b.rget(b.loc("ents"))),
            b.rset(b.loc("ents"), b.list(vec![])),
            b.let_("_cl", b.for_ctrl("rcmcl", "p", b.loc("all"), retire_one)),
        ],
        b.unit(),
    );
    let tuple = b.tuple([
        b.field(b.loc("opts"), "lookup"),
        b.field(b.loc("opts"), "idleTimeToLive"),
        b.field(b.loc("opts"), "capacity"),
        b.loc("env"),
        b.loc("pown"),
        b.loc("ents"),
        b.loc("lk"),
    ]);
    b.lam(
        &["opts"],
        b.block(
            [
                b.let_("env", b.app(b.loc("env_capture"), b.unit())),
                b.let_("pown", b.app(b.global("Scope", "current"), b.unit())),
                b.let_("ents", b.rmake(b.list(vec![]))),
                b.let_("lk", b.app(b.global("Semaphore", "make"), b.int(1))),
                Stmt::Expr(b.app(b.global("Scope", "add_finalizer"), b.task(close_all))),
            ],
            tuple,
        ),
    )
}

fn rc_map_invalidate(b: &ExprBuilder) -> Expr {
    let remove_and_retire = b.block(
        [Stmt::Expr(rc_map_remove_entry(
            b,
            b.loc("ents"),
            b.loc("ent"),
        ))],
        rc_retire_entry(b, b.loc("ent")),
    );
    let on_found = b.match_(
        b.loc("found"),
        [arm(b.tup_pat(&["_ek", "ent"]), remove_and_retire)],
    );
    let body = b.match_(
        rc_map_find(b, b.rget(b.loc("ents")), b.loc("k")),
        [arm(b.some_pat("found"), on_found), arm(Pat::Wild, b.unit())],
    );
    b.lam(
        &["k", "m"],
        b.match_(
            b.loc("m"),
            [arm(
                b.tup_pat(&["_l", "_i", "_c", "_e", "_p", "ents", "lk"]),
                b.app2(
                    b.global("Semaphore", "with_permit"),
                    b.loc("lk"),
                    b.task(body),
                ),
            )],
        ),
    )
}

fn rc_map_touch(b: &ExprBuilder) -> Expr {
    let bumped = b.tuple([
        b.loc("ev"),
        b.loc("eo"),
        b.loc("h"),
        b.loc("ret"),
        b.loc("cls"),
        b.add_i(b.loc("ep"), b.int(1)),
    ]);
    let on_ent = b.match_(
        b.rget(b.loc("ent")),
        [arm(
            b.tup_pat(&["ev", "eo", "h", "ret", "cls", "ep"]),
            b.block([b.rset(b.loc("ent"), bumped)], b.unit()),
        )],
    );
    let on_found = b.match_(b.loc("found"), [arm(b.tup_pat(&["_ek", "ent"]), on_ent)]);
    let body = b.match_(
        rc_map_find(b, b.rget(b.loc("ents")), b.loc("k")),
        [arm(b.some_pat("found"), on_found), arm(Pat::Wild, b.unit())],
    );
    b.lam(
        &["k", "m"],
        b.match_(
            b.loc("m"),
            [arm(
                b.tup_pat(&["_l", "_i", "_c", "_e", "_p", "ents", "_lk"]),
                body,
            )],
        ),
    )
}

pub(super) fn rc_map_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flickMake", 1) => Some(rc_map_make(b)),
        ("flickGet", 2) => Some(rc_map_get(b)),
        ("getOption", 2) => Some(rc_map_get_option(b)),
        ("flickKeys", 1) => {
            let fst = b.lam(
                &["p"],
                b.match_(b.loc("p"), [arm(b.tup_pat(&["k", "_e"]), b.loc("k"))]),
            );
            Some(b.lam(
                &["m"],
                b.match_(
                    b.loc("m"),
                    [arm(
                        b.tup_pat(&["_l", "_i", "_c", "_e", "_p", "ents", "_lk"]),
                        b.app2(b.global("List", "map"), fst, b.rget(b.loc("ents"))),
                    )],
                ),
            ))
        }
        ("has", 2) => {
            let found = b.match_(
                rc_map_find(b, b.rget(b.loc("ents")), b.loc("k")),
                [
                    arm(b.some_pat("_f"), b.bool_(true)),
                    arm(Pat::Wild, b.bool_(false)),
                ],
            );
            Some(b.lam(
                &["k", "m"],
                b.match_(
                    b.loc("m"),
                    [arm(
                        b.tup_pat(&["_l", "_i", "_c", "_e", "_p", "ents", "_lk"]),
                        found,
                    )],
                ),
            ))
        }
        ("invalidate", 2) => Some(rc_map_invalidate(b)),
        ("touch", 2) => Some(rc_map_touch(b)),
        _ => None,
    }
}
