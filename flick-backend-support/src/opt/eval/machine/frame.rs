//! Explicit continuation frames and machine control states for the CEK step loop.

use std::rc::Rc;

use flick_ir::{Arm, BinOp, Expr, Num, Pat, Stmt};

use crate::opt::eval::value::{DictEnv, DictValue, Env, Value};

/// Explicit continuation frame for the CEK machine.
pub(super) enum Frame<'a> {
    /// Function/call return boundary for `ExprKind::Unwrap` (`o?`).
    CallBoundary,
    /// Unwinds an active top-level value evaluation on return (cycle guard).
    LeaveGlobal,
    /// Evaluating `func` of `func arg` (`arg_first: false`).
    AppFunc {
        arg: &'a Expr,
        env: Env<'a>,
        dict_env: DictEnv<'a>,
    },
    /// Evaluating `arg` of `func arg` with `func_val` already evaluated.
    AppReadyFunc { func_val: Value<'a> },
    /// Evaluating `arg` of `arg |> func` (`arg_first: true`).
    AppArgFirst {
        func: &'a Expr,
        env: Env<'a>,
        dict_env: DictEnv<'a>,
    },
    /// Evaluating `func` of `arg |> func` with `arg_val` already evaluated.
    AppReadyArg { arg_val: Value<'a> },
    /// Applying resolved dictionaries to an evaluated dictionary abstraction.
    ApplyDicts { dicts: Rc<[DictValue<'a>]> },
    /// Conditional branch selection after evaluating condition.
    If {
        then_expr: &'a Expr,
        else_expr: &'a Expr,
        env: Env<'a>,
        dict_env: DictEnv<'a>,
    },
    /// Short-circuiting `&&` / `||` after evaluating left operand.
    ShortCircuit {
        op: BinOp,
        rhs: &'a Expr,
        env: Env<'a>,
        dict_env: DictEnv<'a>,
    },
    /// Evaluating left operand of a strict binary operator.
    BinaryLhs {
        op: BinOp,
        rhs: &'a Expr,
        env: Env<'a>,
        dict_env: DictEnv<'a>,
    },
    /// Evaluating right operand of a strict binary operator.
    BinaryRhs { op: BinOp, lhs_val: Value<'a> },
    /// Unary numeric negation.
    Neg(Num),
    /// Logical negation.
    Not,
    /// Strict `ignore e` -> evaluates `e`, returns `Unit`.
    Ignore,
    /// Early-return `o?` (`ExprKind::Unwrap`).
    Unwrap,
    /// Record field projection after evaluating record.
    Field { label: &'a str },
    /// Evaluating elements of a `Tuple`.
    Tuple {
        done: Vec<Value<'a>>,
        rest: &'a [Expr],
        index: usize,
        env: Env<'a>,
        dict_env: DictEnv<'a>,
    },
    /// Evaluating elements of a `List`.
    List {
        done: Vec<Value<'a>>,
        rest: &'a [Expr],
        index: usize,
        env: Env<'a>,
        dict_env: DictEnv<'a>,
    },
    /// Evaluating fields of a `Record` or `Dictionary`.
    Record {
        done: Vec<(Rc<str>, Value<'a>)>,
        rest: &'a [(String, Expr)],
        index: usize,
        as_dict: bool,
        env: Env<'a>,
        dict_env: DictEnv<'a>,
    },
    /// Evaluating base of `ExprKind::Update`.
    UpdateBase {
        fields: &'a [(String, Expr)],
        env: Env<'a>,
        dict_env: DictEnv<'a>,
    },
    /// Evaluating updated fields of `ExprKind::Update`.
    UpdateFields {
        base: Rc<[(Rc<str>, Value<'a>)]>,
        done: Vec<(Rc<str>, Value<'a>)>,
        rest: &'a [(String, Expr)],
        index: usize,
        env: Env<'a>,
        dict_env: DictEnv<'a>,
    },
    /// Binding the result of `Stmt::Let(pat, rhs)` inside a `Block`.
    BlockLet {
        pat: &'a Pat,
        stmts: &'a [Stmt],
        next_index: usize,
        result: &'a Expr,
        env: Env<'a>,
        dict_env: DictEnv<'a>,
    },
    /// Binding a non-recursive `Stmt::Fun` inside a `Block`.
    BlockFun {
        name: &'a str,
        stmts: &'a [Stmt],
        next_index: usize,
        result: &'a Expr,
        env: Env<'a>,
        dict_env: DictEnv<'a>,
    },
    /// Discarding the value of `Stmt::Expr(e)` inside a `Block`.
    BlockDiscard {
        stmts: &'a [Stmt],
        next_index: usize,
        result: &'a Expr,
        env: Env<'a>,
        dict_env: DictEnv<'a>,
    },
    /// Evaluating scrutinee of `ExprKind::Match`.
    MatchScrutinee {
        arms: &'a [Arm],
        env: Env<'a>,
        dict_env: DictEnv<'a>,
    },
    /// Evaluating an arm guard of `ExprKind::Match`.
    MatchGuard {
        scrutinee: Value<'a>,
        arms: &'a [Arm],
        arm_idx: usize,
        arm_env: Env<'a>,
        outer_env: Env<'a>,
        dict_env: DictEnv<'a>,
    },
}

pub(super) enum Control<'a> {
    Eval {
        expr: &'a Expr,
        env: Env<'a>,
        dict_env: DictEnv<'a>,
    },
    Return(Value<'a>),
    BlockStep {
        stmts: &'a [Stmt],
        index: usize,
        result: &'a Expr,
        env: Env<'a>,
        dict_env: DictEnv<'a>,
    },
    MatchArm {
        scrutinee: Value<'a>,
        arms: &'a [Arm],
        arm_idx: usize,
        outer_env: Env<'a>,
        dict_env: DictEnv<'a>,
    },
}

pub(super) enum StepResult<'a> {
    Continue(Control<'a>),
    Done(Value<'a>),
}
