struct Checker<'a> {
    static_witnesses: &'a [Interface],
    file: FileId,
    text: &'a str,
    /// Where generated code starts in `text`.
    generated: u32,
    /// The classes the module marks `@derive`.
    derived: Vec<Derived>,
    env: Env<'a>,
    store: Store,
    kinds: KindStore,
    diagnostics: Vec<Diagnostic>,
    cx: ItemCx,
    /// The sign of each parameter of the declared types met so far.
    signs: variance::Signs,
    /// The modules the blocks around the expression open, innermost last,
    /// each as the file names it: a name one of them exports is that one,
    /// before the names of the file. `ui { … }` opens `Ui`. With each, the
    /// type of a child, if its block names one.
    opened: Vec<ui::Opened>,
    /// Capitalized type members visible only while checking a `type module` body.
    contract_types: Vec<(String, Type, kinds::Kind)>,
    /// Lexically opened `Module Sig` values, innermost first on lookup.
    opened_modules: Vec<module_contract::Opened>,
    witness_counter: u64,
    named_witnesses: module_contract::NamedWitnesses,
    sites: Vec<effects::Site>,
    literals: Vec<NumLiteral>,
    signatures: Vec<bindings::PendingSignature>,
    /// `==` and `!=` with the type of their operands and the equalities of
    /// the GADT branches they are in.
    equalities: Vec<(Type, Span, Vec<Type>)>,
    gadt: gadt::branch::State,
    /// The aliases unification and subsumption are unfolding: one met again
    /// inside its own unfolding is compared by its arguments.
    unfolding: Vec<regions::AliasKey>,
    /// The types of `do` blocks and of their expression statements, by
    /// range, for the lints of `sugar_lints`.
    sugar: Vec<(typing::Range, Type)>,
    recs: Vec<Rec>,
    top: toplevel::TopLevel<'a>,
    options: Options<'a>,
    classes: classes::State,
    typing: Typing,
    /// The predicate of each refined type of the module, by type name.
    predicates: HashMap<String, &'a Node>,
    /// The tree of the module being checked: a fix that depends on what is
    /// around a node finds the node from here.
    root: Option<&'a Node>,
    /// The types marked `@cell`, by module: this module's own, and those the
    /// interfaces it knows declare or pass on.
    cells: HashMap<String, Vec<String>>,
    /// The types marked `@label`: the module, the type and its kind, this
    /// module's first. A handful, so a list.
    labels: Vec<(String, String, String)>,
    /// The services marked `@owner`, each as a row spells it, `Module.Name`:
    /// this module's first.
    owners: Vec<String>,
    /// The applications of a watched name, whose types settle once the
    /// module is solved.
    calls: Vec<facts::PendingCall>,
    /// The fields of the record literals around the value being checked,
    /// outermost first: where a nested record's field is missing.
    field_path: Vec<String>,
    /// Each `_` written in a type, and the variable that stands for it.
    holes: Vec<(Span, Type, bool)>,
    /// Whether the type being converted may have holes.
    hole_ok: bool,
    /// Whether the module is a built-in one, written with the code the
    /// shape lints suggest against.
    builtin: bool,
    /// The effects of this module with a `control` operation, known before any
    /// type is converted: a row may name one declared further down.
    control_effects: HashSet<String>,
}
