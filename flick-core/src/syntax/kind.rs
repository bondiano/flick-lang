//! Token and node kinds of the lossless syntax tree.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SyntaxKind {
    // trivia
    Whitespace,
    Newline,
    LineComment,
    DocComment,
    BlockComment,

    // literals and names
    Ident,
    Con,
    Number,
    BigInt,
    String,

    // keywords
    ModuleKw,
    ExposingKw,
    ImportKw,
    AsKw,
    LetKw,
    RecKw,
    AssertKw,
    FunKw,
    MatchKw,
    WithKw,
    TryKw,
    IfKw,
    ThenKw,
    ElseKw,
    TaskKw,
    AwaitKw,
    ScopeKw,
    ProvideKw,
    IgnoreKw,
    TodoKw,
    EchoKw,
    TypeKw,
    OpaqueKw,
    RefinedKw,
    WhereKw,
    KindKw,
    ErrorKw,
    ServiceKw,
    ClassKw,
    InstanceKw,
    DerivingKw,
    ForeignKw,
    FromKw,
    ForallKw,
    NotKw,
    NoneKw,
    TrueKw,
    FalseKw,
    SchemaKw,
    CodecKw,
    SqlKw,
    UiKw,
    CliKw,
    InKw,
    WhenKw,
    ForKw,
    YieldKw,
    DoKw,
    /// `and` between the bindings of a `do` group, a keyword only there.
    AndKw,
    /// `component` before the name of a component declaration, a keyword
    /// only there.
    ComponentKw,
    /// `server` before `fn` or before the name of a server function, a
    /// keyword only there.
    ServerKw,
    /// `fn` after `server`, a keyword only there.
    FnKw,

    // punctuation and operators
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Semi,
    Colon,
    Dot,
    DotDot,
    /// `..<` of a range without its end.
    DotDotLt,
    DotDotDot,
    Eq,
    Arrow,
    FatArrow,
    /// `<-` of a `do` binding.
    LArrow,
    Pipe,
    PipeGt,
    OrOr,
    AndAnd,
    EqEq,
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    PlusPlus,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    StarStar,
    Caret,
    Dollar,
    /// `@`, which starts an attribute: `@lang int`.
    At,
    /// Postfix `?`: the value of an `Option` or a `Result`, or an early exit.
    Question,
    Underscore,
    /// A character that starts no token.
    Unknown,
    Eof,
    /// Virtual token the parser reports at a layout boundary; never in a tree.
    LayoutSep,

    // nodes: declarations
    SourceFile,
    ModuleHeader,
    Exposing,
    ExposeItem,
    ModulePath,
    ImportDecl,
    Signature,
    LetDecl,
    Params,
    RecGroup,
    TypeDecl,
    TypeParam,
    Variant,
    Refinement,
    DerivingClause,
    /// `@name argument…` before a declaration or a constructor.
    Attribute,
    KindDecl,
    SqlTableDecl,
    SqlMigrationDecl,
    SqlColumn,
    ErrorDecl,
    ServiceDecl,
    ClassDecl,
    InstanceDecl,
    Context,
    Fundep,
    Members,
    ForeignDecl,
    ForeignSource,
    ForeignMeta,
    MetaItem,
    Error,

    // nodes: expressions
    Block,
    Literal,
    NameExpr,
    Path,
    FieldExpr,
    /// `e?`.
    QuestionExpr,
    /// `a..b`, `a..<b`, `a..`.
    RangeExpr,
    AppExpr,
    BinExpr,
    PrefixExpr,
    /// `todo` or `todo "what is missing"`.
    TodoExpr,
    ParenExpr,
    TupleExpr,
    UnitExpr,
    AnnotExpr,
    ListExpr,
    RecordExpr,
    RecordField,
    RecordUpdate,
    FunExpr,
    MatchExpr,
    MatchArm,
    Guard,
    TryExpr,
    IfExpr,
    TaskExpr,
    /// `do { … }`: statements sequenced through `Monad`.
    DoExpr,
    /// `pattern <- expr` in a `do` block.
    BindStmt,
    /// `p <- a and q <- b`: bindings that run independently (`map2`).
    BindGroup,
    /// `if condition` without `then` in a `do` block: a guard through `Alternative`.
    GuardStmt,
    /// `for { … } yield e`: a lazy `Seq`; in `[ ]` alone, a strict `List`.
    ForExpr,
    /// The body of `component name (props : P) = body`: its props pattern,
    /// then the `Block` of its setup statements and render, as a `fun`
    /// lists its parameters then its body.
    ComponentExpr,
    /// `server name`: the implementation of the server function `name`.
    ServerExpr,
    ScopeExpr,
    ProvideExpr,
    SqlExpr,
    SqlClause,
    /// `$name` inside `sql { … }`: a value of the program as a placeholder.
    SqlCapture,
    UiExpr,
    CliExpr,
    CliClause,
    CodecExpr,
    SchemaExpr,

    // nodes: patterns
    WildcardPat,
    VarPat,
    ConPat,
    LitPat,
    TuplePat,
    UnitPat,
    ParenPat,
    ListPat,
    RecordPat,
    RecordPatField,
    AnnotPat,
    OrPat,
    /// `...rest` closing a list pattern; not a pattern by itself.
    RestPat,

    // nodes: types and kinds
    NameType,
    AppType,
    ArrowType,
    EffectAnnot,
    Row,
    RowTail,
    ForallType,
    ConstrainedType,
    TupleType,
    UnitType,
    ParenType,
    RecordType,
    RecordTypeField,
    RowType,
    UnionType,
    /// `_`: a type left for the checker to infer.
    HoleType,
    CaptureType,
}

