//! What code generation needs to know from type checking, recorded by the
//! source range of the node it is about: what a name refers to, which
//! application stages run through the runtime, where a pure function is lifted
//! into a runtime position, and which dictionaries a use of a constrained
//! value receives.

pub use flick_ir::{Constructor, GenericInfo, InstanceInfo, Lift, default_method};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use super::classes::Dict;
use super::env::Abi;
use super::gadt::branch::as_field_given;
#[cfg(not(target_arch = "wasm32"))]
use super::kinds::Kind;
use super::services::OpClass;
use super::ty::{Stage, Type};
use crate::diagnostics::Diagnostic;
#[cfg(not(target_arch = "wasm32"))]
use crate::iface::Entity;
use crate::iface::Interface;
use crate::numeric::Num;
use crate::source::Span;
use crate::types::lang::Lang;
use flick_ir::{Adapter, ForeignMode, Label};

mod finish;
mod owner_productivity;
mod owner_templates;
pub use owner_productivity::{OwnerAbstract, OwnerProductivity, OwnerRawHead};
mod owners;
pub use owner_templates::{OwnerTypeExpr, OwnerTypeKind, OwnerTypeTemplates};
mod record;
mod stages;
pub use owners::{ForeignOwnerNode, ForeignOwnerPolicy, Owner, StageKey};
use owners::{OwnerContext, OwnerStage};

/// A pending constructor use before the module is solved: `(range, ctor_entity, kinded_type_args)`.
#[cfg(not(target_arch = "wasm32"))]
pub(super) type PendingConUse = (Range, Entity, Vec<(Kind, Type)>);

/// A source range of the module being checked.
pub type Range = (u32, u32);

#[cfg(feature = "ide")]
/// One service obligation escaping a `result { … }` block before rows settle:
/// `(actual_row, excluded_labels, scope_region)`.
pub(crate) type PendingResultService = (Type, Vec<super::Elem>, Option<Type>);

pub fn range(span: Span) -> Range {
    (span.start, span.end)
}

