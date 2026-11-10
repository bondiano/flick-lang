//! Kinds, the type core and classes:
//! kind checking, bidirectional type checking against the full signature of
//! every named function, higher-rank `forall` with predicative instantiation,
//! exact records with row polymorphism, exhaustive `match`, and generalization
//! of provably pure bindings only; effects, expected errors, services and
//! layers; classes, instances, row primitives and `deriving`; regions and
//! the captures of closures. Rules:
//! `docs/src/content/docs/reference/types.md`, `docs/src/content/docs/reference/effects.md`,
//! `docs/src/content/docs/reference/classes.md`, `docs/src/content/docs/reference/regions.md`.
//!
//! The checker reads the concrete tree after name resolution; other modules
//! are known only through their [`Interface`], which this phase fills with
//! the types and kinds of the exports.

mod abi;
mod applicative_do;
mod bindings;
mod builtin_sources;
mod builtins;
mod calls;
mod class_members;
mod classes;
mod comprehension;
mod convert;
mod debug;
mod decls;
mod display;
mod do_notation;
mod effects;
mod enums;
mod env;
mod exhaust;
mod exports;
mod facts;
mod ffi;
mod gadt;
mod generalize;
pub mod handle;
mod handlers;
mod infer;
mod instances;
mod interpolation;
mod irrefutable;
mod kinds;
pub mod lang;
#[cfg(test)]
mod lint_tests;
mod lints;
mod makers;
mod module_contract;
pub(crate) use module_contract::{
    Mismatch as ModuleMismatch, check as check_module_contract,
    local_dictionaries as module_local_dictionaries, project as project_module_contract,
};
mod expression_fact;
mod origin;
mod patterns;
mod qualified;
mod question;
mod ranges;
mod records;
mod recursion;
mod redundant;
mod regions;
mod report;
mod route_template;
mod rows;
mod schemas;
mod server;
mod services;
mod settle;
mod shown;
mod signature_lints;
mod stub;
mod sugar_lints;
mod toplevel;
mod ty;
mod typing;
mod ui;
mod unify;
mod watched;
pub mod wire;

use crate::numeric::NumLang;
pub use applicative_do::{and_keys, group_key, pure_argument};
pub use bindings::{LetParts, let_parts, signature_name};
use builtins::builtin_env;
#[cfg(test)]
pub(crate) use builtins::check_sources;
#[cfg(feature = "native-project")]
pub use builtins::read_package;
pub use builtins::{
    ImplicitPackage, builtin_sources, builtins, builtins_of, check_each, has_implicit_packages,
    implicit_go_bindings, implicit_js_dependencies, implicit_package_of, install_builtins,
    set_implicit_packages, try_builtins,
};
pub use class_members::class_defaults;
pub use classes::{Dict, Match, STRUCTURAL, match_type, which};
pub use comprehension::{comprehension, for_parts};
pub use convert::is_type;
pub use display::{show_signature, show_type};
pub use do_notation::{bind_key, guard_key};
pub use effects::NO_EFFECTS;
pub use env::{
    Abi, EnumLit, Form, Fundep, Instance, JsClass, ModuleNominalKind, ModuleRequirement, Rep,
    TypeDef,
};
pub use ffi::keeps_for_owner;
pub use gadt::branch::dict_local;
pub use gadt::scheme::{built, is_indexed, written_dicts};
pub use generalize::without_value_captures;
pub use interpolation::{hole_key, holes};
pub use kinds::Kind;
pub use patterns::is_pattern;
pub use question::Question;
pub use ranges::range_list;
pub use regions::LIVE;
pub use schemas::companion_name;
pub use services::{OpClass, Operation};
pub use ty::{Binder, Effects, Row, SYNC_SERVICES, Stage, Type, stage_of};
pub use typing::{
    Constructor, ForeignInfo, GenericInfo, Identity, InstanceInfo, Lift, Owner, Range, Ref, Typing,
    default_method, passes_dictionary,
};
pub use watched::Watched;

use convert::TyVar;
use env::{Builtins, Env};
use kinds::KindStore;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use unify::Store;

use crate::diagnostics::{Code, Diagnostic, Severity, codes};
use crate::iface::{Interface, Namespace};
use crate::numeric::Num;
use crate::plugin::derive::{Derived, derived_classes};
use crate::resolve::Module;
use crate::source::{FileId, Span};
use crate::syntax::cst::{Node, Token};
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::Lang;

/// Bounds of instance resolution for one definition; exceeding them is a
/// diagnostic, never a success.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub steps: u32,
    pub depth: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            steps: 100_000,
            depth: 256,
        }
    }
}

