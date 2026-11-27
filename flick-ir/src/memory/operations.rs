impl Intrinsic {
    pub fn arity(self) -> usize {
        match self {
            Self::MatchFail => 0,
            Self::PrintLine
            | Self::Show
            | Self::RefMake
            | Self::RefGet
            | Self::ScopeRun
            | Self::ScopeCurrent
            | Self::ArenaMake
            | Self::ArenaGet
            | Self::FromResult
            | Self::Raise
            | Self::PrimitiveHash
            | Self::StringLength
            | Self::NumberToFloat
            | Self::ArrayToList
            | Self::ToBigInt
            | Self::Host(_)
            | Self::LazyMake
            | Self::LazyMakeLocal
            | Self::LazyForce
            | Self::ScopeAddFinalizer
            | Self::ScopeAddFinalizerExit
            | Self::ScopeAddClose
            | Self::Uninterruptible
            | Self::RecordLabels
            | Self::SeqOfList
            | Self::SeqToList
            | Self::ExitIsSuccess
            | Self::ExitIsFailure
            | Self::ExitHasInterrupts
            | Self::EffectExit
            | Self::EffectDie
            | Self::FailureRethrow => 1,
            Self::RefSet
            | Self::RefModify
            | Self::RefSame
            | Self::ArenaAlloc
            | Self::Try
            | Self::LayerSucceed
            | Self::LayerRun
            | Self::LayerMerge
            | Self::StringStartsWith
            | Self::StringIncludes
            | Self::PrimitiveCompare
            | Self::IntUnsafeDiv
            | Self::IntUnsafeRem
            | Self::FloatMin
            | Self::FloatMax
            | Self::ListRange
            | Self::AcquireRelease
            | Self::RecordGet
            | Self::RecordRemove
            | Self::RecordProject
            | Self::RecordMergeDisjoint
            | Self::EffectCatchDefect => 2,
            Self::Bracket | Self::RecordExtend => 3,
            Self::Int(op) => op.arity(),
            Self::FixedWidth(op) => op.arity(),
            Self::Float(op) => op.arity(),
            Self::BigInt(op) => op.arity(),
            Self::String(op) => op.arity(),
            Self::List(op) => op.arity(),
            Self::Record(op) => op.arity(),
            Self::Debug(op) => op.arity(),
            Self::Bytes(op) => op.arity(),
            Self::Hash(op) => op.arity(),
            Self::Json(op) => op.arity(),
            Self::BigDecimal(op) => op.arity(),
            Self::RegExp(op) => op.arity(),
            Self::Duration(op) => op.arity(),
            Self::Outcome(op) => op.arity(),
            Self::Gen(op) => op.arity(),
            Self::Schema(op) => op.arity(),
            Self::JsonSchema(op) => op.arity(),
            Self::Path(op) => op.arity(),
            Self::DateTime(op) => op.arity(),
            Self::Cron(op) => op.arity(),
            Self::Graph(op) => op.arity(),
            Self::Async(op) => op.arity(),
            Self::Layer(op) => op.arity(),
            Self::Crypto(op) => op.arity(),
            Self::Config(op) => op.arity(),
            Self::Arbitrary(op) => op.arity(),
            Self::HostService(op) => op.arity(),
        }
    }
}