#[derive(Debug, Default)]
pub struct Typing {
    /// Every top-level entity of the module with its type (see
    /// [`Checker::declarations`]).
    pub declarations: Arc<Interface>,
    /// Every name in an expression or a constructor pattern.
    pub names: HashMap<Range, Ref>,
    #[cfg(feature = "ide")]
    /// Member spellings that would dispatch through an in-scope class or
    /// service before the module of a qualified method reference.
    pub method_dispatch: HashMap<Range, Vec<String>>,
    /// Accesses to local mutable cells, for order- and capture-sensitive fixes.
    pub mutable_reads: HashSet<Range>,
    /// The public runtime values captured by each `pack M as Sig`.
    pub packs: HashMap<Range, Vec<(String, String, String)>>,
    /// The binder of each name of the user's code that is a local, where an
    /// editor cannot find it in the tree: in a block, generated code decides
    /// which binder a name sees.
    pub binders: HashMap<Range, Range>,
    /// The representation of each `unboxed` or `untagged` type of the module.
    pub abis: HashMap<String, Abi>,
    /// Applications `f a1 … ai` (from `f` to `ai`) and pipes `x |> f` whose
    /// stage runs a runtime call.
    pub runtime: HashSet<Range>,
    /// Those whose stage runs a Sync call.
    pub sync: HashSet<Range>,
    /// The applications whose arguments from this position on are the
    /// elements of the list their function takes last, by the range of the
    /// application.
    pub gathered: HashMap<Range, usize>,
    /// The blocks of lines whose expression lines are the list a function
    /// of them takes, as a `ui` of several: the function, by the block.
    pub siblings: HashMap<Range, Ref>,
    /// The applications of a value a block applies through a function, as a
    /// foreign component inside `ui`: the function, by its module and name.
    pub through: HashMap<Range, Ref>,
    /// The `and` groups of `do` followed only by `pure e`, whose last
    /// `map2` computes `e`.
    pub applicative: HashSet<Range>,
    /// What each postfix `?` does.
    pub questions: HashMap<Range, super::Question>,
    /// Worker of a built-in `Awaitable.await` selected by the class solver.
    pub builtin_awaits: HashMap<Range, Lang>,
    /// The parameters each top-level signature marks `~`, by position.
    pub suspended: Vec<(String, Vec<usize>)>,
    /// Arguments passed unstarted to a parameter marked `~`: the task of each.
    pub suspends: HashSet<Range>,
    /// `await`s of a `let lazy`, which read the variable of its state.
    pub read_lazy: HashSet<Range>,
    /// `let lazy` declarations: a variable of their function, which holds
    /// the initializer until an `await` runs it.
    pub lazy_lets: HashSet<Range>,
    /// Assignments to a `let lazy mut`, which set its state to the value.
    pub lazy_sets: HashSet<Range>,
    /// Lazy `for`s whose body makes Sync calls: each step runs with the
    /// services of the stage that built the sequence.
    pub sync_generators: HashSet<Range>,
    /// Functions written with parameters, `fun` and `let`: the stage of the
    /// arrow of each parameter.
    pub stages: HashMap<Range, Vec<Stage>>,
    /// Functions accepted where one of another stage is expected.
    pub lifts: HashMap<Range, Lift>,
    /// Unique ground conversions at value boundaries.
    pub coercions: HashMap<Range, Coercion>,
    /// Lifts applied to the inner application body of an expression-hole capture.
    pub hole_body_lifts: HashMap<Range, Lift>,
    /// Implicit conversions applied to the inner application body of an expression-hole capture.
    pub hole_body_coercions: HashMap<Range, Coercion>,
    /// The numeric type of arithmetic and integer literals.
    pub numeric: HashMap<Range, Num>,
    /// Operators on a type variable, by `Numeric` or by `Ord`.
    pub generic_numeric: HashSet<Range>,
    pub generic_ordered: HashSet<Range>,
    /// `++` on lists rather than strings.
    pub list_concat: HashSet<Range>,
    /// `match` arms whose pattern matches no value of its type: a GADT
    /// constructor the type of the matched value rules out.
    pub dead_arms: HashSet<Range>,
    /// The class and the number of dictionaries of the operation each arm
    /// of a handler is for, by its pattern, and whether the arm covers it.
    pub handled: HashMap<Range, (OpClass, usize, bool)>,
    /// The operations of the service of each `provide S impl`, by its node:
    /// name, number of dictionaries and stages.
    pub provided_ops: HashMap<Range, Vec<(String, usize, Vec<Stage>)>>,
    /// `==` and `!=` that JS `===` decides: on the values themselves, or on
    /// their tags when every constructor of the type has no fields.
    pub identity_eq: HashMap<Range, Identity>,
    /// Dictionaries a use receives, in the order of its constraints.
    pub dicts: HashMap<Range, Vec<Dict>>,
    /// `C.dictionary { ... }`: written superclass fields and their runtime slots.
    pub dictionary_constructors: HashMap<Range, Vec<(String, String)>>,
    /// `unpack` bodies whose result carries an abstract type witness.
    pub existential_unpacks: HashSet<Range>,
    /// `unpack` expressions opening an existential pair with a value binder.
    pub existential_opens: HashSet<Range>,
    /// The constraints a definition, a method or a higher-rank argument
    /// assumes, as dictionaries name them (`Dict::Given`), in order.
    pub givens: HashMap<Range, Vec<String>>,
    /// The label of each use of `Record.label`.
    pub labels: HashMap<Range, String>,
    /// The top-level atoms, views and actions, which debugging tools know
    /// by `Module.name`.
    pub labelled: BTreeMap<String, Label>,
    /// The top-level functions that make an atom, an action or a view, each
    /// with the maker of the library it ends at.
    pub makers: HashMap<String, String>,
    /// The types of the module marked `@cell`: a value of one holds one value.
    pub cells: Vec<String>,
    /// The services of the module marked `@owner`.
    pub owner_services: Vec<String>,
    /// The top-level values that reach `Server`, each with its path to it.
    pub servers: HashMap<String, Vec<String>>,
    /// Each `foreign`: its calling convention.
    pub foreigns: HashMap<Range, ForeignInfo>,
    /// Parsed/catch-typed original declarations with unfinished representation.
    pub(super) pending_foreigns: Vec<super::ffi::PendingForeign>,
    /// Instances declared or derived in this module.
    pub instances: Vec<InstanceInfo>,
    /// The codec each use of `HttpApi.Derive.wire_codec` stands for.
    pub wire_codecs: HashMap<Range, super::wire::WireCodec>,
    /// The type of each `_` of generated code, for its plugin's `check`.
    pub wildcards: Vec<(Range, flick_plugin::Fact)>,
    /// Each place of the module that calls a name a plugin watches, for its
    /// `check_calls`; the spans are of the text that was checked.
    pub call_sites: Vec<flick_plugin::CallSite>,
    /// The uses of `wire_codec` an annotation has not given a codec yet.
    pub(super) pending_wire: Vec<(Range, Span)>,
    #[cfg(feature = "ide")]
    /// The type of each name in an expression or constructor pattern and
    /// of each variable a pattern binds, for editors; at a keyword that opens
    /// an expression (`task`, `match`, `fun` …) the type of the whole expression, at
    /// `ignore` and `await` the type of the operand, at `|>` the type after
    /// the stage.
    pub types: HashMap<Range, Type>,
    #[cfg(feature = "ide")]
    /// Checked signatures by the range of their type syntax, including
    /// signatures written on instance methods.
    pub signature_types: HashMap<Range, Type>,
    /// The type each type name refers to: its module and name.
    pub type_names: HashMap<Range, (String, String)>,
    /// Pending types also feed compiler diagnostics.
    pub(crate) pending_types: Vec<(Range, Type)>,
    /// The type of each expression, name and binder whose type is a type
    /// constructor on its own (`Float`, `String`), for code generation: an
    /// expression checked against a type keeps the type it was inferred
    /// with, which a lift around it changes. Only those, so that recording
    /// stays linear in the size of the module.
    pub values: HashMap<Range, Type>,
    pending_values: Vec<(Range, Type)>,
    /// Full interned IR types for expressions, names and binders (populated
    /// when IR lowering is requested).
    #[cfg(not(target_arch = "wasm32"))]
    pub ir_values: HashMap<Range, flick_ir::TypeRef>,
    /// Post-coercion target types at ranges with implicit coercions.
    #[cfg(not(target_arch = "wasm32"))]
    pub ir_coercion_targets: HashMap<Range, flick_ir::TypeRef>,
    /// Layout-relevant type arguments instantiating polymorphic globals,
    /// constructors, and methods, keyed by use range.
    #[cfg(not(target_arch = "wasm32"))]
    pub ir_type_args: HashMap<Range, Vec<flick_ir::TypeRef>>,
    /// Instantiated field types at constructor uses (`ExprKind::Con`, `Pat::Con`).
    #[cfg(not(target_arch = "wasm32"))]
    pub ir_con_fields: HashMap<Range, Vec<flick_ir::TypeRef>>,
    /// Module type declarations (data, error, nominal) with constructor fields.
    #[cfg(not(target_arch = "wasm32"))]
    pub ir_type_decls: Vec<flick_ir::TypeDecl>,
    /// Lowered schemes of top-level values and foreigns:
    /// `name -> (type_params, dict_constraints, ty)`.
    #[cfg(not(target_arch = "wasm32"))]
    pub ir_top_schemes:
        HashMap<String, (Vec<String>, Vec<flick_ir::Constraint>, flick_ir::TypeRef)>,
    /// Lowered schemes of local `let` / `let rec` functions keyed by `K::LetDecl` range.
    #[cfg(not(target_arch = "wasm32"))]
    pub ir_local_schemes:
        HashMap<Range, (Vec<String>, Vec<flick_ir::Constraint>, flick_ir::TypeRef)>,
    /// Declared constructor field types for constructors of this module, by name.
    #[cfg(not(target_arch = "wasm32"))]
    pub ir_ctor_fields: HashMap<String, Vec<flick_ir::TypeRef>>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) pending_ir_values: Vec<(Range, Type)>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) pending_coercion_targets: Vec<(Range, Type)>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) pending_type_args: Vec<(Range, Vec<(Kind, Type)>)>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) pending_con_uses: Vec<PendingConUse>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) pending_ir_local_schemes: Vec<(Range, Type)>,
    #[cfg(feature = "ide")]
    /// The record each field label belongs to, at the label: in a field
    /// access, a literal, an update and a record pattern.
    pub fields: HashMap<Range, Type>,
    #[cfg(feature = "ide")]
    pub(crate) pending_fields: Vec<(Range, Type)>,
    #[cfg(feature = "ide")]
    /// What the type at a range is without the equalities of the GADT
    /// branch it is in, where they change it: hover shows `Int (= a)`.
    pub refined: HashMap<Range, Type>,
    #[cfg(feature = "ide")]
    pub(crate) pending_refined: Vec<(Range, Type)>,
    #[cfg(feature = "ide")]
    /// Remaining service requirements of a `result { … }` block, keyed by its
    /// `result` keyword range, for hover.
    pub result_services: HashMap<Range, Type>,
    #[cfg(feature = "ide")]
    pub(crate) pending_result_services: Vec<(Range, Vec<PendingResultService>)>,
    #[cfg(feature = "ide")]
    /// The transparent aliases the module declares, exported or not: their
    /// parameters and bodies.
    pub aliases: HashMap<String, (Vec<String>, Type)>,
    /// Calls whose solved Scope requirement identifies a lexical owner.
    pub owners: HashMap<Range, Owner>,
    #[cfg(feature = "ide")]
    /// Lexical owner and mode resolved for each `defer` / `errdefer` statement,
    /// keyed by its keyword range.
    pub defers: HashMap<Range, DeferInfo>,
    /// Pending: uses whose types are known once the module is solved.
    pending_labels: Vec<(Range, Type)>,
    /// The services of a call that needs, or whose row may still grow to
    /// need, `Scope r`, and the lexical evidence available at the call.
    pub(super) pending_owners: Vec<(Range, Type, OwnerContext)>,
    pub owner_entries: HashSet<StageKey>,
    pub(super) owner_stages: HashMap<StageKey, OwnerStage>,
    pub(super) owner_stack: Vec<OwnerStage>,
    pending_dicts: Vec<(Range, u32, Dict)>,
    /// The proof declarations, as the checker settled them.
    pub proofs: Vec<super::proof_decls::ProofInfo>,
    /// The laws of the module's classes, typed as proofs are.
    pub laws: Vec<super::proof_decls::ProofInfo>,
    /// Local `let` / `let rec` signatures and annotations keyed by `K::LetDecl`
    /// range, keeping enclosing `Type::Skolem` ids until lowering resolves
    /// scope regions to their unique IR names.
    #[cfg(not(target_arch = "wasm32"))]
    pub local_schemes: HashMap<Range, Type>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) pending_local_schemes: Vec<(Range, Type)>,
    /// Source `scope` expression range for each scope skolem id.
    #[cfg(not(target_arch = "wasm32"))]
    pub scope_skolems: HashMap<u32, Range>,
    /// Source name of each skolem id referenced in `local_schemes`.
    #[cfg(not(target_arch = "wasm32"))]
    pub skolem_names: HashMap<u32, String>,
}

