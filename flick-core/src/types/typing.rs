//! What code generation needs to know from type checking, recorded by the
//! source range of the node it is about: what a name refers to, which
//! application stages run through the runtime, where a pure function is lifted
//! into a runtime position, and which dictionaries a use of a constrained
//! value receives.

pub use flick_ir::{Constructor, GenericInfo, InstanceInfo, Lift};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::mem;
use std::sync::Arc;

use super::Checker;
use super::classes::Dict;
use super::env::Abi;
use super::gadt::branch::as_field_given;
use super::services::OpClass;
use super::ty::{Stage, Type};
use crate::iface::Interface;
use crate::ir::{Adapter, ForeignMode, Label};
use crate::numeric::Num;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::Lang;

mod stages;

/// A source range of the module being checked.
pub type Range = (u32, u32);

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
    /// Applications inside `ui` whose function is an opaque `Ui.Component`.
    pub ui_components: HashSet<Range>,
    /// The `and` groups of `do` followed only by `pure e`, whose last
    /// `map2` computes `e`.
    pub applicative: HashSet<Range>,
    /// What each postfix `?` does.
    pub questions: HashMap<Range, super::Question>,
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
    /// The top-level values that reach `Server`, each with its path to it.
    pub servers: HashMap<String, Vec<String>>,
    /// Each `foreign`: its calling convention.
    pub foreigns: HashMap<Range, ForeignInfo>,
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
    /// The transparent aliases the module declares, exported or not: their
    /// parameters and bodies.
    pub aliases: HashMap<String, (Vec<String>, Type)>,
    /// Runtime calls inside a `scope` that acquire into the owner of another
    /// region than the innermost scope's.
    pub owners: HashMap<Range, Owner>,
    /// Pending: uses whose types are known once the module is solved.
    pending_labels: Vec<(Range, Type)>,
    /// A call needing `Scope r`, with the scopes open around it as regions
    /// and `scope` expressions, innermost last.
    pub(super) pending_owners: Vec<(Range, Type, OpenScopes)>,
    pending_dicts: Vec<(Range, u32, Dict)>,
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
    pub stages: Vec<bool>,
    pub dicts: usize,
    /// `arity n`, when written.
    pub arity: Option<usize>,
    /// The `catch` mapping expression.
    pub catch: Option<Range>,
    pub adapter: Adapter,
    /// How the host is called: the mode as written, else as the signature tells.
    pub mode: ForeignMode,
}

/// The scopes around a call, as their regions and `scope` expressions,
/// innermost last.
pub(super) type OpenScopes = Vec<(u32, Range)>;

/// The owner a call acquires into: that of the `scope` expression `scope`,
/// or with `outside`, the one around it (a region of the signature).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Owner {
    pub scope: Range,
    pub outside: bool,
}

/// The top-level value a class's default definition of `method` compiles
/// to, in the class's module; it takes the class's dictionary first.
pub fn default_method(class: &str, method: &str) -> String {
    format!("$default_{class}_{method}")
}

/// Constraints that are solved by the region checker, and the equalities a
/// GADT branch assumes, pass no dictionary.
pub fn passes_dictionary(constraint: &Type) -> bool {
    let class = |item| super::classes::args_of(constraint, item).is_some();
    !class(Lang::Outlives)
        && !class(Lang::Provided)
        && !class(Lang::Detached)
        && !class(Lang::TypeEq)
        && as_field_given(constraint).is_none()
}

