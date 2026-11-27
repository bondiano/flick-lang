//! Bytecode instruction set, prepared image, and structured preparation errors.

use std::collections::BTreeMap;
use std::{error, fmt};

use flick_core::ir::memory::{
    self, AsyncIntrinsic, CellLayout, CliIntrinsic, CodeId, HostServiceIntrinsic, Intrinsic,
    LayerIntrinsic, MemorySpan, Name, ValueRep,
};
use flick_core::ir::{BinOp, Pat};
use flick_core::numeric::Num;
use flick_core::types::OpClass;

use crate::tier0::Value;

/// Maximum argument count for in-place static tail-call frame reuse.
pub(crate) const MAX_TAIL_ARGS: usize = 8;

/// Summary statistics for a prepared tier-1 bytecode image.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PreparedStats {
    pub functions: usize,
    pub instructions: usize,
    pub fused_compute: usize,
    pub fused_compare_branch: usize,
    pub fused_update_cell: usize,
    pub fused_match_flag: usize,
    pub fused_tail_calls: usize,
}

/// Interned effect operation descriptor for fast handler lookup and caching.
#[derive(Debug, Clone)]
pub(crate) struct PreparedOp {
    pub(crate) effect: String,
    pub(crate) op: String,
    pub(crate) console_fallback: Option<Intrinsic>,
}

/// Compiled operation entry installed by [`Instr::Handle`].
#[derive(Debug, Clone)]
pub(crate) struct CompiledHandlerOp {
    pub(crate) op_id: usize,
    pub(crate) class: OpClass,
    pub(crate) dicts: usize,
    pub(crate) arity: usize,
    pub(crate) function: usize,
}

/// A validated, bytecode-compiled tier-1 VM image rooted at one entry global.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub(crate) functions: Vec<CompiledFunction>,
    pub(crate) ir_codes: Vec<CodeId>,
    pub(crate) code_indices: BTreeMap<CodeId, usize>,
    pub(crate) globals: Vec<usize>,
    pub(crate) global_names: Vec<Name>,
    pub(crate) entry: usize,
    pub(crate) root: Option<usize>,
    pub(crate) tags: Vec<String>,
    pub(crate) dictionary_fields: Vec<String>,
    pub(crate) loops: Vec<String>,
    pub(crate) operations: Vec<PreparedOp>,
}

impl Prepared {
    /// Returns the source span preserved for bytecode instruction `(code, pc)`, if any.
    #[must_use]
    pub(crate) fn span(&self, code: usize, pc: usize) -> Option<MemorySpan> {
        self.functions
            .get(code)
            .and_then(|function| function.spans.get(pc).copied().flatten())
    }

    /// Returns instruction and fusion counts across all reachable functions.
    #[must_use]
    pub fn stats(&self) -> PreparedStats {
        let mut stats = PreparedStats {
            functions: self.functions.len(),
            ..PreparedStats::default()
        };
        for function in &self.functions {
            stats.instructions += function.instructions.len();
            for instr in &function.instructions {
                match instr {
                    Instr::Compute { .. } => stats.fused_compute += 1,
                    Instr::CompareBranch { .. } => stats.fused_compare_branch += 1,
                    Instr::UpdateCell { .. } => stats.fused_update_cell += 1,
                    Instr::MatchFlag { .. } | Instr::MatchBranch { .. } => {
                        stats.fused_match_flag += 1;
                    }
                    Instr::TailCall { .. } | Instr::TailApply { .. } => {
                        stats.fused_tail_calls += 1;
                    }
                    _ => {}
                }
            }
        }
        stats
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CompiledFunction {
    pub(crate) params: Vec<usize>,
    pub(crate) slots: usize,
    pub(crate) instructions: Vec<Instr>,
    pub(crate) spans: Vec<Option<MemorySpan>>,
}

/// Copyable immediate scalar values stored directly in bytecode instructions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Immediate {
    Unit,
    Bool(bool),
    Int(i32),
    Int8(i8),
    Int16(i16),
    Int64(i64),
    UInt8(u8),
    UInt16(u16),
    UInt32(u32),
    UInt64(u64),
    Float(f64),
    Float32(f32),
}

impl Immediate {
    pub(crate) const fn to_value(self) -> Value {
        match self {
            Self::Unit => Value::Unit,
            Self::Bool(v) => Value::Bool(v),
            Self::Int(v) => Value::Int(v),
            Self::Int8(v) => Value::Int8(v),
            Self::Int16(v) => Value::Int16(v),
            Self::Int64(v) => Value::Int64(v),
            Self::UInt8(v) => Value::UInt8(v),
            Self::UInt16(v) => Value::UInt16(v),
            Self::UInt32(v) => Value::UInt32(v),
            Self::UInt64(v) => Value::UInt64(v),
            Self::Float(v) => Value::Float(v),
            Self::Float32(v) => Value::Float32(v),
        }
    }
}

/// Source operand for fused arithmetic, comparison, and in-place cell updates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ValueSource {
    Slot(usize),
    Cell(usize),
    Immediate(Immediate),
}

