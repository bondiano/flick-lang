//! A `foreign` declaration: a function of the host a program runs on, by
//! the name the host knows it by, called by a convention each backend reads
//! its own way.

use serde::{Deserialize, Serialize};

use crate::Expr;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Foreign {
    pub name: String,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ForeignMode {
    /// A function of its arguments alone.
    Pure,
    /// Runs at once, in the runtime; a failure is a defect.
    Sync,
    /// Gives its value later; a failure is a defect.
    Async,
    /// Gives an effect of the host's runtime, which runs it.
    Effect,
}

/// What a `foreign` declaration says about how values cross: the arguments
/// as the function takes them, and its result as the program gets it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Adapter {
    /// The first argument is the receiver of the call, `this`.
    pub receiver: bool,
    /// The function also takes what cancels it, as its last argument.
    pub cancel: bool,
    /// The arguments the function may go without: an absent `Option` is
    /// left out.
    pub optional: Vec<usize>,
    /// The last argument, a list, is spread into the arguments.
    pub variadic: bool,
    pub result: ResultAdapter,
    /// For each argument: the number of arguments of the function of the
    /// host a callback is adapted to.
    pub callbacks: Vec<Option<usize>>,
    pub copies: Copies,
}

/// Where a call may pass Flick data, which a function outside the runtime
/// gets as a copy, as the host may change a list or a record in place
/// (`docs/formal/ts-holes.md#31-flick-data-is-copied-at-the-foreign-boundary`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct ResultAdapter {
    /// `nullable`: the host's absent values are `None`.
    pub nullable: bool,
    /// The result is `Unit`: whatever the function gives becomes `()`.
    pub unit: bool,
}
