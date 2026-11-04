//! What code generation needs to know from type checking, recorded by the
//! source range of the node it is about: what a name refers to, which
//! application stages run through the runtime, where a pure function is lifted
//! into a runtime position, and which dictionaries a use of a constrained
//! value receives.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::mem;
use std::sync::Arc;

use super::Checker;
use super::classes::Dict;
use super::env::{Abi, Rep};
#[cfg(feature = "ide")]
use super::sql::dialect::Dialect;
pub use super::sql_plan::{SqlColumn, SqlForm, SqlPlan, SqlTable, SqlValue};
use super::ty::{Stage, Type};
use super::{FormField, UrlRoute};
use crate::iface::Interface;
use crate::ir::Label;
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
    /// The numeric type of arithmetic and integer literals.
    pub numeric: HashMap<Range, Num>,
    /// Operators on a type variable, by `Numeric` or by `Ord`.
    pub generic_numeric: HashSet<Range>,
    pub generic_ordered: HashSet<Range>,
    /// `++` on lists rather than strings.
    pub list_concat: HashSet<Range>,
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
    pub labelled: HashMap<String, Label>,
    /// The top-level functions that make an atom, an action or a view, each
    /// with the maker of the library it ends at.
    pub makers: HashMap<String, String>,
    /// The top-level values that reach `Server`, each with its path to it.
    pub servers: HashMap<String, Vec<String>>,
    /// Each `foreign`: its calling convention.
    pub foreigns: HashMap<Range, ForeignInfo>,
    /// Instances declared or derived in this module.
    pub instances: Vec<InstanceInfo>,
    /// Witnesses of the types that derive `Schema`, and of what each
    /// server function sends.
    pub schemas: Vec<SchemaItem>,
    /// The server functions whose witnesses are all known.
    pub server_fns: Vec<ServerFn>,
    /// The records that derive `HttpApi`.
    pub http_apis: Vec<super::http_api::HttpApiItem>,
    /// `codec` blocks: the fields' labels in the order written.
    pub codecs: HashMap<Range, CodecBlock>,
    #[cfg(feature = "ide")]
    /// The type of each name in an expression or constructor pattern and
    /// of each variable a pattern binds, for editors; at a keyword that opens
    /// an expression (`task`, `match`, `fun` …) the type of the whole expression, at
    /// `ignore` and `await` the type of the operand, at `|>` the type after
    /// the stage.
    pub types: HashMap<Range, Type>,
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
    /// `sql table` declarations.
    pub sql_tables: Vec<SqlTable>,
    #[cfg(feature = "ide")]
    /// The dialect the SQL of the module was checked against.
    pub sql_dialect: &'static Dialect,
    #[cfg(feature = "ide")]
    /// The transparent aliases the module declares, exported or not: their
    /// parameters and bodies.
    pub aliases: HashMap<String, (Vec<String>, Type)>,
    /// `sql { … }` blocks: their SQL and how values cross the driver.
    pub sql: HashMap<Range, SqlPlan>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Constructor {
    pub module: String,
    pub ty: String,
    pub name: String,
    pub arity: usize,
    /// How its values are represented in JS.
    pub rep: Rep,
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
    pub this: bool,
    /// The `catch` mapping expression.
    pub catch: Option<Range>,
    pub cancel: bool,
    pub optional: Vec<usize>,
    pub variadic: bool,
    pub result: ResultAdapter,
    /// For each JS argument: the number of arguments of the JS function a
    /// callback is adapted to.
    pub callbacks: Vec<Option<usize>>,
    pub copies: Copies,
}

/// Where a foreign call may pass Flick data, which an import outside the
/// runtime gets as a copy: JS may change an array or object in place
/// (`docs/formal/ts-holes.md#31-flick-data-is-copied-at-the-foreign-boundary`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Copies {
    /// For each JS argument.
    pub args: Vec<bool>,
    /// The result; of a `Promise` or an `Effect`, its value.
    pub result: bool,
    /// For each JS argument that is a callback: each argument JS gives it,
    /// and the result it gives JS.
    pub callbacks: Vec<Option<(Vec<bool>, bool)>>,
}

impl Copies {
    /// Whether any value is copied.
    pub fn any(&self) -> bool {
        self.result
            || self.args.contains(&true)
            || self
                .callbacks
                .iter()
                .flatten()
                .any(|(args, result)| *result || args.contains(&true))
    }
}

