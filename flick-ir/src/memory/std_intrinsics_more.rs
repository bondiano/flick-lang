/// Pure JSON, JSON Pointer, JSON Patch, Formatter, Inspectable, and Version intrinsics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JsonIntrinsic {
    Parse,
    Stringify,
    FormatJson,
    InspectToString,
    EscapeToken,
    UnescapeToken,
    ParseUriFragment,
    FormatUriFragment,
    PatchGet,
    PatchApply,
    VersionCurrent,
}

impl JsonIntrinsic {
    pub fn arity(self) -> usize {
        match self {
            Self::Parse
            | Self::Stringify
            | Self::EscapeToken
            | Self::UnescapeToken
            | Self::ParseUriFragment
            | Self::FormatUriFragment
            | Self::VersionCurrent => 1,
            Self::FormatJson | Self::InspectToString | Self::PatchGet | Self::PatchApply => 2,
        }
    }
}

/// Pure exact decimal intrinsics (`packages/std/src/BigDecimal.fl`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BigDecimalIntrinsic {
    FromString,
    FromBigInt,
    FromInteger,
    FromNumber,
    ToString,
    ToStringNormal,
    ToExponential,
    Scale,
    Add,
    Subtract,
    Multiply,
    DivideToScale,
    Normalize,
    SetScale,
    SumAll,
    MultiplyAll,
    Divide,
    Remainder,
    Compare,
    Sign,
    Abs,
    Negate,
    IsInteger,
    IsZero,
    IsNegative,
    IsPositive,
    Round,
    Truncate,
    Ceil,
    Floor,
}

impl BigDecimalIntrinsic {
    pub fn arity(self) -> usize {
        match self {
            Self::FromString
            | Self::FromInteger
            | Self::FromNumber
            | Self::ToString
            | Self::ToStringNormal
            | Self::ToExponential
            | Self::Scale
            | Self::Normalize
            | Self::SumAll
            | Self::MultiplyAll
            | Self::Sign
            | Self::Abs
            | Self::Negate
            | Self::IsInteger
            | Self::IsZero
            | Self::IsNegative
            | Self::IsPositive => 1,
            Self::Round => 3,
            Self::DivideToScale => 4,
            _ => 2,
        }
    }
}

/// Pure regular expression intrinsics (`packages/std/src/RegExp.fl`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegExpIntrinsic {
    Make,
    Escape,
    MatchFirst,
    MatchAll,
    Search,
    Replace,
    ReplaceAll,
}

impl RegExpIntrinsic {
    pub fn arity(self) -> usize {
        match self {
            Self::Escape => 1,
            Self::Replace | Self::ReplaceAll => 3,
            _ => 2,
        }
    }
}

/// Pure non-negative `Duration` intrinsics (`packages/std/src/Duration.fl`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DurationIntrinsic {
    Zero,
    Infinity,
    Nanos,
    Micros,
    Millis,
    Seconds,
    Minutes,
    Hours,
    Days,
    Weeks,
    FromInput,
    IsFinite,
    IsZero,
    IsPositive,
    ToMillis,
    ToSeconds,
    ToMinutes,
    ToHours,
    ToDays,
    ToWeeks,
    ToNanos,
    ToHrTime,
    Parts,
    Format,
    Deconstruct,
    Add,
    Subtract,
    Times,
    Divide,
    Compare,
}

impl DurationIntrinsic {
    pub fn arity(self) -> usize {
        match self {
            Self::Zero | Self::Infinity => 0,
            Self::Add | Self::Subtract | Self::Times | Self::Divide | Self::Compare => 2,
            _ => 1,
        }
    }
}

/// Pure `Exit`, `Cause`, and `Failure` observer/constructor intrinsics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutcomeIntrinsic {
    ExitVoid,
    CauseEmpty,
    ExitValue,
    ExitFailure,
    ExitSucceed,
    ExitFail,
    ExitFailCause,
    ExitDie,
    ExitInterrupt,
    ExitHasFails,
    ExitHasDies,
    ExitFilterSuccess,
    ExitFilterValue,
    ExitFilterCause,
    ExitFindError,
    ExitFindErrorOption,
    ExitFindDefect,
    ExitAsVoid,
    ExitAsVoidAll,
    CauseFail,
    CauseDie,
    CauseInterrupt,
    CauseFromReasons,
    CauseReasons,
    CauseMakeFailReason,
    CauseMakeDieReason,
    CauseMakeInterruptReason,
    CauseIsFailReason,
    CauseIsDieReason,
    CauseIsInterruptReason,
    CauseHasInterruptsOnly,
    CauseHasFails,
    CauseHasDies,
    CauseHasInterrupts,
    CauseFindFail,
    CauseFindError,
    CauseFindErrorOption,
    CauseFindDie,
    CauseFindDefect,
    CauseFindInterrupt,
    CauseInterruptors,
    CauseFilterInterruptors,
    CausePretty,
    CausePrettyErrors,
    CauseNoneFrame,
    CauseCombine,
    CauseReplaceFirstFail,
}

impl OutcomeIntrinsic {
    pub fn arity(self) -> usize {
        match self {
            Self::ExitVoid | Self::CauseEmpty => 0,
            Self::CauseCombine | Self::CauseReplaceFirstFail => 2,
            _ => 1,
        }
    }
}

/// Pure `Gen` PRNG step, ASCII string draw, and non-empty check intrinsics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GenIntrinsic {
    Step,
    DrawString,
    OneOfCheck,
    DrawOneOf,
}

impl GenIntrinsic {
    pub fn arity(self) -> usize {
        match self {
            Self::Step | Self::OneOfCheck => 1,
            Self::DrawString | Self::DrawOneOf => 2,
        }
    }
}