impl Typing {
    pub fn uses_owner(&self, owner: Owner) -> bool {
        self.owners.values().any(|o| *o == owner)
    }
}

#[cfg(feature = "ide")]
/// Lexical owner metadata for hover and refactoring on `defer` / `errdefer`.
#[derive(Debug, Clone)]
pub struct DeferInfo {
    /// Span of the selected explicit `scope` keyword.
    pub owner_span: Span,
    /// Full byte range of the selected explicit `scope` expression.
    pub owner_node: Range,
    /// Optional explicit owner name (`scope name { … }`).
    pub owner_name: Option<String>,
    /// Whether the statement is `errdefer` (`true`) or `defer` (`false`).
    pub is_errdefer: bool,
}

/// A class method selected without inference or dictionary search.
#[derive(Debug, Clone)]
pub struct Coercion {
    pub method: String,
    pub dict: Dict,
}

/// What a name refers to. Locals are known to the lowering by scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ref {
    Local,
    /// A public value field of the lexically opened `Module Sig` local.
    PackedField {
        binder: String,
        field: String,
    },
    /// A top-level `let` or `foreign` of `module`.
    Value {
        module: String,
        name: String,
    },
    /// A class method: a field of the dictionary of the first constraint.
    Method {
        module: String,
        name: String,
    },
    Constructor(Constructor),
    /// `S.op`: the service is looked up at the first runtime stage.
    Operation {
        service: String,
        name: String,
        stages: Vec<Stage>,
    },
    /// A service named as a value: `Layer.succeed Clock …`.
    ServiceTag(String),
}

