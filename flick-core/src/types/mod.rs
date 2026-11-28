//! Kinds, the type core and classes:
//! kind checking, bidirectional type checking against the full signature of
//! every named function, higher-rank `forall` with predicative instantiation,
//! exact records with row polymorphism, exhaustive `match`, and generalization
//! of provably pure bindings only; effects, expected errors, services and
//! layers; classes, instances, row primitives and `deriving`; regions and
//! the captures of closures. Rules:
//! `docs/src/content/docs/reference/types.md`, `docs/src/content/docs/reference/effects.md`,
//! `docs/src/content/docs/reference/services.md`, `docs/src/content/docs/reference/classes.md`,
//! `docs/src/content/docs/reference/regions.md`.
//!
//! The checker reads the concrete tree after name resolution; other modules
//! are known only through their [`Interface`], which this phase fills with
//! the types and kinds of the exports.

use std::mem;

mod abi;
mod applicative_do;
mod assumed_pure;
mod awaitable;
mod binary;
mod bindings;
mod braced_branch;
mod builtin_sources;
mod builtins;
mod call_args;
mod calls;
mod cells;
pub(crate) mod children;
mod class_members;
mod classes;
mod comprehension;
mod convert;
mod debug;
mod decls;
mod detached;
mod display;
mod do_notation;
mod effects;
mod elem;
mod enums;
mod env;
mod error_match;
mod exhaust;
mod exports;
mod facts;
mod ffi;
mod func_ref;
mod gadt;
mod generalize;
pub mod handle;
mod handlers;
mod holes;
mod host_exports;
#[cfg(test)]
mod host_exports_tests;
pub mod hot_swap;
mod infer;
mod instances;
mod interpolation;
#[cfg(not(target_arch = "wasm32"))]
mod ir_type_decls;
#[cfg(not(target_arch = "wasm32"))]
mod ir_types;
mod irrefutable;
mod kinds;
mod labels;
pub mod lang;
mod lazy;
mod let_lazy;
#[cfg(test)]
mod lint_tests;
mod lints;
mod makers;
mod module_contract;
mod module_signature;
mod mutation;
mod optional_paths;
mod options;
pub(crate) use module_signature::normalize_module_interface;
mod static_modules;
pub(crate) use module_contract::{
    Mismatch as ModuleMismatch, check as check_module_contract,
    local_dictionaries as module_local_dictionaries,
};
mod contracts;
mod expression_fact;
mod origin;
mod patterns;
mod pipes;
mod proof_bundles;
mod proof_data;
mod proof_decls;
mod proofs;
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
mod siblings;
mod signature_lints;
mod stub;
mod sugar_lints;
mod suspended;
mod toplevel;
mod ts_forms;
mod ty;
mod typing;
mod ui;
mod unify;
mod variance;
mod watched;
pub mod wire;

#[cfg(not(target_arch = "wasm32"))]
pub use crate::logic::{ProofSearchRequest, ProofSearchResult, ProofSearchStatus};
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
    implicit_go_bindings, implicit_package_of, implicit_target_dependencies, install_builtins,
    set_implicit_packages, try_builtins,
};
pub use class_members::class_defaults;
pub use classes::{Dict, Match, STRUCTURAL, match_type, which};
pub use comprehension::{comprehension, for_parts};
pub use contracts::ContractInfo;
pub use convert::is_type;
pub use display::{show_signature, show_type};
pub use do_notation::{bind_key, guard_key};
pub use elem::{Atom, Elem};
pub use env::{
    Abi, EnumLit, Form, Fundep, Instance, JsClass, ModuleNominalKind, ModuleRequirement,
    ModuleSignature, Rep, TypeDef,
};
pub use ffi::*;
pub use gadt::branch::dict_local;
pub use gadt::scheme::{built, is_indexed, written_dicts};
pub use generalize::without_value_captures;
#[cfg(feature = "ide")]
pub(crate) use handlers::catch_to_try_text;
pub use holes::{app_hole, has_hole};
pub use interpolation::{hole_key, holes};
pub use kinds::Kind;
pub use options::{Limits, Options, PluginAttribute};
pub use patterns::is_pattern;
pub use pipes::{
    applied_order, composed_key, composition, param_key, result_key, right_application,
};
pub use proof_bundles::record;
pub use proof_decls::{
    Bundle, GoalSyntax, GoalType, Law, LawObligation, ProofEvidence, ProofInfo, ProofStatus,
    goal_syntax,
};
pub use question::Question;
pub use ranges::range_list;
pub use records::accessor_keys;
pub use schemas::companion_name;
pub(crate) use services::impl_def;
pub use services::{OpClass, Operation};
#[cfg(feature = "ide")]
pub(crate) use ts_forms::describe_targets;
pub(crate) use ts_forms::is_reserved_core_attribute;
pub use ty::{Binder, Effects, Elided, MetaOccurrence, Row, Scope, ScopeId, Stage, Type, stage_of};
pub use typing::{
    Constructor, ForeignInfo, ForeignOwnerNode, ForeignOwnerPolicy, GenericInfo, Identity,
    InstanceInfo, Lift, Owner, OwnerAbstract, OwnerProductivity, OwnerRawHead, OwnerTypeExpr,
    OwnerTypeKind, OwnerTypeTemplates, Range, Ref, StageKey, Typing, default_method,
    passes_dictionary,
};
pub use variance::{
    Variance, decl_param_names, format_interface_summary, format_markdown,
    format_parameter_markdown, format_plain,
};
pub(crate) use variance::{param_kinds as variance_param_kinds, valid_exported_variance};
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
    check_typed_with_static_context(
        module,
        imported,
        builtins,
        options,
        interface,
        StaticContext {
            skip_ir_types: true,
            ..StaticContext::default()
        },
    )
    .0
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

