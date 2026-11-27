/// Pure fixed-width integer and binary32 float intrinsics (`flick:rt`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FixedWidthIntrinsic {
    ShowInt64,
    ShowUInt64,
    ToInt8,
    ToInt16,
    ToInt32,
    ToInt64,
    ToUInt8,
    ToUInt16,
    ToUInt32,
    ToUInt64,
    ToFloat32,
    ToFloat64,
    ToBigInt,
    Int64BitAnd,
    Int64BitOr,
    Int64BitXor,
    Int64BitNot,
    Int64ShiftLeft,
    Int64ShiftRight,
    Int64ShiftRightUnsigned,
}

impl FixedWidthIntrinsic {
    pub fn arity(self) -> usize {
        match self {
            Self::Int64BitAnd
            | Self::Int64BitOr
            | Self::Int64BitXor
            | Self::Int64ShiftLeft
            | Self::Int64ShiftRight
            | Self::Int64ShiftRightUnsigned => 2,
            _ => 1,
        }
    }
}

/// Pure 32-bit signed integer intrinsics for `packages/core/src/Int.fl`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntIntrinsic {
    Parse,
    FromFloat,
    Truncate,
    Abs,
    BitNot,
    CheckedAdd,
    CheckedSub,
    CheckedMul,
    CheckedDiv,
    CheckedRem,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
    ShiftRightUnsigned,
}

impl IntIntrinsic {
    pub fn arity(self) -> usize {
        match self {
            Self::Parse | Self::FromFloat | Self::Truncate | Self::Abs | Self::BitNot => 1,
            Self::CheckedAdd
            | Self::CheckedSub
            | Self::CheckedMul
            | Self::CheckedDiv
            | Self::CheckedRem
            | Self::BitAnd
            | Self::BitOr
            | Self::BitXor
            | Self::ShiftLeft
            | Self::ShiftRight
            | Self::ShiftRightUnsigned => 2,
        }
    }
}

/// Pure binary64 float intrinsics for `packages/core/src/Float.fl` and `String.to_number`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FloatIntrinsic {
    IsNaN,
    IsFinite,
    IsInteger,
    IsSafeInteger,
    Abs,
    Floor,
    Ceil,
    Round,
    Trunc,
    Sqrt,
    Parse,
    NextPow2,
    SafeInteger,
    ToString,
    StringToNumber,
    Remainder,
    RoundTo,
    /// `Math.log`, with the bits JS gives.
    Log,
}

impl FloatIntrinsic {
    pub fn arity(self) -> usize {
        match self {
            Self::Remainder | Self::RoundTo => 2,
            _ => 1,
        }
    }
}

/// Pure arbitrary-precision integer intrinsics for `packages/std/src/BigInt.fl`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BigIntIntrinsic {
    FromNumber,
    FromSafeNumber,
    ToSafeNumber,
    ToString,
    Parse,
    FromString,
    Abs,
    Sqrt,
    Divide,
    Remainder,
    Gcd,
    Lcm,
}

impl BigIntIntrinsic {
    pub fn arity(self) -> usize {
        match self {
            Self::Divide | Self::Remainder | Self::Gcd | Self::Lcm => 2,
            _ => 1,
        }
    }
}

/// Pure string intrinsics for `packages/core/src/String.fl`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StringIntrinsic {
    Trim,
    TrimStart,
    TrimEnd,
    ToUpper,
    ToLower,
    Capitalize,
    Uncapitalize,
    Lines,
    LinesWithSeparators,
    StripMargin,
    SnakeToCamel,
    SnakeToPascal,
    SnakeToKebab,
    CamelToSnake,
    PascalToSnake,
    KebabToSnake,
    NoCase,
    PascalCase,
    CamelCase,
    ConstantCase,
    ConfigCase,
    KebabCase,
    SnakeCase,
    Chars,
    Split,
    EndsWith,
    IndexOf,
    LastIndexOf,
    ToLocaleUpper,
    ToLocaleLower,
    TakeLeft,
    TakeRight,
    At,
    CharAt,
    CharCodeAt,
    CodePointAt,
    Normalize,
    Repeat,
    StripMarginWith,
    Slice,
    Substring,
    PadStart,
    PadEnd,
    ReplaceAll,
}

