//! Lowered specialized IR with explicit `Box`/`Unbox` boundary coercions,
//! direct vs. fallback call targets, and offset vs. label record accesses.

use flick_ir::{BinOp, Constructor, Lift, Lit, Num, OpClass, Span, Stage, TypeRef};

use super::types::{SymbolId, ValueRep};

/// One expression in a [`super::SpecializedProgram`].
#[derive(Debug, Clone, PartialEq)]
pub struct SpecExpr {
    pub kind: SpecExprKind,
    pub span: Span,
    /// Concrete monomorphic type when unboxed, or `None` / polymorphic in
    /// fallback code.
    pub ty: Option<TypeRef>,
    /// Whether this expression produces an unboxed concrete value or a
    /// uniform boxed cell.
    pub rep: ValueRep,
}

impl SpecExpr {
    pub fn unboxed(kind: SpecExprKind, span: Span, ty: Option<TypeRef>) -> SpecExpr {
        let rep = match &ty {
            Some(t) => ValueRep::Unboxed(t.clone()),
            None => ValueRep::Boxed,
        };
        SpecExpr {
            kind,
            span,
            ty,
            rep,
        }
    }

    pub fn boxed(kind: SpecExprKind, span: Span, ty: Option<TypeRef>) -> SpecExpr {
        SpecExpr {
            kind,
            span,
            ty,
            rep: ValueRep::Boxed,
        }
    }
}