/// `[lint]` thresholds: a function past one gets a warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thresholds {
    pub function_lines: u32,
    pub nesting: u32,
    pub complexity: u32,
    pub params: u32,
}

impl Default for Thresholds {
    fn default() -> Self {
        Thresholds {
            function_lines: 60,
            nesting: 4,
            complexity: 10,
            params: 5,
        }
    }
}

/// What checking a module needs beyond its imports.
#[derive(Clone, Copy)]
pub struct Options<'a> {
    /// Whether a module belongs to the package being checked: only the
    /// package of a class or of the type an instance is for may declare it.
    pub local: &'a dyn Fn(&str) -> bool,
    pub limits: Limits,
    /// `[lint] no-let-assert`: every `let assert` is an error.
    pub no_let_assert: bool,
    pub thresholds: Thresholds,
    /// Whether runtime code may run: `false` under `effect = "none"`.
    pub effects: bool,
    /// The names whose places of call the plugins of the project watch:
    /// each one checked gives a [`Typing::call_sites`].
    pub watched: &'a Watched,
    /// Whether a stage comes from the classes of the operations of its row, as
    /// on Flick's own runtime: see [`Effects::direct`]. Every module of a build
    /// and the built-in modules it reads are checked alike.
    pub stages_by_class: bool,
}

/// Type-checks `module` and stores the types of its exports in `interface`.
/// `imported` gives the interfaces of its imports, `builtins` those of the
/// built-in modules (see [`builtins`]).
pub fn check(
    module: &Module,
    imported: &dyn Fn(&str) -> Option<Interface>,
    builtins: &[Interface],
    options: Options,
    interface: &mut Interface,
) -> Vec<Diagnostic> {
    check_typed(module, imported, builtins, options, interface).0
}

/// [`check`], also returning the dictionary each class constraint resolved
/// to, by the span that required it.
pub fn check_with_evidence(
    module: &Module,
    imported: &dyn Fn(&str) -> Option<Interface>,
    builtins: &[Interface],
    options: Options,
    interface: &mut Interface,
) -> (Vec<Diagnostic>, Vec<(Span, Dict)>) {
    let (diagnostics, _, evidence) = run(module, imported, builtins, options, interface);
    (diagnostics, evidence)
}

/// [`check`], also returning what code generation needs to know.
pub fn check_typed(
    module: &Module,
    imported: &dyn Fn(&str) -> Option<Interface>,
    builtins: &[Interface],
    options: Options,
    interface: &mut Interface,
) -> (Vec<Diagnostic>, Typing) {
    let (diagnostics, typing, _) = run(module, imported, builtins, options, interface);
    (diagnostics, typing)
}

/// Checks a named parameterized module result with the dictionaries supplied by its
/// static module arguments in scope for every definition of the result.
pub(crate) fn check_typed_with_module_dictionaries(
    module: &Module,
    imported: &dyn Fn(&str) -> Option<Interface>,
    builtins: &[Interface],
    options: Options,
    interface: &mut Interface,
    dictionaries: &[(Type, Dict)],
) -> (Vec<Diagnostic>, Typing) {
    let (diagnostics, typing, _) =
        run_with_dictionaries(module, imported, builtins, options, interface, dictionaries);
    (diagnostics, typing)
}

/// Checks a built-in module for editor queries against the built-in modules
/// before it, allowing the compiler's `@lang` marks in its source.
pub fn check_builtin_typed(
    module: &Module,
    imported: &dyn Fn(&str) -> Option<Interface>,
    dependencies: &[Interface],
    options: Options,
    interface: &mut Interface,
) -> (Vec<Diagnostic>, Typing) {
    let known = Builtins::new(dependencies);
    let (diagnostics, typing, _) = run_in(module, imported, &known, options, interface, &[]);
    (diagnostics, typing)
}

fn run(
    module: &Module,
    imported: &dyn Fn(&str) -> Option<Interface>,
    builtins: &[Interface],
    options: Options,
    interface: &mut Interface,
) -> (Vec<Diagnostic>, Typing, Vec<(Span, Dict)>) {
    run_with_dictionaries(module, imported, builtins, options, interface, &[])
}

