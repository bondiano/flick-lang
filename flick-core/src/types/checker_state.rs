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
    /// The sign of each parameter of the classes met so far.
    class_signs: variance::Signs,
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
    annotation_contexts: Vec<Vec<ty::LexicalScope>>,
    named_witnesses: module_contract::NamedWitnesses,
    sites: Vec<effects::Site>,
    literals: Vec<NumLiteral>,
    signatures: Vec<bindings::PendingSignature>,
    /// `==` and `!=` with the type of their operands, the operator and the
    /// equalities of the GADT branches they are in.
    equalities: Vec<(Type, Span, &'static str, Vec<Type>)>,
    gadt: gadt::branch::State,
    /// While a pattern of a `match` is checked, the rows it added an error
    /// to, each by the variable that stands for the rest of it.
    error_rows: Option<Vec<Type>>,
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
    /// The invariants of the module's types and what must be proved of them.
    proofs: proofs::Proofs<'a>,
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
    /// The type of the first parameter of a function inferred where it is
    /// applied (`.email user`, `(fun r -> r.id) p`): its argument, checked
    /// first, gave it. The function opened next takes it.
    given_param: Option<Type>,
    /// While the function of a pipeline (`users |> List.map .town`) is an
    /// application being inferred, the accessors among its arguments whose
    /// type the pipeline's argument gives: each by its index, with the
    /// parameter type to check it against once the pipeline has applied it.
    piped_late: Option<Vec<(usize, Type)>>,
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