impl Operation {
    pub fn consumed(&self) -> Vec<Local> {
        match self {
            Self::Literal(_)
            | Self::ReadOwnerEvidence
            | Self::Project { .. }
            | Self::Field { .. }
            | Self::Method { .. }
            | Self::Structure { .. }
            | Self::Match { .. }
            | Self::UnboxBorrow { .. }
            | Self::Global(_)
            | Self::ServiceTag(_)
            | Self::Label(_) => Vec::new(),
            Self::RecoveryResult { selected } => selected.iter().copied().collect(),
            Self::VarMake(value)
            | Self::VarGet(value)
            | Self::UnwrapOption(value)
            | Self::Yield(value)
            | Self::SeqOpen(value)
            | Self::SeqNext(value)
            | Self::Not(value)
            | Self::Neg { value, .. }
            | Self::Await(value)
            | Self::DropReuse(value)
            | Self::BoxAggregate { value, .. }
            | Self::UnboxConsume { object: value } => vec![*value],
            Self::VarSet { var, value } => vec![*var, *value],
            Self::Loop { args, .. }
            | Self::FunctionBoundary { args, .. }
            | Self::GenerateStrict { args, .. }
            | Self::Call { args, .. }
            | Self::Intrinsic { args, .. }
            | Self::ForeignCall { args, .. } => args.clone(),
            Self::LoopExit { value, .. } => value.iter().copied().collect(),
            Self::GenerateLazy {
                captures, limit, ..
            } => captures.iter().chain(limit).copied().collect(),
            Self::Construct { fields, .. } => fields.clone(),
            Self::ConstructWithDicts { fields, dicts, .. } => {
                fields.iter().chain(dicts).copied().collect()
            }
            Self::Record { fields } => fields.iter().map(|(_, local)| *local).collect(),
            Self::Dictionary { entries } => {
                entries.iter().map(|(_, entry)| entry.local()).collect()
            }
            Self::Update { object, fields } => iter::once(*object)
                .chain(fields.iter().map(|(_, local)| *local))
                .collect(),
            Self::Closure { captures, .. } | Self::Task { captures, .. } => captures.clone(),
            Self::Handle {
                body,
                effects,
                ret,
                errors,
            } => iter::once(*body)
                .chain(
                    effects
                        .iter()
                        .flat_map(|effect| effect.operations.iter().map(|op| op.function)),
                )
                .chain(*ret)
                .chain(*errors)
                .collect(),
            Self::Perform { dicts, args, .. } => dicts.iter().chain(args).copied().collect(),
            Self::Within { owner, args, .. } | Self::WithOwnerEvidence { owner, args, .. } => {
                let mut operands = vec![*owner];
                operands.extend(args);
                operands
            }
            Self::Apply { function, argument } => vec![*function, *argument],
            Self::Binary { left, right, .. } => vec![*left, *right],
            Self::ReuseWithDicts {
                object,
                fields,
                dicts,
                ..
            } => iter::once(*object)
                .chain(fields.iter().copied())
                .chain(dicts.iter().copied())
                .collect(),
            Self::Reuse { object, fields, .. } => {
                let mut operands = vec![*object];
                operands.extend(fields);
                operands
            }
            Self::ReuseRecord { object, fields } => iter::once(*object)
                .chain(fields.iter().map(|(_, local)| *local))
                .collect(),
        }
    }

    pub fn borrowed(&self) -> Option<Local> {
        match self {
            Self::Project { object, .. }
            | Self::Field { object, .. }
            | Self::Structure { object, .. }
            | Self::Match { object, .. }
            | Self::UnboxBorrow { object } => Some(*object),
            Self::Method { dictionary, .. } => Some(*dictionary),
            _ => None,
        }
    }

    /// Returns the statically known value representation produced by this
    /// operation, when determined solely by the operation itself.
    pub fn result_rep(&self) -> Option<ValueRep> {
        match self {
            Self::Construct { rep, .. } | Self::ConstructWithDicts { rep, .. } => Some(*rep),
            Self::UnboxBorrow { .. } | Self::UnboxConsume { .. } => Some(ValueRep::Aggregate),
            Self::Literal(_)
            | Self::Record { .. }
            | Self::Match { .. }
            | Self::Update { .. }
            | Self::Closure { .. }
            | Self::Task { .. }
            | Self::ReadOwnerEvidence
            | Self::Binary { .. }
            | Self::Not(_)
            | Self::Neg { .. }
            | Self::DropReuse(_)
            | Self::Reuse { .. }
            | Self::ReuseWithDicts { .. }
            | Self::ReuseRecord { .. }
            | Self::BoxAggregate { .. }
            | Self::ServiceTag(_)
            | Self::Dictionary { .. }
            | Self::Structure { .. }
            | Self::RecoveryResult { .. }
            | Self::VarMake(_)
            | Self::GenerateStrict { .. }
            | Self::GenerateLazy { .. }
            | Self::SeqOpen(_)
            | Self::Label(_) => Some(ValueRep::Cell),
            Self::Project { .. }
            | Self::Field { .. }
            | Self::Await(_)
            | Self::Apply { .. }
            | Self::Call { .. }
            | Self::Within { .. }
            | Self::WithOwnerEvidence { .. }
            | Self::Global(_)
            | Self::Intrinsic { .. }
            | Self::Handle { .. }
            | Self::Perform { .. }
            | Self::Method { .. }
            | Self::VarGet(_)
            | Self::VarSet { .. }
            | Self::Loop { .. }
            | Self::LoopExit { .. }
            | Self::FunctionBoundary { .. }
            | Self::UnwrapOption(_)
            | Self::Yield(_)
            | Self::SeqNext(_)
            | Self::ForeignCall { .. } => None,
        }
    }
}