impl SyntaxKind {
    /// Tokens the parser skips. `Unknown` is among them: the lexer already
    /// reported it, and skipping it keeps the parse free of a second error.
    pub fn is_trivia(self) -> bool {
        use SyntaxKind as K;
        matches!(
            self,
            K::Whitespace
                | K::Newline
                | K::LineComment
                | K::DocComment
                | K::BlockComment
                | K::Unknown
        )
    }

    pub fn is_comment(self) -> bool {
        use SyntaxKind as K;
        matches!(self, K::LineComment | K::DocComment | K::BlockComment)
    }

    pub fn is_keyword(self) -> bool {
        (SyntaxKind::ModuleKw..=SyntaxKind::DoKw).contains(&self)
    }

    pub fn is_closer(self) -> bool {
        use SyntaxKind as K;
        matches!(self, K::RParen | K::RBracket | K::RBrace)
    }

    /// Binary operator tokens of the fixed operator table.
    pub fn is_binary_op(self) -> bool {
        (SyntaxKind::PipeGt..=SyntaxKind::StarStar).contains(&self)
    }
}

/// The words after `foreign` that say how a binding runs: keywords there
/// only, names everywhere else.
pub const FOREIGN_MODES: [&str; 5] = ["pure", "sync", "async", "effect", "blocked"];

pub fn keyword(text: &str) -> Option<SyntaxKind> {
    use SyntaxKind as K;
    Some(match text {
        "module" => K::ModuleKw,
        "exposing" => K::ExposingKw,
        "import" => K::ImportKw,
        "as" => K::AsKw,
        "let" => K::LetKw,
        "rec" => K::RecKw,
        "assert" => K::AssertKw,
        "fun" => K::FunKw,
        "match" => K::MatchKw,
        "with" => K::WithKw,
        "try" => K::TryKw,
        "if" => K::IfKw,
        "then" => K::ThenKw,
        "else" => K::ElseKw,
        "task" => K::TaskKw,
        "await" => K::AwaitKw,
        "scope" => K::ScopeKw,
        "provide" => K::ProvideKw,
        "ignore" => K::IgnoreKw,
        "todo" => K::TodoKw,
        "echo" => K::EchoKw,
        "type" => K::TypeKw,
        "opaque" => K::OpaqueKw,
        "refined" => K::RefinedKw,
        "where" => K::WhereKw,
        "kind" => K::KindKw,
        "error" => K::ErrorKw,
        "service" => K::ServiceKw,
        "class" => K::ClassKw,
        "instance" => K::InstanceKw,
        "deriving" => K::DerivingKw,
        "foreign" => K::ForeignKw,
        "from" => K::FromKw,
        "forall" => K::ForallKw,
        "not" => K::NotKw,
        "none" => K::NoneKw,
        "true" => K::TrueKw,
        "false" => K::FalseKw,
        "schema" => K::SchemaKw,
        "codec" => K::CodecKw,
        "sql" => K::SqlKw,
        "ui" => K::UiKw,
        "cli" => K::CliKw,
        "in" => K::InKw,
        "when" => K::WhenKw,
        "for" => K::ForKw,
        "yield" => K::YieldKw,
        "do" => K::DoKw,
        _ => return None,
    })
}

/// How a kind is shown in "expected …" messages.
pub fn describe(kind: SyntaxKind) -> &'static str {
    use SyntaxKind as K;
    match kind {
        K::Ident => "a name",
        K::Con => "a capitalized name",
        K::Number | K::BigInt => "a number",
        K::String => "a string",
        K::LParen => "`(`",
        K::RParen => "`)`",
        K::LBracket => "`[`",
        K::RBracket => "`]`",
        K::LBrace => "`{`",
        K::RBrace => "`}`",
        K::Comma => "`,`",
        K::Semi => "`;`",
        K::Colon => "`:`",
        K::Dot => "`.`",
        K::Eq => "`=`",
        K::Arrow => "`->`",
        K::LArrow => "`<-`",
        K::Question => "`?`",
        K::At => "`@`",
        K::Pipe => "`|`",
        K::Eof => "end of file",
        K::LayoutSep => "a new line",
        K::ExposingKw => "`exposing`",
        K::WithKw => "`with`",
        K::ThenKw => "`then`",
        K::ElseKw => "`else`",
        K::FromKw => "`from`",
        K::InKw => "`in`",
        K::WhereKw => "`where`",
        K::TypeKw => "`type`",
        K::YieldKw => "`yield`",
        _ => "a token",
    }
}