fn run_with_dictionaries(
    module: &Module,
    imported: &dyn Fn(&str) -> Option<Interface>,
    builtins: &[Interface],
    options: Options,
    interface: &mut Interface,
    dictionaries: &[(Type, Dict)],
) -> (Vec<Diagnostic>, Typing, Vec<(Span, Dict)>) {
    let owned;
    let builtins = if let Some(by_class) = builtins::are_builtins(builtins) {
        builtin_env(by_class)
    } else {
        owned = Builtins::new(builtins);
        &owned
    };
    let (mut diagnostics, typing, evidence) =
        run_in(module, imported, builtins, options, interface, dictionaries);
    // only the built-in modules, which `run_in` checks directly, mark lang items
    let marks = lang::attributes(module.tree);
    for attribute in marks
        .into_iter()
        .filter(|a| lang::lang_of(a, module.text).is_some())
    {
        let (start, end) = attribute.range();
        diagnostics.push(Diagnostic::error(
            codes::R016,
            "`@lang` marks what the compiler relies on, in the built-in modules only".to_owned(),
            Span::new(module.file, start, end),
        ));
    }
    (diagnostics, typing, evidence)
}

/// [`run`] over built-in modules already learned.
fn run_in(
    module: &Module,
    imported: &dyn Fn(&str) -> Option<Interface>,
    builtins: &Builtins,
    options: Options,
    interface: &mut Interface,
    dictionaries: &[(Type, Dict)],
) -> (Vec<Diagnostic>, Typing, Vec<(Span, Dict)>) {
    let env = Env::new(module.name, module.syntax, imported, builtins);
    let mut checker = Checker {
        file: module.file,
        text: module.text,
        generated: module.generated,
        derived: derived_classes(module.tree, module.text),
        env,
        store: Store::default(),
        kinds: KindStore::default(),
        diagnostics: Vec::new(),
        locals: Vec::new(),
        ui: 0,
        tyvars: Vec::new(),
        contract_types: Vec::new(),
        opened_modules: Vec::new(),
        runtime: false,
        stage: effects::Stage::default(),
        sites: Vec::new(),
        subrows: Vec::new(),
        operators: Vec::new(),
        literals: Vec::new(),
        signatures: Vec::new(),
        equalities: Vec::new(),
        recs: Vec::new(),
        top: toplevel::TopLevel::default(),
        options,
        classes: classes::State::default(),
        regions: regions::State::default(),
        typing: Typing::default(),
        predicates: HashMap::new(),
        calls: Vec::new(),
        field_path: Vec::new(),
        holes: Vec::new(),
        hole_ok: false,
        control_effects: HashSet::new(),
        builtin: builtins::builtin_sources()
            .iter()
            .any(|(name, _)| *name == module.name),
        sugar: Vec::new(),
        gadt: gadt::branch::State::default(),
    };
    checker.classes.local.extend_from_slice(dictionaries);
    checker.module(module.tree);
    for export in &interface.exports {
        if export.entity.module == module.name
            && export.entity.namespace == Namespace::Value
            && !checker.top.has_signature(&export.entity.name)
            && checker
                .env
                .value(&export.entity)
                .is_some_and(|ty| ty.contains(&mut |part| matches!(part, Type::Exists { .. })))
        {
            checker.error(
                codes::T064,
                export.at.map_or_else(|| Span::new(module.file, 0, 0), |[start, end]| Span::new(module.file, start, end)),
                format!("public value `{}` contains an abstract module witness; write an explicit `exists K : Sig. T` signature immediately above its definition", export.entity.name),
            );
        }
    }
    checker.report_holes();
    checker.record_call_sites();
    checker.unannotated_codecs();
    checker.fill(interface);
    checker.typing.declarations = Arc::new(checker.declarations());
    checker.sugar_lints(module.tree);
    let typing = checker.finish_typing();
    let mut evidence = checker.classes.evidence;
    evidence.sort_by_key(|(span, _)| (span.start, span.end));
    (checker.diagnostics, typing, evidence)
}

/// The diagnostics of checking a module with syntax errors that do not
/// touch the code the parser could not read, where the checker only guessed.
pub fn beside_syntax_errors(tree: &Node, checked: Vec<Diagnostic>) -> Vec<Diagnostic> {
    fn errors(node: &Node, out: &mut Vec<(u32, u32)>) {
        for child in node.nodes() {
            if child.kind == K::Error {
                out.push(child.range());
            } else {
                errors(child, out);
            }
        }
    }
    let mut broken = Vec::new();
    errors(tree, &mut broken);
    // a type the checker could not finish is no news while code is missing
    let unfinished = |text: &str| text.contains("?`") || text.contains("? ");
    let guessed = |d: &Diagnostic| d.code == codes::T013 || unfinished(&d.message) || d.incomplete;
    checked
        .into_iter()
        .filter(|d| {
            (broken.is_empty() || !guessed(d))
                && d.span
                    .is_none_or(|s| !broken.iter().any(|(b, e)| s.start <= *e && *b <= s.end))
        })
        .collect()
}