/// What `===` compares for an `==` (see [`Typing::identity_eq`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Identity {
    Value,
    Tag,
}

/// How a `foreign` is called: its arrows, the dictionaries it takes and
/// what its metadata says.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ForeignInfo {
    /// Scope explicitly declared on each original source arrow.
    #[serde(default)]
    pub scoped_stages: Vec<bool>,
    #[serde(default)]
    pub owner_result: Option<Box<ForeignOwnerPolicy>>,
    pub stages: Vec<bool>,
    pub dicts: usize,
    /// `arity n`, when written.
    pub arity: Option<usize>,
    /// The `catch` mapping expression.
    pub catch: Option<Range>,
    pub adapter: Adapter,
    /// How the host is called: the mode as written, else as the signature tells.
    pub mode: ForeignMode,
    /// Whether parameter `arity - 1` has type `List a` (used by official-backend
    /// fallback adapter checks when a backend runner is not invoked).
    #[serde(default)]
    pub last_param_is_list: Option<bool>,
}

/// The scopes around a call, as their regions and `scope` expressions,
/// innermost last.
pub(super) type OpenScopes = Vec<(u32, Range)>;

/// Constraints that are solved by the region checker, and the equalities a
/// GADT branch assumes, pass no dictionary.
pub fn passes_dictionary(constraint: &Type) -> bool {
    let class = |item| super::classes::args_of(constraint, item).is_some();
    !class(Lang::Outlives)
        && !class(Lang::Provided)
        && !class(Lang::Detached)
        && !class(Lang::NoControl)
        && !class(Lang::TypeEq)
        && super::gadt::row_facts::Equation::from_given(constraint).is_none()
        && as_field_given(constraint).is_none()
}

impl Typing {
    /// Where a dictionary is passed, known once the module is solved and
    /// before `finish_typing` sorts the dictionaries into `dicts`.
    pub(super) fn dictionary_sites(&self) -> HashSet<Range> {
        self.pending_dicts.iter().map(|(at, _, _)| *at).collect()
    }

    pub(crate) fn pending_foreign_diagnostics(&self) -> Vec<Diagnostic> {
        self.pending_foreigns
            .iter()
            .map(super::ffi::PendingForeign::diagnostic)
            .collect()
    }
}
