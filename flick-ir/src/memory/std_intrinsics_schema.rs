/// Pure `Schema`, `Codec`, `Check`, `Route`, and `Differ.json_patch` intrinsics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SchemaIntrinsic {
    JsonNull,
    CheckInert,
    DescribeInput,
    JoinErrorMessages,
    TagOf,
    DeconstructAdt,
    RecordEntries,
    RecordFromEntries,
    RecordFromSortedEntries,
    TupleToList,
    ListFromWire,
    TupleFromList,
    IsJsonNull,
    IsRedacted,
    IsChunk,
    CheckEmail,
    CheckNonEmpty,
    CheckPositive,
    FormatValue,
    RouteParseUrl,
    RouteCoerceCandidates,
    RouteJsString,
    Defect,
    PrefixIssues,
    CombineErrors,
    ConstructAdt,
    RecordLookup,
    DecodeLeaf,
    EncodeLeaf,
    ValidateLeaf,
    AcceptsLeaf,
    LiteralAccepts,
    SplitDecode,
    SplitEncode,
    MakeCheck,
    RunCheck,
    SameValue,
    StrictSameValue,
    EncodeXml,
    ExportJsonSchema,
    RouteFormatUrl,
    DifferJsonPatchOps,
    DifferApplyJsonPatch,
    LiteralDecode,
    SplitKvDecode,
    SplitKvEncode,
}

impl SchemaIntrinsic {
    pub fn arity(self) -> usize {
        match self {
            Self::JsonNull => 0,
            Self::CheckInert
            | Self::DescribeInput
            | Self::JoinErrorMessages
            | Self::TagOf
            | Self::DeconstructAdt
            | Self::RecordEntries
            | Self::RecordFromEntries
            | Self::RecordFromSortedEntries
            | Self::TupleToList
            | Self::ListFromWire
            | Self::TupleFromList
            | Self::IsJsonNull
            | Self::IsRedacted
            | Self::IsChunk
            | Self::CheckEmail
            | Self::CheckNonEmpty
            | Self::CheckPositive
            | Self::FormatValue
            | Self::RouteParseUrl
            | Self::RouteCoerceCandidates
            | Self::RouteJsString
            | Self::Defect => 1,
            Self::LiteralDecode | Self::SplitKvDecode | Self::SplitKvEncode => 3,
            _ => 2,
        }
    }
}

/// Pure `JsonSchema` dialect conversion intrinsics (`packages/std/src/JsonSchema.fl`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JsonSchemaIntrinsic {
    FromDraft07,
    FromDraft202012,
    FromOpenApi31,
    FromOpenApi30,
    ToDraft07,
    ToDraft04,
    ToOpenApi31,
}

impl JsonSchemaIntrinsic {
    pub fn arity(self) -> usize {
        1
    }
}