/// Register-machine bytecode instruction for tier 1.
#[derive(Debug, Clone)]
pub(crate) enum Instr {
    Literal {
        dst: usize,
        value: Immediate,
    },
    String {
        dst: usize,
        value: String,
    },
    BigInt {
        dst: usize,
        value: String,
    },
    Label {
        dst: usize,
        value: String,
    },
    ServiceTag {
        dst: usize,
        key: String,
    },
    ReadOwnerEvidence {
        dst: usize,
    },
    Dup {
        src: usize,
        dst: usize,
    },
    Drop {
        src: usize,
    },
    Binary {
        dst: usize,
        op: BinOp,
        left: usize,
        right: usize,
    },
    Compute {
        dst: usize,
        op: BinOp,
        left: ValueSource,
        right: ValueSource,
    },
    Not {
        dst: usize,
        src: usize,
    },
    Neg {
        dst: usize,
        num: Num,
        src: usize,
    },
    Primitive {
        dst: usize,
        kind: Intrinsic,
        args: Vec<usize>,
    },
    StringPredicate {
        dst: usize,
        kind: Intrinsic,
        needle: usize,
        subject: usize,
    },
    HostIntrinsic {
        dst: usize,
        kind: Intrinsic,
        src: usize,
    },
    HostService {
        dst: usize,
        op: HostServiceIntrinsic,
        args: Vec<usize>,
    },
    Cli {
        dst: usize,
        op: CliIntrinsic,
        args: Vec<usize>,
    },
    Construct {
        dst: usize,
        tag: usize,
        fields: Vec<usize>,
        dicts: Vec<usize>,
        rep: ValueRep,
        layout: Option<CellLayout>,
    },
    DropReuse {
        dst: usize,
        object: usize,
    },
    Reuse {
        dst: usize,
        object: usize,
        tag: usize,
        fields: Vec<usize>,
        dicts: Vec<usize>,
        layout: Option<CellLayout>,
    },
    ReuseRecord {
        dst: usize,
        object: usize,
        fields: Vec<(String, usize)>,
    },
    BoxAggregate {
        dst: usize,
        value: usize,
        layout: Option<CellLayout>,
    },
    UnboxBorrow {
        dst: usize,
        object: usize,
    },
    UnboxConsume {
        dst: usize,
        object: usize,
    },
    Project {
        dst: usize,
        object: usize,
        field: usize,
    },
    ProjectOwned {
        dst: usize,
        object: usize,
        field: usize,
    },
    Record {
        dst: usize,
        fields: Vec<(String, usize)>,
    },
    Field {
        dst: usize,
        object: usize,
        field: String,
    },
    Update {
        dst: usize,
        object: usize,
        labels: Vec<String>,
        values: Vec<usize>,
    },
    Match {
        dst: usize,
        object: usize,
        pattern: Box<Pat>,
    },
    MatchFlag {
        dst: usize,
        object: usize,
        pattern: Box<Pat>,
    },
    MatchBranch {
        object: usize,
        pattern: Box<Pat>,
        then_pc: usize,
        else_pc: usize,
    },
    Closure {
        dst: usize,
        code: usize,
        captures: Vec<usize>,
    },
    Apply {
        dst: usize,
        function: usize,
        argument: usize,
    },
    TailApply {
        dst: usize,
        function: usize,
        argument: usize,
    },
    Call {
        dst: usize,
        code: usize,
        args: Vec<usize>,
    },
    TailCall {
        dst: usize,
        code: usize,
        args: Vec<usize>,
    },
    Global {
        dst: usize,
        global: usize,
    },
    Dictionary {
        dst: usize,
        entries: Vec<(usize, usize, bool)>,
    },
    Method {
        dst: usize,
        dictionary: usize,
        field: usize,
    },
    Structure {
        dst: usize,
        object: usize,
        expected: usize,
    },
    VarMake {
        dst: usize,
        src: usize,
    },
    VarGet {
        dst: usize,
        var: usize,
    },
    VarSet {
        dst: usize,
        var: usize,
        value: usize,
    },
    ReadCell {
        dst: usize,
        var: usize,
    },
    WriteCell {
        var: usize,
        value: usize,
    },
    UpdateCell {
        var: usize,
        op: BinOp,
        right: ValueSource,
    },
    Loop {
        dst: usize,
        id: usize,
        code: usize,
        args: Vec<usize>,
    },
    LoopExit {
        id: usize,
        value: Option<usize>,
    },
    FunctionBoundary {
        dst: usize,
        code: usize,
        args: Vec<usize>,
    },
    UnwrapOption {
        dst: usize,
        option: usize,
    },
    Try {
        dst: usize,
        body: usize,
        selector: usize,
    },
    Handle {
        dst: usize,
        body: usize,
        operations: Vec<CompiledHandlerOp>,
        ret: Option<usize>,
        errors: Option<usize>,
    },
    Perform {
        dst: usize,
        op_id: usize,
        dicts: Vec<usize>,
        args: Vec<usize>,
    },
    ScopeRun {
        dst: usize,
        body: usize,
    },
    ScopeCurrent {
        dst: usize,
        unit: usize,
    },
    ScopeAddFinalizer {
        dst: usize,
        task: usize,
    },
    ScopeAddFinalizerExit {
        dst: usize,
        func: usize,
    },
    ScopeAddClose {
        dst: usize,
        func: usize,
    },
    AcquireRelease {
        dst: usize,
        acquire: usize,
        release: usize,
    },
    Bracket {
        dst: usize,
        acquire: usize,
        use_fn: usize,
        release: usize,
    },
    Uninterruptible {
        dst: usize,
        task: usize,
    },
    Task {
        dst: usize,
        code: usize,
        captures: Vec<usize>,
    },
    Await {
        dst: usize,
        task: usize,
    },
    Within {
        dst: usize,
        owner: usize,
        code: usize,
        args: Vec<usize>,
    },
    WithOwnerEvidence {
        dst: usize,
        owner: usize,
        code: usize,
        args: Vec<usize>,
    },
    GenerateStrict {
        dst: usize,
        code: usize,
        args: Vec<usize>,
    },
    GenerateLazy {
        dst: usize,
        code: usize,
        captures: Vec<usize>,
        limit: Option<usize>,
    },
    Yield {
        dst: usize,
        value: usize,
    },
    SeqOpen {
        dst: usize,
        source: usize,
    },
    SeqNext {
        dst: usize,
        iterator: usize,
    },
    SeqOfList {
        dst: usize,
        list: usize,
    },
    SeqToList {
        dst: usize,
        seq: usize,
    },
    RefMake {
        dst: usize,
        src: usize,
    },
    RefGet {
        dst: usize,
        cell: usize,
    },
    RefSet {
        dst: usize,
        cell: usize,
        value: usize,
    },
    RefModify {
        dst: usize,
        cell: usize,
        func: usize,
    },
    RefSame {
        dst: usize,
        left: usize,
        right: usize,
    },
    ArenaMake {
        dst: usize,
        unit: usize,
    },
    ArenaAlloc {
        dst: usize,
        arena: usize,
        value: usize,
    },
    ArenaGet {
        dst: usize,
        view: usize,
    },
    LazyMake {
        dst: usize,
        task: usize,
        local: bool,
    },
    LazyForce {
        dst: usize,
        lazy: usize,
    },
    EffectExit {
        dst: usize,
        task: usize,
    },
    EffectCatchDefect {
        dst: usize,
        handler: usize,
        task: usize,
    },
    LayerSucceed {
        dst: usize,
        tag: usize,
        implementation: usize,
    },
    LayerMerge {
        dst: usize,
        left: usize,
        right: usize,
    },
    LayerRun {
        dst: usize,
        layer: usize,
        body: usize,
    },
    AsyncOp {
        dst: usize,
        op: AsyncIntrinsic,
        args: Vec<usize>,
    },
    LayerOp {
        dst: usize,
        op: LayerIntrinsic,
        args: Vec<usize>,
    },
    Recovery {
        dst: usize,
        selected: Option<usize>,
    },
    FromResult {
        dst: usize,
        src: usize,
    },
    Raise {
        src: usize,
    },
    Die {
        src: usize,
    },
    FailureRethrow {
        src: usize,
    },
    MatchFail,
    Return {
        value: usize,
    },
    ReturnUnit,
    Branch {
        condition: usize,
        then_pc: usize,
        else_pc: usize,
    },
    CompareBranch {
        op: BinOp,
        left: ValueSource,
        right: ValueSource,
        then_pc: usize,
        else_pc: usize,
    },
}