/// A value in scope inside a declaration.
#[derive(Debug, Clone)]
struct Local {
    name: String,
    /// Where its name is written; none for a field a record pattern binds by
    /// its label.
    at: Option<u32>,
    /// Whether a plugin generated it: only generated names see it.
    generated: bool,
    ty: Type,
    /// A function of a recursive group used before its type is proved, by
    /// index into [`Checker::recs`].
    rec: Option<usize>,
    /// A local function without a signature: it has one type, from its body
    /// and its first uses.
    unsigned: bool,
}

/// A recursive function whose signature has no explicit `forall`: its
/// recursive uses must have the type being checked, up to the names of the
/// variables of its group.
#[derive(Debug, Clone, Default)]
struct Rec {
    name: String,
    /// The signature, and its body with the skolems its body is checked
    /// against, by variable.
    scheme: Option<Type>,
    target: Option<Type>,
    skolems: Vec<(String, Type)>,
    uses: Vec<(Type, Span)>,
}

/// An operator whose operand type is checked once inference settles it.
#[derive(Debug, Clone)]
struct Operator {
    ty: Type,
    span: Span,
    op: &'static str,
    domain: Domain,
    /// The class constraints in scope where it is written.
    givens: Vec<Type>,
}

/// The operand types an operator accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Domain {
    /// The numeric types: arithmetic.
    Numeric,
    /// The numeric types and `String`: comparisons.
    Ordered,
    /// `String` and any list: `++`.
    Concat,
}

impl Domain {
    /// The built-in types it accepts; `List` means any list. An operand
    /// type nothing determined becomes the first.
    fn names(self) -> Vec<Lang> {
        let numeric = Num::ALL.into_iter().map(Num::lang);
        match self {
            Domain::Numeric => numeric.collect(),
            Domain::Ordered => numeric.chain([Lang::String]).collect(),
            Domain::Concat => vec![Lang::String, Lang::List],
        }
    }

    fn allows(self, head: &Type) -> bool {
        self.names().into_iter().any(|item| head.is_lang(item))
    }
}

/// A number literal, whose type is any numeric type, a `Float` one when it
/// has a fraction: the one inference settles, else `Int` or `Float`.
#[derive(Debug, Clone)]
struct NumLiteral {
    ty: Type,
    span: Span,
    /// The integer it spells; `None` when the digits do not fit 128 bits
    /// or it has a fraction.
    value: Option<i128>,
    fractional: bool,
    /// A literal pattern, which a type variable cannot match.
    pattern: bool,
    /// The class constraints in scope where it is written.
    givens: Vec<Type>,
}

struct Checker<'a> {
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
    locals: Vec<Local>,
    /// How many `ui { … }` blocks enclose the expression: inside one, the
    /// names `Ui` exports need no `Ui.`.
    ui: usize,
    tyvars: Vec<TyVar>,
    /// Capitalized type members visible only while checking a `type module` body.
    contract_types: Vec<(String, Type, kinds::Kind)>,
    /// Lexically opened `Module Sig` values, innermost first on lookup.
    opened_modules: Vec<module_contract::Opened>,
    /// Whether the expression being inferred runs a runtime call at this
    /// stage; lambdas and `task` bodies are deferred and do not count.
    runtime: bool,
    /// The stage being checked: what its runtime calls may need.
    stage: effects::Stage,
    sites: Vec<effects::Site>,
    /// Row constraints of the declaration being checked.
    subrows: Vec<effects::SubRow>,
    operators: Vec<Operator>,
    literals: Vec<NumLiteral>,
    signatures: Vec<bindings::PendingSignature>,
    /// `==` and `!=` with the type of their operands and the equalities of
    /// the GADT branches they are in.
    equalities: Vec<(Type, Span, Vec<Type>)>,
    gadt: gadt::branch::State,
    /// The types of `do` blocks and of their expression statements, by
    /// range, for the lints of `sugar_lints`.
    sugar: Vec<(typing::Range, Type)>,
    recs: Vec<Rec>,
    top: toplevel::TopLevel<'a>,
    options: Options<'a>,
    classes: classes::State,
    regions: regions::State,
    typing: Typing,
    /// The predicate of each refined type of the module, by type name.
    predicates: HashMap<String, &'a Node>,
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

include!("checker_helpers.rs");
