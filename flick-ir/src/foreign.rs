//! A `foreign` declaration: a function of the host a program runs on, by
//! the name the host knows it by, called by a convention each backend reads
//! its own way.

use serde::{Deserialize, Serialize};

use crate::{Expr, Span, TypeRef};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Foreign {
    pub name: String,
    /// Original source identity when a compiler-generated shim owns `name`.
    /// Host binding lookup and source schemes must not use a generated raw name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_name: Option<String>,
    pub mode: ForeignMode,
    /// What the host knows the function by.
    pub binding: Binding,
    /// Each arrow of the signature: whether it is runtime.
    pub stages: Vec<bool>,
    /// Dictionaries the function takes before its arguments.
    pub dicts: usize,
    /// The arguments the function takes: `arity n`, else the stages up to
    /// the first runtime one.
    pub arity: usize,
    /// How values cross into the function and out of it.
    pub adapter: Adapter,
    /// The `catch` mapping, a function of the thrown value.
    pub catch: Option<Expr>,
    /// Quantified layout-relevant type parameters of this foreign declaration.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub type_params: Vec<String>,
    /// The full layout-relevant type in terms of `type_params`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ty: Option<TypeRef>,
}

impl Foreign {
    /// The Flick parameters a call takes: `arity 0` still takes `()`.
    pub fn taken(&self) -> usize {
        if self.arity == 0 {
            self.stages.len().min(1)
        } else {
            self.arity
        }
    }
}

/// The name of a function of the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    /// The module of the host it comes from; none for one the host has
    /// everywhere, a `global`.
    pub from: Option<String>,
    /// Its name there, or the path of a global (`Date.now`).
    pub name: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ForeignMode {
    /// A function of its arguments alone.
    #[default]
    Pure,
    /// Runs at once, in the runtime; a failure is a defect.
    Sync,
    /// Gives its value later; a failure is a defect.
    Async,
    /// Gives an effect of the host's runtime, which runs it.
    Effect,
}

/// One positional argument passed to a backend-declared foreign adapter word.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdapterArg {
    Word(String),
    Int(u64),
    String(String),
}

/// One backend-declared adapter word attached to a `foreign` binding, with
/// its parsed arguments and source span for backend-owned diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendAdapter {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<AdapterArg>,
    pub span: Span,
}

/// What a `foreign` declaration says about how values cross: the core
/// contract fields plus any backend-declared adapter words.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Adapter {
    /// The arguments the function may go without: an absent `Option` is
    /// left out.
    pub optional: Vec<usize>,
    /// The arguments whose ownership transfers to the foreign call (`owned`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub owned: Vec<usize>,
    pub result: ResultAdapter,
    /// For each argument: the number of arguments of the function of the
    /// host a callback is adapted to.
    pub callbacks: Vec<Option<usize>>,
    pub copies: Copies,
    /// What the Effect bridge converts, for each parameter and the result.
    #[serde(default)]
    pub shapes: Vec<Shape>,
    #[serde(default)]
    pub result_shape: Shape,
    /// Backend-declared adapter words (for example `this`, `cancel signal`,
    /// `variadic`, or third-party backend words) in declaration order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<BackendAdapter>,
}

impl Adapter {
    /// Whether the binding carries a backend-declared adapter word named `name`.
    pub fn has_word(&self, name: &str) -> bool {
        self.words.iter().any(|word| word.name == name)
    }

    /// The first backend-declared adapter word named `name`, if any.
    pub fn word(&self, name: &str) -> Option<&BackendAdapter> {
        self.words.iter().find(|word| word.name == name)
    }

    /// Every backend-declared adapter word named `name`, in declaration order.
    pub fn words_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a BackendAdapter> {
        self.words.iter().filter(move |word| word.name == name)
    }

    /// Whether the binding carries `name` with a single `AdapterArg::Word(arg)`.
    pub fn has_word_arg(&self, name: &str, arg: &str) -> bool {
        self.words.iter().any(|word| {
            word.name == name
                && matches!(word.args.as_slice(), [AdapterArg::Word(actual)] if actual == arg)
        })
    }
}

/// The part of a type that must change when a value crosses the Effect bridge.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub enum Shape {
    #[default]
    Data,
    /// Pure arrows up to and including the first runtime one.
    Fun {
        args: Vec<Shape>,
        result: Box<Shape>,
        runtime: bool,
        sync: bool,
    },
    Eff(Box<Shape>),
    List(Box<Shape>),
    Option(Box<Shape>),
    Result(Box<Shape>, Box<Shape>),
    Tuple(Vec<Shape>),
    Record(Vec<(String, Shape)>),
    Layer,
    Schema,
    Codec,
    Fiber,
    Duration,
    Schedule,
    Chunk,
    HashMap,
    HashSet,
    Handlers,
    Exit,
    Failure,
}

impl Shape {
    pub fn is_data(&self) -> bool {
        matches!(self, Shape::Data)
    }
}

/// Where a call may pass Flick data, which a function outside the runtime
/// gets as a copy, as the host may change a list or a record in place
/// (`docs/formal/ts-holes.md#31-flick-data-is-copied-at-the-foreign-boundary`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Copies {
    /// For each argument.
    pub args: Vec<bool>,
    /// The result; of an asynchronous one or an effect, its value.
    pub result: bool,
    /// For each argument that is a callback: each argument the host gives
    /// it, and the result it gives the host.
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

/// How the result of a call becomes a Flick value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ResultAdapter {
    /// `nullable`: the host's absent values are `None`.
    pub nullable: bool,
    /// The result is `Unit`: whatever the function gives becomes `()`.
    pub unit: bool,
}