/// Structured failure when preparing a memory-IR program for tier 1.
#[derive(Debug, PartialEq, Eq)]
pub enum PrepareError {
    /// The input memory IR failed linear/ownership validation.
    Invalid(memory::Error),
    /// The requested entry global does not exist in the program.
    UnknownEntry(Name),
    /// A reachable memory-IR operation is not supported by tier 1 yet.
    Unsupported {
        code: CodeId,
        operation: String,
        span: Option<MemorySpan>,
    },
}

impl PrepareError {
    /// Returns the source span attached to this preparation failure, if any.
    #[must_use]
    pub const fn span(&self) -> Option<MemorySpan> {
        match self {
            Self::Unsupported { span, .. } => *span,
            Self::Invalid(_) | Self::UnknownEntry(_) => None,
        }
    }

    /// Returns the unsupported operation name when this is `Self::Unsupported`.
    #[must_use]
    pub fn operation(&self) -> Option<&str> {
        match self {
            Self::Unsupported { operation, .. } => Some(operation),
            Self::Invalid(_) | Self::UnknownEntry(_) => None,
        }
    }
}

impl fmt::Display for PrepareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(error) => write!(f, "invalid memory IR: {error}"),
            Self::UnknownEntry(name) => write!(f, "unknown entry {}.{}", name.module, name.name),
            Self::Unsupported {
                code,
                operation,
                span: Some(span),
            } => write!(
                f,
                "tier 1 VM does not support operation `{operation}` in memory code {code} \
                 (file {}, bytes {}..{})",
                span.file, span.start, span.end
            ),
            Self::Unsupported {
                code,
                operation,
                span: None,
            } => write!(
                f,
                "tier 1 VM does not support operation `{operation}` in memory code {code}"
            ),
        }
    }
}

impl error::Error for PrepareError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::Invalid(error) => Some(error),
            Self::UnknownEntry(_) | Self::Unsupported { .. } => None,
        }
    }
}