/// Expression variants of the specialized IR.
#[derive(Debug, Clone, PartialEq)]
pub enum SpecExprKind {
    /// Explicit boundary coercion: wraps an unboxed concrete value (`from_ty`)
    /// into a uniform boxed cell when entering fallback code, a polymorphic
    /// `foreign` binding, a runtime dictionary call, or an existential/GADT
    /// payload.
    Box {
        expr: Box<SpecExpr>,
        from_ty: Option<TypeRef>,
    },
    /// Explicit boundary coercion: unwraps a uniform boxed cell back into a
    /// concrete unboxed value (`to_ty`) when returning to specialized code.
    Unbox {
        expr: Box<SpecExpr>,
        to_ty: TypeRef,
    },
    /// Reference to a specialized monomorphic definition (or devirtualized
    /// class instance method), optionally supplied with remaining runtime
    /// dictionaries.
    CallDirect {
        target: SymbolId,
        dicts: Vec<SpecDict>,
    },
    /// Reference to a uniform boxed-fallback definition compiled once over
    /// boxed cells.
    CallFallback {
        target: SymbolId,
        dicts: Vec<SpecDict>,
    },
    /// Reference to a `foreign` binding (monomorphic when `polymorphic == false`,
    /// uniform boxed-cell ABI when `polymorphic == true`).
    CallForeign {
        target: SymbolId,
        module: String,
        name: String,
        polymorphic: bool,
        dicts: Vec<SpecDict>,
    },
    /// Unresolved/runtime class dictionary method lookup (preserved when exact
    /// dictionary evidence is not statically known).
    MethodCall {
        dict: SpecDict,
        name: String,
    },
    /// Closed-record field access resolved to a concrete layout index and byte
    /// offset (specialized code only).
    FieldOffset {
        record: Box<SpecExpr>,
        label: String,
        index: usize,
        offset: u64,
        size: u64,
    },
    /// Open or boxed record field access performed by label (fallback code).
    FieldByLabel {
        record: Box<SpecExpr>,
        label: String,
    },
    /// Closed-record update with concrete layout indices and byte offsets.
    UpdateOffset {
        record: Box<SpecExpr>,
        fields: Vec<(String, usize, u64, SpecExpr)>,
    },
    /// Open or boxed record update performed by label.
    UpdateByLabel {
        record: Box<SpecExpr>,
        fields: Vec<(String, SpecExpr)>,
    },
    /// Constructor value/function, tagged with whether its fields are unboxed
    /// (with concrete layout) or uniform boxed cells.
    Construct {
        ctor: Constructor,
        boxed: bool,
        dicts: Vec<SpecDict>,
    },
    Lit(Lit),
    Local(String),
    Var(String),
    Perform {
        effect: String,
        op: String,
        stages: Vec<Stage>,
        dicts: Vec<SpecDict>,
    },
    ServiceTag(String),
    Label(String),
    Generate {
        body: Box<SpecExpr>,
        strict: bool,
        sync: bool,
        limit: Option<Box<SpecExpr>>,
    },
    Each {
        stop: Option<Box<SpecExpr>>,
        pat: SpecPat,
        binder_types: Vec<(String, ValueRep)>,
        source: Box<SpecExpr>,
        body: Box<SpecExpr>,
    },
    Yield(Box<SpecExpr>),
    Loop {
        condition: Box<SpecExpr>,
        body: Box<SpecExpr>,
    },
    ControlLoop {
        id: String,
        condition: Option<Box<SpecExpr>>,
        body: Box<SpecExpr>,
        iterable: bool,
    },
    LoopExit {
        id: String,
        value: Option<Box<SpecExpr>>,
    },
    /// Passing runtime dictionaries to a local/higher-rank value expression.
    Dicts(Box<SpecExpr>, Vec<SpecDict>),
    /// Abstracting runtime dictionary parameters (e.g. higher-rank callback).
    Abstract(Vec<String>, Box<SpecExpr>),
    App {
        func: Box<SpecExpr>,
        arg: Box<SpecExpr>,
        stage: Stage,
        arg_first: bool,
    },
    Lambda {
        params: Vec<SpecParam>,
        body: Box<SpecExpr>,
    },
    Block(Vec<SpecStmt>, Box<SpecExpr>),
    If(Box<SpecExpr>, Box<SpecExpr>, Box<SpecExpr>),
    Match(Box<SpecExpr>, Vec<SpecArm>),
    Tuple(Vec<SpecExpr>),
    List(Vec<SpecExpr>),
    Record {
        fields: Vec<(String, SpecExpr)>,
        /// `true` when represented as a boxed label-indexed record rather than
        /// an unboxed closed-layout struct.
        boxed: bool,
    },
    /// Runtime class dictionary value (`Show.dictionary { ... }`), whose
    /// methods follow the uniform boxed-cell ABI.
    Dictionary(Vec<(String, SpecExpr)>),
    Binary(BinOp, Box<SpecExpr>, Box<SpecExpr>),
    Not(Box<SpecExpr>),
    Unwrap(Box<SpecExpr>),
    Neg(Num, Box<SpecExpr>),
    Ignore(Box<SpecExpr>),
    Await(Box<SpecExpr>),
    Task(Box<SpecExpr>),
    Within(String, Box<SpecExpr>),
    ReadOwnerEvidence,
    WithOwnerEvidence(Box<SpecExpr>, Box<SpecExpr>),
    Handle(Box<SpecHandler>),
    Lift(Box<SpecExpr>, Lift),
}

