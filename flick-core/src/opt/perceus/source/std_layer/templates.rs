//! Pure `Layer` AST template builders (`sync`, `effect`, `provide`, `catch`, `tap`).

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, ExprKind, Pat};

fn build_memo(b: &ExprBuilder, layer: Expr) -> Expr {
    b.app2(b.global("Layer", "build_with_memo_map"), b.loc("m"), layer)
}

fn from_build_catch(b: &ExprBuilder, on_cause: Expr) -> Expr {
    let build = b.lam(
        &["m"],
        b.app2(
            b.global("Effect.Errors", "catch_cause"),
            on_cause,
            b.at(ExprKind::Task(Box::new(build_memo(b, b.loc("self"))))),
        ),
    );
    b.lam(
        &["f", "self"],
        b.app(b.global("Layer", "from_build"), build),
    )
}

fn layer_effect_builders(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("effect", 2) => {
            let build = b.lam(
                &["_m"],
                b.app2(
                    b.global("Context", "make"),
                    b.loc("tag"),
                    b.at(ExprKind::Await(Box::new(b.loc("t")))),
                ),
            );
            Some(b.lam(
                &["tag", "t"],
                b.app(b.global("Layer", "from_build_memo"), build),
            ))
        }
        ("effectContext", 1) => Some(b.lam(
            &["t"],
            b.app(
                b.global("Layer", "from_build_memo"),
                b.lam(&["_m"], b.at(ExprKind::Await(Box::new(b.loc("t"))))),
            ),
        )),
        ("effectDiscard", 1) => {
            let build = b.lam(
                &["_m"],
                b.block(
                    [b.let_("_u", b.at(ExprKind::Await(Box::new(b.loc("t")))))],
                    b.app(b.global("Context", "empty"), b.unit()),
                ),
            );
            Some(b.lam(&["t"], b.app(b.global("Layer", "from_build_memo"), build)))
        }
        ("suspend", 1) => Some(b.lam(
            &["f"],
            b.app(
                b.global("Layer", "from_build_memo"),
                b.lam(&["m"], build_memo(b, b.app(b.loc("f"), b.unit()))),
            ),
        )),
        ("unwrap", 1) => Some(b.lam(
            &["t"],
            b.app(
                b.global("Layer", "from_build"),
                b.lam(
                    &["m"],
                    build_memo(b, b.at(ExprKind::Await(Box::new(b.loc("t"))))),
                ),
            ),
        )),
        _ => None,
    }
}

pub(super) fn pure_layer_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    if let Some(expr) = layer_effect_builders(b, bind, stages) {
        return Some(expr);
    }
    match (bind, stages) {
        ("sync", 2) => {
            let build = b.lam(
                &["_m"],
                b.app2(
                    b.global("Context", "make"),
                    b.loc("tag"),
                    b.app(b.loc("f"), b.unit()),
                ),
            );
            Some(b.lam(
                &["tag", "f"],
                b.app(b.global("Layer", "from_build_memo"), build),
            ))
        }
        ("syncContext", 1) => Some(b.lam(
            &["f"],
            b.app(
                b.global("Layer", "from_build_memo"),
                b.lam(&["_m"], b.app(b.loc("f"), b.unit())),
            ),
        )),
        ("flatMap", 2) => {
            let build = b.lam(
                &["m"],
                b.block(
                    [b.let_("s", build_memo(b, b.loc("self")))],
                    build_memo(b, b.app(b.loc("f"), b.loc("s"))),
                ),
            );
            Some(b.lam(
                &["f", "self"],
                b.app(b.global("Layer", "from_build"), build),
            ))
        }
        ("tap", 2) => {
            let build = b.lam(
                &["m"],
                b.block(
                    [
                        b.let_("s", build_memo(b, b.loc("self"))),
                        b.let_("_u", b.app(b.loc("f"), b.loc("s"))),
                    ],
                    b.loc("s"),
                ),
            );
            Some(b.lam(
                &["f", "self"],
                b.app(b.global("Layer", "from_build"), build),
            ))
        }
        ("tapError", 2) => Some(layer_tap_error_template(b)),
        ("flickTapCause", 2) => Some(layer_tap_cause_template(b)),
        ("catch", 2) => Some(layer_catch_template(b)),
        ("flickCatchCause", 2) => Some(layer_catch_cause_template(b)),
        _ => None,
    }
}

pub(super) fn layer_provide_template(b: &ExprBuilder, merge: bool) -> Expr {
    let build_self = b.at(ExprKind::Task(Box::new(build_memo(b, b.loc("self")))));
    let out_expr = b.app2(b.loc("prov"), b.loc("inputs"), build_self);
    let tail = if merge {
        b.app2(b.global("Context", "merge"), out_expr, b.loc("inputs"))
    } else {
        out_expr
    };
    let build = b.lam(
        &["m"],
        b.block([b.let_("inputs", build_memo(b, b.loc("deps")))], tail),
    );
    b.lam(
        &["prov", "deps", "self"],
        b.app(b.global("Layer", "from_build"), build),
    )
}

pub(super) fn layer_update_service_template(b: &ExprBuilder) -> Expr {
    let build_self = b.at(ExprKind::Task(Box::new(build_memo(b, b.loc("self")))));
    let ctx = b.app2(
        b.global("Context", "make"),
        b.loc("tag"),
        b.app(b.loc("f"), b.loc("cur")),
    );
    let build = b.lam(
        &["m"],
        b.block(
            [
                b.let_("cur", b.app(b.loc("lookup"), b.loc("tag"))),
                b.let_("ctx", ctx),
            ],
            b.app2(b.loc("prov"), b.loc("ctx"), build_self),
        ),
    );
    b.lam(
        &["prov", "lookup", "tag", "f", "self"],
        b.app(b.global("Layer", "from_build"), build),
    )
}

fn layer_tap_error_template(b: &ExprBuilder) -> Expr {
    let tap = b.match_(
        b.app(b.global("Cause", "find_error_option"), b.loc("c")),
        [
            arm(
                b.some_pat("e"),
                b.block([b.let_("_x", b.app(b.loc("f"), b.loc("e")))], b.unit()),
            ),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let on_cause = b.lam(
        &["c"],
        b.block(
            [b.let_("_u", tap)],
            b.app(b.global("Failure", "rethrow"), b.loc("c")),
        ),
    );
    from_build_catch(b, on_cause)
}

fn layer_tap_cause_template(b: &ExprBuilder) -> Expr {
    let on_cause = b.lam(
        &["c"],
        b.block(
            [b.let_("_u", b.app(b.loc("f"), b.loc("c")))],
            b.app(b.global("Failure", "rethrow"), b.loc("c")),
        ),
    );
    from_build_catch(b, on_cause)
}

fn layer_catch_template(b: &ExprBuilder) -> Expr {
    let on_cause = b.lam(
        &["c"],
        b.match_(
            b.app(b.global("Cause", "find_error_option"), b.loc("c")),
            [
                arm(
                    b.some_pat("e"),
                    build_memo(b, b.app(b.loc("f"), b.loc("e"))),
                ),
                arm(Pat::Wild, b.app(b.global("Failure", "rethrow"), b.loc("c"))),
            ],
        ),
    );
    from_build_catch(b, on_cause)
}

fn layer_catch_cause_template(b: &ExprBuilder) -> Expr {
    let on_cause = b.lam(&["c"], build_memo(b, b.app(b.loc("f"), b.loc("c"))));
    from_build_catch(b, on_cause)
}
