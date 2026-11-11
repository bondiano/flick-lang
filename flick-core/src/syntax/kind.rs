//! Token and node kinds of the lossless syntax tree.

/// Defines `SyntaxKind`. A keyword with a word is reserved: the lexer makes
/// it of that word, and no name may spell it. One without a word is
/// contextual, and the parser gives the kind where the word means it.
macro_rules! syntax_kinds {
    (
        before { $( $(#[$b:meta])* $before:ident, )* }
        keywords { $( $(#[$k:meta])* $kw:ident $(= $word:literal)?, )* }
        after { $( $(#[$a:meta])* $after:ident, )* }
    ) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum SyntaxKind {
            $( $(#[$b])* $before, )*
            $( $(#[$k])* $kw, )*
            $( $(#[$a])* $after, )*
        }

        /// The reserved words.
        pub const KEYWORDS: &[&str] = &[$($($word,)?)*];

        impl SyntaxKind {
            pub fn is_keyword(self) -> bool {
                matches!(self, $(SyntaxKind::$kw)|*)
            }

            /// `` `word` `` of a reserved word.
            fn quoted(self) -> Option<&'static str> {
                match self {
                    $($(SyntaxKind::$kw => Some(concat!("`", $word, "`")),)?)*
                    _ => None,
                }
            }
        }

        pub fn keyword(text: &str) -> Option<SyntaxKind> {
            match text {
                $($($word => Some(SyntaxKind::$kw),)?)*
                _ => None,
            }
        }
    };
}

syntax_kinds! {
    before {
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
        /// `"text\{` of a string with holes: up to its first hole.
        StringStart,
        /// `}text\{` between two holes.
        StringMid,
        /// `}text"` after the last hole.
        StringEnd,

    }
    keywords {
        ModuleKw = "module",
        ExposingKw = "exposing",
        ImportKw = "import",
        AsKw = "as",
        LetKw = "let",
        MutKw = "mut",
        RecKw = "rec",
        AssertKw = "assert",
        FunKw = "fun",
        MatchKw = "match",
        WithKw = "with",
        TryKw = "try",
        IfKw = "if",
        ThenKw = "then",
        ElseKw = "else",
        TaskKw = "task",
        AwaitKw = "await",
        ScopeKw = "scope",
        ProvideKw = "provide",
        IgnoreKw = "ignore",
        TodoKw = "todo",
        EchoKw = "echo",
        TypeKw = "type",
        OpaqueKw = "opaque",
        RefinedKw = "refined",
        WhereKw = "where",
        KindKw = "kind",
        ErrorKw = "error",
        ServiceKw = "service",
        ClassKw = "class",
        InstanceKw = "instance",
        DerivingKw = "deriving",
        ForeignKw = "foreign",
        FromKw = "from",
        ForallKw = "forall",
        NotKw = "not",
        NoneKw = "none",
        TrueKw = "true",
        FalseKw = "false",
        UiKw = "ui",
        InKw = "in",
        WhenKw = "when",
        LoopKw,
        WhileKw,
        BreakKw = "break",
        ContinueKw = "continue",
        ForKw = "for",
        YieldKw = "yield",
        DoKw = "do",
    }
    after {
        /// `and` between the bindings of a `do` group, a keyword only there.
        AndKw,
        /// `plugin` before a head in the exposed items of an import, a keyword
        /// only there.
        PluginKw,
        /// `effect` before the name of an effect declaration, a keyword only
        /// there.
        EffectKw,
        /// `tail`, `abort` and `control` before an operation of an effect, the
        /// class of its handler's branch; keywords only there.
        TailKw,
        AbortKw,
        ControlKw,
        /// `return` before the pattern of a handler's branch for the value of
        /// its body, a keyword only there.
        ReturnKw,

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
        /// A private inline child whose declarations share its parent's source file.
        InlineModuleDecl,
        /// A named static application: `module Names = Set.Make(Key)`.
        ModuleApplyDecl,
        ModuleParam,
        ModuleParamGroup,
        ModuleResult,
        ModuleSignatureArrow,
        ModuleSignatureBinder,
        ModuleSignatureParen,
        ModuleSignatureRefinement,
        ModuleTypeEquality,
        ModuleMember,
        ModuleApplyExpr,
        ModuleParamExpr,
        ModuleParenExpr,
        ModuleAnnotExpr,
        Exposing,
        ExposeItem,
        ModulePath,
        ImportDecl,
        Signature,
        LetDecl,
        Params,
        RecGroup,
        TypeDecl,
        /// `type module Name = ...`: a structural module contract.
        ModuleTypeDecl,
        /// An abstract or manifest type required by a module contract.
        ModuleTypeMember,
        /// `class C = M.C` or `service S = M.S` in a module contract.
        ModuleNominalMember,
        /// `instance C T from dictionary` in a module contract.
        ModuleInstanceReq,
        TypeParam,
        Variant,
        /// `: T index…` after the fields of a constructor of a GADT: the type
        /// it builds.
        VariantResult,
        Refinement,
        DerivingClause,
        /// `{ … }` after a class in `deriving`: the arguments of its derive,
        /// tokens its plugin reads.
        DerivingArgs,
        /// `deriving instance C (T a)`: an instance a class's derive makes for a
        /// type of another module.
        DerivingDecl,
        /// `@name argument…` before a declaration or a constructor.
        Attribute,
        KindDecl,
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
        /// `"text \{e} text"`: a string with holes, each a `display` of its expression.
        InterpExpr,
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
        PackExpr,
        UnpackExpr,
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
        /// `name = expr`: an assignment to a `let mut` variable.
        AssignStmt,
        /// `p <- a and q <- b`: bindings that run independently (`map2`).
        BindGroup,
        /// `if condition` without `then` in a `do` block: a guard through `Alternative`.
        GuardStmt,
        /// `for { … } yield e`: a lazy `Seq`; in `[ ]` alone, a strict `List`.
        LoopExpr,
        WhileExpr,
        BreakExpr,
        ContinueExpr,
        ForExpr,
        ScopeExpr,
        ProvideExpr,
        UiExpr,
        /// A body with a module open, which only generated code writes: the
        /// names of the module are in sight in it before the file's own.
        OpenExpr,
        /// `head { … }` of a head an import exposes as `plugin head`: the body is
        /// kept as tokens, which the plugin reads.
        PluginExpr,
        /// `( … )` among the arguments of a plugin block's head, kept as tokens.
        PluginArgs,
        /// A plugin block that stands as a declaration of the module: `head args
        /// { … }`, or `head args` and the rest of the declaration.
        PluginDecl,
        /// The rest of a declaration after a plugin head and its arguments, kept
        /// as tokens up to the next declaration.
        PluginRest,

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
        RowSpread,
        RowOmit,
        ForallType,
        ExistsType,
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
        /// `todo`: a required type placeholder that keeps checking the draft.
        TodoType,
        CaptureType,
    }
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

    pub fn is_closer(self) -> bool {
        use SyntaxKind as K;
        matches!(self, K::RParen | K::RBracket | K::RBrace)
    }

    /// Binary operator tokens of the fixed operator table.
    pub fn is_binary_op(self) -> bool {
        self.binding_power().is_some()
    }

    /// Left and right binding powers of a binary operator, weakest first
    /// (grammar.md); the formatter groups operators of one left power.
    pub fn binding_power(self) -> Option<(u8, u8)> {
        use SyntaxKind as K;
        Some(match self {
            K::PipeGt => (2, 3),
            K::OrOr => (4, 4),
            K::AndAnd => (6, 6),
            K::EqEq | K::NotEq | K::Lt | K::Le | K::Gt | K::Ge => (8, 9),
            K::PlusPlus => (10, 10),
            K::Plus | K::Minus => (12, 13),
            K::Star | K::Slash | K::Percent => (14, 15),
            K::StarStar => (18, 18),
            _ => return None,
        })
    }
}

/// The words after `foreign` that say how a binding runs: keywords there
/// only, names everywhere else.
/// The words a `foreign` may name its mode with; without one the signature
/// tells: a call of the host, awaited when its row has `Async`. `sync` and
/// `async` are the words of before, which a fix removes.
pub const FOREIGN_MODES: [&str; 5] = ["pure", "effect", "blocked", "sync", "async"];

/// How a kind is shown in "expected …" messages.
pub fn describe(kind: SyntaxKind) -> &'static str {
    use SyntaxKind as K;
    if let Some(word) = kind.quoted() {
        return word;
    }
    match kind {
        K::Ident => "a name",
        K::Con => "a capitalized name",
        K::Number | K::BigInt => "a number",
        K::String => "a string",
        K::StringMid | K::StringEnd => "`}` to close `\\{`",
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
        _ => "a token",
    }
}
