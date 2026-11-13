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
mod braced_branch;
mod builtin_sources;
mod builtins;
mod calls;
mod cells;
pub(crate) mod children;
mod class_members;
mod classes;
mod comprehension;
mod convert;
mod debug;
mod decls;
mod display;
mod do_notation;
mod effects;
mod elem;
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
mod labels;
pub mod lang;
#[cfg(test)]
mod lint_tests;
mod lints;
mod makers;
mod module_contract;
mod module_signature;
mod mutation;
mod options;
pub(crate) use module_signature::normalize_module_interface;
mod static_modules;
pub(crate) use module_contract::{
    Mismatch as ModuleMismatch, check as check_module_contract,
    local_dictionaries as module_local_dictionaries,
};
mod expression_fact;
mod origin;
mod patterns;
mod pipes;
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
mod ts_forms;
mod ty;
mod typing;
mod ui;
mod unify;
mod variance;
mod watched;
pub mod wire;

use crate::numeric::NumLang;
pub use crate::syntax::structure::Thresholds;
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
pub use elem::{Atom, Elem};
pub use env::{
    Abi, EnumLit, Form, Fundep, Instance, JsClass, ModuleNominalKind, ModuleRequirement,
    ModuleSignature, Rep, TypeDef,
};
pub use ffi::{PURE_OVER_CAPABILITY, keeps_for_owner};
pub use gadt::branch::dict_local;
pub use gadt::scheme::{built, is_indexed, written_dicts};
pub use generalize::without_value_captures;
pub use interpolation::{hole_key, holes};
pub use kinds::Kind;
pub use options::{Limits, Options};
pub use patterns::is_pattern;
pub use pipes::{
    applied_order, composed_key, composition, param_key, result_key, right_application,
};
pub use question::Question;
pub use ranges::range_list;
pub use schemas::companion_name;
pub use services::{OpClass, Operation};
pub use ty::{Binder, Effects, Elided, Row, Stage, Type, stage_of};
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

#[derive(Clone, Copy, Default)]
pub(crate) struct StaticContext<'a> {
    pub dictionaries: &'a [(Type, Dict)],
    /// Transparent definitions available when comparing a concrete result.
    /// They are not added to the body's abstract parameter environment.
    pub witnesses: &'a [Interface],
}

/// Checks a named result with its static arguments' dictionaries and
/// equality context. Body lookup still uses only the projected interfaces.
pub(crate) fn check_typed_with_static_context(
    module: &Module,
    imported: &dyn Fn(&str) -> Option<Interface>,
    builtins: &[Interface],
    options: Options,
    interface: &mut Interface,
    context: StaticContext<'_>,
) -> (Vec<Diagnostic>, Typing) {
    let (diagnostics, typing, _) =
        run_with_static_context(module, imported, builtins, options, interface, context);
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
    let (diagnostics, typing, _) = run_in(
        module,
        imported,
        &known,
        options,
        interface,
        StaticContext::default(),
    );
    (diagnostics, typing)
}

fn run(
    module: &Module,
    imported: &dyn Fn(&str) -> Option<Interface>,
    builtins: &[Interface],
    options: Options,
    interface: &mut Interface,
) -> (Vec<Diagnostic>, Typing, Vec<(Span, Dict)>) {
    run_with_static_context(
        module,
        imported,
        builtins,
        options,
        interface,
        StaticContext::default(),
    )
}

fn run_with_static_context(
    module: &Module,
    imported: &dyn Fn(&str) -> Option<Interface>,
    builtins: &[Interface],
    options: Options,
    interface: &mut Interface,
    context: StaticContext<'_>,
) -> (Vec<Diagnostic>, Typing, Vec<(Span, Dict)>) {
    let owned;
    let builtins = if let Some(by_class) = builtins::are_builtins(builtins) {
        builtin_env(by_class)
    } else {
        owned = Builtins::new(builtins);
        &owned
    };
    let (mut diagnostics, typing, evidence) =
        run_in(module, imported, builtins, options, interface, context);
    // only the built-in modules, which `run_in` checks directly, mark lang
    // items
    for attribute in lang::attributes(module.tree) {
        if lang::lang_of(attribute, module.text).is_none() {
            continue;
        }
        let (start, end) = attribute.range();
        diagnostics.push(Diagnostic::error(
            codes::R016,
            "`@lang` marks what the compiler relies on, in the built-in modules only",
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
    context: StaticContext<'_>,
) -> (Vec<Diagnostic>, Typing, Vec<(Span, Dict)>) {
    let env = Env::new(module.name, module.syntax, imported, builtins);
    let mut checker = Checker {
        file: module.file,
        text: module.text,
        generated: module.generated,
        derived: derived_classes(module.tree, module.text),
        env,
        static_witnesses: context.witnesses,
        store: Store::default(),
        kinds: KindStore::default(),
        diagnostics: Vec::new(),
        cx: ItemCx::default(),
        signs: HashMap::new(),
        opened: Vec::new(),
        contract_types: Vec::new(),
        opened_modules: Vec::new(),
        sites: Vec::new(),
        literals: Vec::new(),
        signatures: Vec::new(),
        equalities: Vec::new(),
        recs: Vec::new(),
        top: toplevel::TopLevel::default(),
        options,
        classes: classes::State::default(),
        typing: Typing::default(),
        predicates: HashMap::new(),
        cells: HashMap::new(),
        labels: Vec::new(),
        owners: Vec::new(),
        root: None,
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
        unfolding: Vec::new(),
    };
    checker
        .classes
        .local
        .extend_from_slice(context.dictionaries);
    let static_parameters = checker.prepare_static_parameters(module);
    let static_results = checker.prepare_static_applications(module);
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
    checker.finish_static_module(
        module,
        imported,
        interface,
        static_parameters,
        &static_results,
    );
    checker.typing.declarations =
        Arc::new(checker.static_declarations(module, interface, &static_results));
    checker.sugar_lints(module.tree);
    let typing = checker.finish_typing();
    let mut evidence = checker.classes.evidence;
    evidence.sort_by_key(|(span, _)| (span.start, span.end));
    (checker.diagnostics, typing, evidence)
}

include!("beside_syntax.rs");

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
    binding: mutation::Binding,
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
    /// Each use with whether it is in the member's own body.
    uses: Vec<(Type, Span, bool)>,
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

/// What checking one declaration keeps of its own: a declaration that
/// another names before its turn is checked from inside that one, and
/// starts with none of this.
#[derive(Default)]
struct ItemCx {
    mutation: mutation::Frames,
    locals: Vec<Local>,
    tyvars: Vec<TyVar>,
    /// Whether the expression being inferred runs a runtime call at this
    /// stage; lambdas and `task` bodies are deferred and do not count.
    runtime: bool,
    /// The stage being checked: what its runtime calls may need.
    stage: effects::Stage,
    /// Row constraints of the declaration being checked.
    subrows: Vec<effects::SubRow>,
    operators: Vec<Operator>,
    regions: regions::State,
    /// A name was taken on trust: a class constraint left open has no type
    /// to choose an instance by, and no error of its own.
    trusted: bool,
}

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

include!("checker_helpers.rs");