/// A `server fn`: its declaration, its name and how many errors it sends.
#[derive(Debug, Clone)]
pub struct ServerFn {
    pub decl: Range,
    pub name: String,
    /// How many parameters it takes; several are sent as one tuple.
    pub params: usize,
    pub errors: usize,
    /// Whether it lists `Server.Unauthorized`, so `Server.authenticated` serves it.
    pub guarded: bool,
}

/// The witness `T$schema` of a type that derives `Schema`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaItem {
    pub ty: String,
    /// The type declaration.
    pub decl: Range,
    /// How many witnesses of its parameters it takes.
    pub witnesses: usize,
    pub plan: SchemaPlan,
    /// The routes of `T$url` when the type derives `Url`.
    pub url: Option<Vec<UrlRoute>>,
    /// The fields of `T$form` when the type derives `Form`.
    pub form: Option<Vec<FormField>>,
}

/// How a witness is built from the witnesses of its parts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SchemaPlan {
    /// A primitive: `Int`, `Float`, `String`, `Bool`, `Unit`, ….
    Prim(String),
    List(Box<SchemaPlan>),
    Option(Box<SchemaPlan>),
    Result(Box<SchemaPlan>, Box<SchemaPlan>),
    Redacted(Box<SchemaPlan>),
    Tuple(Vec<SchemaPlan>),
    /// Fields in label order.
    Record(Vec<(String, SchemaPlan)>),
    /// The constructors of a variant type or an error, with their fields.
    Adt {
        name: String,
        variants: Vec<(String, Vec<SchemaPlan>)>,
    },
    /// The witness of the `i`-th parameter.
    Param(usize),
    /// The polymorphic witness of the `i`-th parameter applied to a schema.
    Applied(usize, Box<SchemaPlan>),
    /// A built-in witness over type constructors: `List`, `Option`, `Redacted`.
    Constructor(String),
    /// The witness of a type with one, given the witnesses of its arguments;
    /// `lazy` within the module's group, which may be recursive.
    Named {
        module: String,
        ty: String,
        args: Vec<SchemaPlan>,
        lazy: bool,
    },
    /// A refined type: its base and the check exporters know, if any.
    Refined {
        name: String,
        base: Box<SchemaPlan>,
        known: Option<String>,
    },
}

/// A `codec` block: the name of what it decodes, for issue paths, and its
/// fields as written.
#[derive(Debug, Clone, Default)]
pub struct CodecBlock {
    pub name: String,
}

/// How the JS result of a foreign call becomes a Flick value.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct ResultAdapter {
    /// `nullable`: `null` and `undefined` are `None`.
    pub nullable: bool,
    /// The result is `Unit`: whatever JS returns becomes `()`.
    pub unit: bool,
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

/// How a function becomes the one of another stage its position expects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lift {
    /// A function: the stage of this stage and the one expected, and how its
    /// result lifts.
    Stage {
        from: Stage,
        to: Stage,
        result: Option<Box<Lift>>,
    },
    /// The result of an `Eff`.
    Eff(Box<Lift>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceInfo {
    pub module: String,
    /// The head as written: what `Dict::Instance` names it by.
    pub name: String,
    pub class: (String, String),
    /// The `instance` declaration, or the `deriving` clause.
    pub span: Range,
    /// The context, as its dictionaries are named.
    pub context: Vec<String>,
    pub supers: Vec<(String, Dict)>,
    /// For a derived instance: the dictionary of each field, constructor by
    /// constructor, and the constructors (`None` for a type without variants,
    /// whose one field is its representation).
    pub derived: Option<(Vec<Dict>, Option<Vec<Constructor>>)>,
    /// Methods taken from the class's defaults, with how many dictionaries
    /// of their own each takes.
    pub defaults: Vec<(String, usize)>,
}

/// The top-level value a class's default definition of `method` compiles
/// to, in the class's module; it takes the class's dictionary first.
pub fn default_method(class: &str, method: &str) -> String {
    format!("$default_{class}_{method}")
}

/// Constraints that are solved by the region checker and pass no dictionary.
pub fn passes_dictionary(constraint: &Type) -> bool {
    let class = |item| super::classes::args_of(constraint, item).is_some();
    !class(Lang::Outlives) && !class(Lang::Provided)
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
            .map(|g| self.show(&self.canon(g)))
            .collect();
        if !shown.is_empty() {
            self.typing.givens.insert(range(span), shown);
        }
    }

    /// Records the type of a name or binder at `span`, settled once the
    /// module is solved.
    pub fn note_type(&mut self, span: Span, ty: &Type) {
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
            K::SqlExpr => K::SqlKw,
            K::CodecExpr => K::CodecKw,
            K::SchemaExpr => K::SchemaKw,
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
