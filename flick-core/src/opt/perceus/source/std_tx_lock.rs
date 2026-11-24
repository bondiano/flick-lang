//! Audited `TxSemaphore` and `TxReentrantLock` pure IR adapters (Stage 4C).

mod reentrant;

use flick_ir::{BinOp, Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::{Code, Env, Expr, Local, Lower, LowerError, Span, Stmt};

impl Lower<'_> {
    pub(super) fn std_tx_lock_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 || value.mode != ForeignMode::Effect || !has_standard_shape(value) {
            return None;
        }
        let (from, bind) = value.binding.module_export()?;
        let stages = value.stages.len();
        let b = ExprBuilder::new(span, "$txl");
        let expr = match from {
            "flick:TxSemaphore" => tx_sem_template(&b, bind, stages)?,
            "flick:TxReentrantLock" => reentrant::tx_lock_template(&b, bind, stages)?,
            _ => return None,
        };
        Some(self.expr(&expr, &Env::new(), block))
    }
}

// TxSemaphore: `(cap : Int, ref : TxRef Int)`
fn tx_sem_split(b: &ExprBuilder, body: Expr) -> Expr {
    b.match_(b.loc("s"), vec![arm(b.tup_pat(&["cap", "r"]), body)])
}

fn tx_sem_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    tx_sem_acquire_template(b, bind, stages)
        .or_else(|| tx_sem_release_and_scoped_template(b, bind, stages))
}

fn tx_sem_acquire_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("make", 1) => Some(b.lam(
            &["n"],
            b.if_(
                b.bin(BinOp::Lt, b.loc("n"), b.int(0)),
                b.app(
                    b.global("Effect.Core", "die"),
                    b.str_("Permits must be non-negative"),
                ),
                b.tuple(vec![
                    b.loc("n"),
                    b.app(b.global("TxRef", "make"), b.loc("n")),
                ]),
            ),
        )),
        ("available", 1) => Some(b.lam(
            &["s"],
            tx_sem_split(b, b.app(b.global("TxRef", "get"), b.loc("r"))),
        )),
        ("capacity", 1) => Some(b.lam(&["s"], tx_sem_split(b, b.loc("cap")))),
        ("acquire", 1) => Some(b.lam(
            &["s"],
            b.app2(b.global("TxSemaphore", "acquire_n"), b.int(1), b.loc("s")),
        )),
        ("acquireN", 2) => Some(b.lam(
            &["n", "s"],
            b.app(
                b.global("Tx", "atomically"),
                b.task(tx_sem_split(
                    b,
                    b.block(
                        vec![b.let_("p", b.app(b.global("TxRef", "get"), b.loc("r")))],
                        b.if_(
                            b.bin(BinOp::Lt, b.loc("p"), b.loc("n")),
                            b.app(b.global("Tx", "retry"), b.unit()),
                            b.app2(
                                b.global("TxRef", "set"),
                                b.sub_i(b.loc("p"), b.loc("n")),
                                b.loc("r"),
                            ),
                        ),
                    ),
                )),
            ),
        )),
        ("tryAcquire", 1) => Some(b.lam(
            &["s"],
            b.app2(
                b.global("TxSemaphore", "try_acquire_n"),
                b.int(1),
                b.loc("s"),
            ),
        )),
        ("tryAcquireN", 2) => Some(b.lam(
            &["n", "s"],
            tx_sem_split(
                b,
                b.app2(
                    b.global("TxRef", "modify"),
                    b.lam(
                        &["p"],
                        b.if_(
                            b.bin(BinOp::Ge, b.loc("p"), b.loc("n")),
                            b.tuple(vec![b.bool_(true), b.sub_i(b.loc("p"), b.loc("n"))]),
                            b.tuple(vec![b.bool_(false), b.loc("p")]),
                        ),
                    ),
                    b.loc("r"),
                ),
            ),
        )),
        _ => None,
    }
}

fn tx_sem_release_and_scoped_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("release", 1) => Some(b.lam(
            &["s"],
            b.app2(b.global("TxSemaphore", "release_n"), b.int(1), b.loc("s")),
        )),
        ("releaseN", 2) => Some(b.lam(
            &["n", "s"],
            tx_sem_split(
                b,
                b.app2(
                    b.global("TxRef", "update"),
                    b.lam(
                        &["p"],
                        b.block(
                            vec![b.let_("sum", b.add_i(b.loc("p"), b.loc("n")))],
                            b.if_(
                                b.bin(BinOp::Gt, b.loc("sum"), b.loc("cap")),
                                b.loc("cap"),
                                b.loc("sum"),
                            ),
                        ),
                    ),
                    b.loc("r"),
                ),
            ),
        )),
        ("withPermit", 2) => Some(b.lam(
            &["s", "body"],
            b.app3(
                b.global("TxSemaphore", "with_permits"),
                b.loc("s"),
                b.int(1),
                b.loc("body"),
            ),
        )),
        ("withPermits", 3) => Some(b.lam(
            &["s", "n", "body"],
            b.block(
                vec![Stmt::Expr(b.app(
                    b.global("Tx", "atomically"),
                    b.task(b.app2(b.global("TxSemaphore", "acquire_n"), b.loc("n"), b.loc("s"))),
                ))],
                b.app2(
                    b.global("Effect.Errors", "ensuring"),
                    b.task(b.app(
                        b.global("Tx", "atomically"),
                        b.task(b.app2(
                            b.global("TxSemaphore", "release_n"),
                            b.loc("n"),
                            b.loc("s"),
                        )),
                    )),
                    b.loc("body"),
                ),
            ),
        )),
        ("withPermitScoped", 1) => Some(b.lam(
            &["s"],
            b.block(
                vec![
                    Stmt::Expr(b.app(
                        b.global("Tx", "atomically"),
                        b.task(b.app(b.global("TxSemaphore", "acquire"), b.loc("s"))),
                    )),
                    Stmt::Expr(b.app(
                        b.global("Scope", "add_finalizer"),
                        b.task(b.app(
                            b.global("Tx", "atomically"),
                            b.task(b.app(b.global("TxSemaphore", "release"), b.loc("s"))),
                        )),
                    )),
                ],
                b.unit(),
            ),
        )),
        _ => None,
    }
}
