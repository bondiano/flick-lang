//! Explicit continuation frames and machine control states for the CEK step loop.

use std::rc::Rc;

use flick_ir::{Arm, BinOp, Expr, Num, Pat, Stmt};

use crate::opt::eval::value::{DictValue, Env, Value};

/// How a [`FrameKind::Block`] binds or discards the finished statement value.
pub(super) enum BlockKind<'a> {
    Let(&'a Pat),
    Fun(&'a str),
    Discard,
}

/// Target built when a [`FrameKind::Record`] finishes evaluating its field expressions.
pub(super) enum RecordTarget<'a> {
    Record,
    Dict,
    Update(Rc<Vec<(Rc<str>, Value<'a>)>>),
}

/// Unary continuation operation on an evaluated scalar/option operand.
pub(super) enum UnaryOp {
    Neg(Num),
    Not,
    Ignore,
    Unwrap,
}

/// Explicit continuation frame carrying a single shared `env` slot (empty for
/// env-less frames) to keep `Frame` drop glue compact.
pub(super) struct Frame<'a> {
    pub(super) kind: FrameKind<'a>,
    pub(super) env: Env<'a>,
}

/// Continuation frame payloads for the CEK machine.
pub(super) enum FrameKind<'a> {
    /// Function/call return boundary for `ExprKind::Unwrap` (`o?`).
    CallBoundary,
    /// Unwinds an active top-level value evaluation on return (cycle guard).
    LeaveGlobal,
    /// Waiting for first operand of `App`; next evaluates `second` in `frame.env`.
    AppSecond { second: &'a Expr, arg_first: bool },
    /// Waiting for second operand of `App` with first operand already evaluated.
    AppReady { first: Value<'a>, arg_first: bool },
    /// Applying resolved dictionaries to an evaluated dictionary abstraction.
    ApplyDicts { dicts: Rc<Vec<DictValue<'a>>> },
    /// Conditional branch selection after evaluating condition.
    If {
        then_expr: &'a Expr,
        else_expr: &'a Expr,
    },
    /// Evaluating left operand of a strict or short-circuiting (`&&` / `||`) binary operator.
    BinaryLhs { op: BinOp, rhs: &'a Expr },
    /// Evaluating right operand of a strict binary operator.
    BinaryRhs { op: BinOp, lhs_val: Value<'a> },
    /// Unary operation (`Neg`, `Not`, `Ignore`, `Unwrap`).
    Unary(UnaryOp),
    /// Record field projection after evaluating record.
    Field { label: &'a str },
    /// Evaluating elements of a `Tuple` (`is_list: false`) or `List` (`is_list: true`).
    Seq {
        is_list: bool,
        done: Vec<Value<'a>>,
        rest: &'a [Expr],
    },
    /// Evaluating fields of a `Record`, `Dictionary`, or `Update`.
    Record {
        label: &'a str,
        done: Vec<(Rc<str>, Value<'a>)>,
        rest: &'a [(String, Expr)],
        target: RecordTarget<'a>,
    },
    /// Evaluating base of `ExprKind::Update`.
    UpdateBase { fields: &'a [(String, Expr)] },
    /// Resuming a `Block` after evaluating a `Let`, non-recursive `Fun`, or `Expr` statement.
    Block {
        kind: BlockKind<'a>,
        stmts: &'a [Stmt],
        result: &'a Expr,
    },
    /// Evaluating scrutinee of `ExprKind::Match`.
    MatchScrutinee { arms: &'a [Arm] },
    /// Evaluating an arm guard of `ExprKind::Match` (`frame.env` holds `outer_env`).
    MatchGuard {
        scrutinee: Value<'a>,
        body: &'a Expr,
        rest: &'a [Arm],
        arm_env: Env<'a>,
    },
}

pub(super) enum Control<'a> {
    Eval { expr: &'a Expr, env: Env<'a> },
    Return(Value<'a>),
}