impl StringIntrinsic {
    pub fn arity(self) -> usize {
        match self {
            Self::Trim
            | Self::TrimStart
            | Self::TrimEnd
            | Self::ToUpper
            | Self::ToLower
            | Self::Capitalize
            | Self::Uncapitalize
            | Self::Lines
            | Self::LinesWithSeparators
            | Self::StripMargin
            | Self::SnakeToCamel
            | Self::SnakeToPascal
            | Self::SnakeToKebab
            | Self::CamelToSnake
            | Self::PascalToSnake
            | Self::KebabToSnake
            | Self::NoCase
            | Self::PascalCase
            | Self::CamelCase
            | Self::ConstantCase
            | Self::ConfigCase
            | Self::KebabCase
            | Self::SnakeCase
            | Self::Chars => 1,
            Self::Split
            | Self::EndsWith
            | Self::IndexOf
            | Self::LastIndexOf
            | Self::ToLocaleUpper
            | Self::ToLocaleLower
            | Self::TakeLeft
            | Self::TakeRight
            | Self::At
            | Self::CharAt
            | Self::CharCodeAt
            | Self::CodePointAt
            | Self::Normalize
            | Self::Repeat
            | Self::StripMarginWith => 2,
            Self::Slice
            | Self::Substring
            | Self::PadStart
            | Self::PadEnd
            | Self::ReplaceAll => 3,
        }
    }
}

/// Pure non-callback list and `NonEmpty` intrinsics for `packages/core/src/List*.fl`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ListIntrinsic {
    Length,
    Reverse,
    Concat,
    Unzip,
    Tail,
    Init,
    NonEmptyFromList,
    NonEmptyHead,
    NonEmptyLast,
    NonEmptyTail,
    NonEmptyInit,
    NonEmptyUnappend,
    NonEmptyUnprepend,
    Get,
    Repeat,
    Take,
    Drop,
    Zip,
    TakeRight,
    DropRight,
    SplitAt,
    Split,
    ChunksOf,
    Window,
    Rotate,
    Remove,
    Intersperse,
    AppendAll,
    Cartesian,
    RepeatAll,
    NonEmptyCons,
    NonEmptyAppend,
    NonEmptyRange,
    NonEmptyReplicate,
    NonEmptySetHead,
    NonEmptySetLast,
    NonEmptySplitAt,
    Set,
    Slice,
    Pad,
    InsertAt,
    Replace,
}

impl ListIntrinsic {
    pub fn arity(self) -> usize {
        match self {
            Self::Length
            | Self::Reverse
            | Self::Concat
            | Self::Unzip
            | Self::Tail
            | Self::Init
            | Self::NonEmptyFromList
            | Self::NonEmptyHead
            | Self::NonEmptyLast
            | Self::NonEmptyTail
            | Self::NonEmptyInit
            | Self::NonEmptyUnappend
            | Self::NonEmptyUnprepend => 1,
            Self::Set | Self::Slice | Self::Pad | Self::InsertAt | Self::Replace => 3,
            _ => 2,
        }
    }
}

/// Pure non-callback `ReadonlyRecord` map intrinsics for `packages/core/src/Record.fl`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordIntrinsic {
    Empty,
    IsEmpty,
    FromEntries,
    ToEntries,
    Keys,
    Values,
    Size,
    FromKeys,
    Singleton,
    Has,
    Get,
    Remove,
    Pop,
    Difference,
    Replace,
    Set,
}

impl RecordIntrinsic {
    pub fn arity(self) -> usize {
        match self {
            Self::Empty
            | Self::IsEmpty
            | Self::FromEntries
            | Self::ToEntries
            | Self::Keys
            | Self::Values
            | Self::Size => 1,
            Self::Replace | Self::Set => 3,
            _ => 2,
        }
    }
}

/// Diagnostic debug intrinsics for `packages/core/src/Flick/Debug.fl`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DebugIntrinsic {
    Todo,
    Echo,
    Assert,
}

impl DebugIntrinsic {
    pub fn arity(self) -> usize {
        match self {
            Self::Todo => 1,
            Self::Echo | Self::Assert => 2,
        }
    }
}

/// Packed immutable `Bytes` and legacy `Bytes` UTF-8/base64url intrinsics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BytesIntrinsic {
    Empty,
    Length,
    Get,
    Slice,
    FromList,
    ToList,
    EncodeUtf8,
    DecodeUtf8,
    EncodeBase64Url,
    DecodeBase64Url,
    EqualConstantTime,
    LegacyUtf8Encode,
    LegacyUtf8Decode,
    LegacyBase64UrlEncode,
    LegacyBase64UrlDecode,
}

impl BytesIntrinsic {
    pub fn arity(self) -> usize {
        match self {
            Self::Empty => 0,
            Self::Get | Self::EqualConstantTime => 2,
            Self::Slice => 3,
            _ => 1,
        }
    }
}

/// Bitwise hash mixing and HAMT walk-order comparison intrinsics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HashIntrinsic {
    Combine,
    Optimize,
    WalkCompare,
}

impl HashIntrinsic {
    pub fn arity(self) -> usize {
        match self {
            Self::Optimize => 1,
            Self::Combine | Self::WalkCompare => 2,
        }
    }
}