impl Checker<'_> {
    pub fn record_name(&mut self, span: Span, r: Ref) {
        self.typing.names.insert(range(span), r);
    }

    /// Records the dictionaries of the constraints of a definition or
    /// argument checked at `span`.
    pub fn record_givens(&mut self, span: Span, givens: &[Type]) {
        let shown: Vec<String> = givens
            .iter()
            .filter(|g| passes_dictionary(g))
            .map(|g| self.show_exact(&self.canon(g)))
            .collect();
        if !shown.is_empty() {
            self.typing.givens.insert(range(span), shown);
        }
    }

    /// Records the type of a name or binder at `span`, settled once the
    /// module is solved.
    pub fn note_type(&mut self, span: Span, ty: &Type) {
        if !self.store.equalities.is_empty() {
            return self.note_refined(range(span), ty, ty);
        }
        self.typing.pending_types.push((range(span), ty.clone()));
    }

    #[cfg(feature = "ide")]
    /// `record` is the record the field label at `span` belongs to.
    pub fn note_field(&mut self, span: Span, record: &Type) {
        self.typing
            .pending_fields
            .push((range(span), record.clone()));
    }

    /// The type of an expression a keyword opens (`task`, `match`, `fun` …),
    /// recorded at the keyword so an editor can show it there.
    pub fn note_head(&mut self, node: &Node, ty: &Type) {
        if let t @ (Type::Con { .. } | Type::Meta(_)) = self.store.head(ty) {
            self.typing.pending_values.push((node.range(), t.clone()));
        }
        let head = match node.kind {
            K::TaskExpr => K::TaskKw,
            K::DoExpr => K::DoKw,
            K::ForExpr => K::ForKw,
            K::ScopeExpr => K::ScopeKw,
            K::ProvideExpr => K::ProvideKw,
            K::TryExpr => K::TryKw,
            K::MatchExpr => K::MatchKw,
            K::IfExpr => K::IfKw,
            K::FunExpr => K::FunKw,
            K::UiExpr => K::UiKw,
            _ => return,
        };
        if let Some(kw) = node.child_tokens().find(|t| t.kind == head) {
            self.note_type(self.token_span(kw), ty);
        }
    }

    pub fn record_dict(&mut self, span: Span, order: u32, dict: Dict) {
        self.typing.pending_dicts.push((range(span), order, dict));
    }

    pub fn record_label(&mut self, span: Span, ty: Type) {
        self.typing.pending_labels.push((range(span), ty));
    }

    #[cfg(feature = "ide")]
    /// `t` with each rigid variable written as the variable it stands for.
    fn named_skolems(&self, t: &Type) -> Type {
        match t {
            Type::Skolem(s) => Type::Var(self.store.skolem_name(*s).to_owned()),
            other => other.map_children(&mut |c| self.named_skolems(c)),
        }
    }

    /// Settles what waited for the whole module to be solved.
    pub fn finish_typing(&mut self) -> Typing {
        let mut typing = mem::take(&mut self.typing);
        let pending = mem::take(&mut typing.pending_values);
        for (at, ty) in pending.iter().chain(&typing.pending_types) {
            if let t @ Type::Con { .. } = self.store.head(ty) {
                typing.values.entry(*at).or_insert_with(|| t.clone());
            }
        }
        #[cfg(feature = "ide")]
        {
            typing.aliases = self
                .env
                .local_aliases()
                .map(|(name, params, body)| (name.to_owned(), (params.to_vec(), body.clone())))
                .collect();
            for (at, ty) in mem::take(&mut typing.pending_types) {
                let ty = self.named_skolems(&self.zonk(&ty));
                typing.types.insert(at, ty);
            }
            for (at, ty) in mem::take(&mut typing.pending_fields) {
                let ty = self.named_skolems(&self.zonk(&ty));
                typing.fields.insert(at, ty);
            }
            for (at, ty) in mem::take(&mut typing.pending_refined) {
                let ty = self.named_skolems(&self.zonk(&ty));
                typing.refined.insert(at, ty);
            }
        }
        #[cfg(not(feature = "ide"))]
        typing.pending_types.clear();
        for (at, ty) in mem::take(&mut typing.pending_labels) {
            let ty = self.zonk(&ty);
            if let Type::App { arg, .. } = ty
                && let Type::Symbol(label) = *arg
            {
                typing.labels.insert(at, label);
            }
        }
        for (at, needed, open) in mem::take(&mut typing.pending_owners) {
            let region = match self.zonk(&needed).spine().1[..] {
                [Type::Skolem(id)] => Some(*id),
                _ => None,
            };
            let owner = match open.iter().position(|(r, _)| Some(*r) == region) {
                Some(i) if i + 1 == open.len() => None,
                Some(i) => Some(Owner {
                    scope: open[i].1,
                    outside: false,
                }),
                None => open.first().map(|(_, scope)| Owner {
                    scope: *scope,
                    outside: true,
                }),
            };
            if let Some(owner) = owner {
                typing.owners.insert(at, owner);
            }
        }
        let mut dicts = mem::take(&mut typing.pending_dicts);
        dicts.sort_by_key(|(at, order, _)| (*at, *order));
        for (at, _, dict) in dicts {
            typing.dicts.entry(at).or_default().push(dict);
        }
        typing
    }
}