/// Runtime dictionary expression preserved in the specialized IR.
#[derive(Debug, Clone, PartialEq)]
pub enum SpecDict {
    /// Reference to a lowered runtime class instance table, applied to its
    /// context dictionaries.
    Instance {
        target: SymbolId,
        module: String,
        name: String,
        args: Vec<SpecDict>,
    },
    /// A runtime dictionary parameter of the enclosing definition or `Abstract`.
    Given(String),
    /// A top-level named dictionary value (`Dict::Value`).
    Value { target: SymbolId },
    /// A dictionary field of a lexically opened packed module (`unpack`).
    Opened { binder: String, field: String },
    /// A dictionary unpacked from a GADT constructor pattern.
    Field(String),
    /// Superclass projection from another runtime dictionary.
    Super { dict: Box<SpecDict>, class: String },
    /// Compiler-owned structural dictionary for records/tuples.
    Structural {
        class: String,
        fields: Vec<SpecDict>,
    },
    /// Row primitive dictionary.
    Primitive {
        constraint: String,
        labels: Vec<String>,
    },
    /// Call-site metadata dictionary.
    Site { place: String, definition: String },
    /// Error placeholder dictionary.
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpecParam {
    pub pat: SpecPat,
    pub stage: Stage,
    pub rep: ValueRep,
    pub binder_reps: Vec<(String, ValueRep)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SpecStmt {
    Let {
        pat: SpecPat,
        binder_reps: Vec<(String, ValueRep)>,
        value: SpecExpr,
    },
    Var {
        name: String,
        rep: ValueRep,
        value: SpecExpr,
    },
    Assign(String, SpecExpr),
    Fun {
        name: String,
        rec: bool,
        dicts: Vec<String>,
        rep: ValueRep,
        boxed_fallback: bool,
        value: SpecExpr,
    },
    Expr(SpecExpr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpecArm {
    pub pat: SpecPat,
    pub binder_reps: Vec<(String, ValueRep)>,
    pub guard: Option<SpecExpr>,
    pub body: SpecExpr,
}

/// One field entry inside [`SpecPat::Record`]: `(label, optional (index, offset), pattern)`.
pub type RecordPatField = (String, Option<(usize, u64)>, SpecPat);

#[derive(Debug, Clone, PartialEq)]
pub enum SpecPat {
    Wild,
    Bind(String),
    Lit(Lit),
    Tuple(Vec<SpecPat>),
    List(Vec<SpecPat>, Option<Box<SpecPat>>),
    /// Record pattern: `boxed == false` reads by concrete layout offset;
    /// `boxed == true` reads by label.
    Record {
        fields: Vec<RecordPatField>,
        boxed: bool,
    },
    Con {
        ctor: Constructor,
        boxed: bool,
        args: Vec<SpecPat>,
    },
    Or(Vec<SpecPat>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpecHandler {
    pub effects: Vec<SpecHandled>,
    pub body: SpecExpr,
    pub ret: Option<SpecExpr>,
    pub errors: Vec<SpecArm>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpecHandled {
    pub effect: String,
    pub operations: Vec<SpecOpArm>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpecOpArm {
    pub name: String,
    pub class: OpClass,
    pub dicts: usize,
    pub arity: usize,
    pub covers: bool,
    pub function: SpecExpr,
}

/// Structural counts accumulated over a lowered [`SpecExpr`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ExprCounts {
    pub box_count: usize,
    pub unbox_count: usize,
    pub runtime_dict_calls: usize,
    pub offset_field_reads: usize,
    pub label_field_reads: usize,
    pub node_count: usize,
}

impl ExprCounts {
    /// Deterministic byte estimate of the emitted IR (used for budget checks
    /// and [`super::SpecializeStats::estimated_output_bytes`]).
    pub(crate) fn estimated_bytes(&self) -> usize {
        self.node_count.saturating_mul(32)
    }
}

/// Recursively counts nodes, boxing coercions, runtime dictionary calls, and
/// record field access modes in `expr`.
#[allow(clippy::too_many_lines)]
pub(crate) fn count_expr(expr: &SpecExpr, out: &mut ExprCounts) {
    out.node_count += 1;
    match &expr.kind {
        SpecExprKind::Box { expr: inner, .. } => {
            out.box_count += 1;
            count_expr(inner, out);
        }
        SpecExprKind::Unbox { expr: inner, .. } => {
            out.unbox_count += 1;
            count_expr(inner, out);
        }
        SpecExprKind::MethodCall { .. } => {
            out.runtime_dict_calls += 1;
        }
        SpecExprKind::FieldOffset { record, .. } => {
            out.offset_field_reads += 1;
            count_expr(record, out);
        }
        SpecExprKind::FieldByLabel { record, .. } => {
            out.label_field_reads += 1;
            count_expr(record, out);
        }
        SpecExprKind::UpdateOffset { record, fields } => {
            out.offset_field_reads += fields.len();
            count_expr(record, out);
            for (_, _, _, v) in fields {
                count_expr(v, out);
            }
        }
        SpecExprKind::UpdateByLabel { record, fields } => {
            out.label_field_reads += fields.len();
            count_expr(record, out);
            for (_, v) in fields {
                count_expr(v, out);
            }
        }
        SpecExprKind::App { func, arg, .. } => {
            count_expr(func, out);
            count_expr(arg, out);
        }
        SpecExprKind::Lambda { body, .. }
        | SpecExprKind::Abstract(_, body)
        | SpecExprKind::Dicts(body, _)
        | SpecExprKind::Not(body)
        | SpecExprKind::Unwrap(body)
        | SpecExprKind::Neg(_, body)
        | SpecExprKind::Ignore(body)
        | SpecExprKind::Await(body)
        | SpecExprKind::Task(body)
        | SpecExprKind::Within(_, body)
        | SpecExprKind::Lift(body, _)
        | SpecExprKind::Yield(body) => count_expr(body, out),
        SpecExprKind::Block(stmts, tail) => {
            for stmt in stmts {
                match stmt {
                    SpecStmt::Let { value, .. }
                    | SpecStmt::Var { value, .. }
                    | SpecStmt::Assign(_, value)
                    | SpecStmt::Fun { value, .. }
                    | SpecStmt::Expr(value) => count_expr(value, out),
                }
            }
            count_expr(tail, out);
        }
        SpecExprKind::If(c, t, e) => {
            count_expr(c, out);
            count_expr(t, out);
            count_expr(e, out);
        }
        SpecExprKind::Match(s, arms) => {
            count_expr(s, out);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    count_expr(g, out);
                }
                count_expr(&arm.body, out);
            }
        }
        SpecExprKind::Tuple(items) | SpecExprKind::List(items) => {
            for item in items {
                count_expr(item, out);
            }
        }
        SpecExprKind::Record { fields, .. } | SpecExprKind::Dictionary(fields) => {
            for (_, v) in fields {
                count_expr(v, out);
            }
        }
        SpecExprKind::Binary(_, l, r) | SpecExprKind::WithOwnerEvidence(l, r) => {
            count_expr(l, out);
            count_expr(r, out);
        }
        SpecExprKind::Generate { body, limit, .. }
        | SpecExprKind::ControlLoop {
            body,
            condition: limit,
            ..
        } => {
            if let Some(l) = limit {
                count_expr(l, out);
            }
            count_expr(body, out);
        }
        SpecExprKind::Each {
            stop, source, body, ..
        } => {
            if let Some(s) = stop {
                count_expr(s, out);
            }
            count_expr(source, out);
            count_expr(body, out);
        }
        SpecExprKind::Loop { condition, body } => {
            count_expr(condition, out);
            count_expr(body, out);
        }
        SpecExprKind::LoopExit { value, .. } => {
            if let Some(v) = value {
                count_expr(v, out);
            }
        }
        SpecExprKind::Handle(h) => {
            for eff in &h.effects {
                for op in &eff.operations {
                    count_expr(&op.function, out);
                }
            }
            count_expr(&h.body, out);
            if let Some(ret) = &h.ret {
                count_expr(ret, out);
            }
            for arm in &h.errors {
                if let Some(g) = &arm.guard {
                    count_expr(g, out);
                }
                count_expr(&arm.body, out);
            }
        }
        SpecExprKind::CallDirect { .. }
        | SpecExprKind::CallFallback { .. }
        | SpecExprKind::CallForeign { .. }
        | SpecExprKind::Construct { .. }
        | SpecExprKind::Lit(_)
        | SpecExprKind::Local(_)
        | SpecExprKind::Var(_)
        | SpecExprKind::Perform { .. }
        | SpecExprKind::ServiceTag(_)
        | SpecExprKind::Label(_)
        | SpecExprKind::ReadOwnerEvidence => {}
    }
}