pub(crate) use options::StaticContext;

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

/// Shared initialization for production checking and direct solver regressions.
fn checker_in<'a>(
    module: &Module<'a>,
    imported: &dyn Fn(&str) -> Option<Interface>,
    builtins: &'a Builtins,
    options: Options<'a>,
    context: StaticContext<'a>,
) -> Checker<'a> {
    let mut env = Env::new(module.name, module.syntax, imported, builtins);
    let support_errors = mem::take(&mut env.support_errors);
    let store = Store::default();
    let kinds = KindStore::for_witnesses(store.witness_authority());
    Checker {
        file: module.file,
        text: module.text,
        generated: module.generated,
        derived: derived_classes(module.tree, module.text),
        env,
        static_witnesses: context.witnesses,
        store,
        kinds,
        diagnostics: support_errors
            .into_iter()
            .map(|message| Diagnostic::error(codes::P010, message, Span::new(module.file, 0, 0)))
            .collect(),
        cx: ItemCx::default(),
        signs: HashMap::new(),
        class_signs: HashMap::new(),
        opened: Vec::new(),
        contract_types: Vec::new(),
        opened_modules: Vec::new(),
        witness_counter: 0,
        annotation_contexts: Vec::new(),
        named_witnesses: module_contract::NamedWitnesses::default(),
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
        proofs: proofs::Proofs::default(),
        cells: HashMap::new(),
        labels: Vec::new(),
        owners: Vec::new(),
        root: None,
        calls: Vec::new(),
        field_path: Vec::new(),
        func_refs: Vec::new(),
        given_param: None,
        piped_late: None,
        holes: Vec::new(),
        hole_ok: false,
        control_effects: HashSet::new(),
        builtin: builtins::builtin_sources()
            .iter()
            .any(|(name, _)| *name == module.name),
        sugar: Vec::new(),
        gadt: gadt::branch::State::default(),
        error_rows: None,
        unfolding: Vec::new(),
        skip_ir_types: context.skip_ir_types,
    }
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
    let mut checker = checker_in(module, imported, builtins, options, context);
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
    checker.discharge_proofs();
    checker.unannotated_codecs();
    checker.populate_own_variances();
    checker.fill(interface);
    checker.finish_static_module(
        module,
        imported,
        interface,
        static_parameters,
        &static_results,
    );
    if !checker
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error)
    {
        match checker.collect_support_types(interface) {
            Ok(support) => interface.support_types = support,
            Err(message) => checker.error(codes::P010, Span::new(module.file, 0, 0), message),
        }
    }
    checker.typing.declarations =
        Arc::new(checker.static_declarations(module, interface, &static_results));
    checker.sugar_lints(module.tree);
    let typing = checker.finish_typing_with(context);
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

/// What checking one declaration keeps of its own: a declaration that
/// another names before its turn is checked from inside that one, and
/// starts with none of this.
#[derive(Default)]
struct ItemCx {
    mutation: mutation::Frames,
    locals: Vec<Local>,
    /// Active expression-hole parameters: `(hole_range, index_in_locals)`.
    hole_locals: Vec<(Range, usize)>,
    tyvars: Vec<TyVar>,
    /// Whether the expression being inferred runs a runtime call at this
    /// stage; lambdas and `task` bodies are deferred and do not count.
    runtime: bool,
    /// The stage being checked: what its runtime calls may need.
    stage: effects::Stage,
    /// Row constraints of the declaration being checked.
    subrows: Vec<effects::SubRow>,
    /// Open row tails added when widening a closed row that fixed a type
    /// variable under subsumption, closed to empty when settled.
    widened_rows: Vec<Type>,
    operators: Vec<Operator>,
    regions: regions::State,
    /// A name was taken on trust: a class constraint left open has no type
    /// to choose an instance by, and no error of its own.
    trusted: bool,
    /// The proof being checked, where its rules are names.
    proof: Option<Range>,
    /// The parameters of the class whose law is being checked: applied,
    /// each is a type the instance of a bundle fills in.
    law: Vec<u32>,
    /// Locals applied before their type was known.
    assumed_pure: Vec<assumed_pure::Assumption>,
    /// Where the parameters of each local function without a signature are
    /// bound, by where the function is bound; `None` for a pattern.
    local_params: HashMap<u32, Vec<Option<u32>>>,
    /// The argument being checked meets this parameter: where it is bound
    /// and its type.
    parameter: Option<(u32, Type)>,
    /// The variables an operator kept from being generalized, each with
    /// that operation and its operator.
    kept_operands: Vec<(u32, Span, &'static str)>,
    /// The operand of an `await` being typed is a name: a `let lazy` may be
    /// read there.
    awaiting: bool,
    /// What the path to the expression being checked establishes.
    facts: Vec<proofs::Fact>,
    /// The facts around each body that runs apart, set aside.
    held_facts: Vec<Vec<proofs::Fact>>,
    /// Nesting depth inside postfix `catch` arms, where `resume` is never bound.
    catch_arm: u32,
}

include!("checker_state.rs");

include!("checker_helpers.rs");

#[cfg(test)]
mod witness_tests;
