//! Kinds, the type core and classes:
//! kind checking, bidirectional type checking against the full signature of
//! every named function, higher-rank `forall` with predicative instantiation,
//! exact records with row polymorphism, exhaustive `match`, and generalization
//! of provably pure bindings only; effects, expected errors, services and
//! layers; classes, instances, row primitives and `deriving`; regions and
//! the captures of closures. Rules:
//! `docs/reference/types.md`, `docs/reference/effects.md`,
//! `docs/reference/classes.md`, `docs/reference/regions.md`.
//!
//! The checker reads the concrete tree after name resolution; other modules
//! are known only through their [`Interface`], which this phase fills with
//! the types and kinds of the exports.

mod bindings;
mod builtins;
mod calls;
mod classes;
mod convert;
mod decls;
mod display;
mod effects;
mod env;
mod exhaust;
mod exports;
mod generalize;
mod handlers;
mod infer;
mod instances;
mod kinds;
mod patterns;
mod regions;
mod report;
mod rows;
mod services;
mod toplevel;
mod ty;
mod unify;

pub use builtins::builtins;
pub use classes::Dict;
pub use env::{Form, Fundep, Instance, Operation, TypeDef};
pub use kinds::Kind;
pub use ty::{Binder, Effects, PRELUDE, Row, Type};

use convert::TyVar;
use env::Env;
use kinds::KindStore;
use unify::Store;

use crate::diagnostics::{Code, Diagnostic, Severity};
use crate::iface::Interface;
use crate::resolve::Module;
use crate::source::{FileId, Span};
use crate::syntax::cst::{Node, Token};

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

/// What checking a module needs beyond its imports.
#[derive(Clone, Copy)]
pub struct Options<'a> {
    /// Whether a module belongs to the package being checked: only the
    /// package of a class or of the type an instance is for may declare it.
    pub local: &'a dyn Fn(&str) -> bool,
    pub limits: Limits,
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
    check_with_evidence(module, imported, builtins, options, interface).0
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
    let env = Env::new(module.name, module.syntax, imported, builtins);
    let mut checker = Checker {
        file: module.file,
        text: module.text,
        env,
        store: Store::default(),
        kinds: KindStore::default(),
        diagnostics: Vec::new(),
        locals: Vec::new(),
        tyvars: Vec::new(),
        runtime: false,
        stage: effects::Stage::default(),
        sites: Vec::new(),
        subrows: Vec::new(),
        operators: Vec::new(),
        recs: Vec::new(),
        top: toplevel::TopLevel::default(),
        options,
        classes: classes::State::default(),
        regions: regions::State::default(),
    };
    checker.module(module.tree);
    checker.fill(interface);
    let mut evidence = checker.classes.evidence;
    evidence.sort_by_key(|(span, _)| (span.start, span.end));
    (checker.diagnostics, evidence)
}

/// A value in scope inside a declaration.
#[derive(Debug, Clone)]
struct Local {
    name: String,
    ty: Type,
    /// A function of a recursive group used before its type is proved, by
    /// index into [`Checker::recs`].
    rec: Option<usize>,
}

/// A recursive function whose signature has no explicit `forall`: its
/// recursive uses must have exactly the type being checked.
#[derive(Debug, Clone)]
struct Rec {
    name: String,
    /// The signature with the skolems its body is checked against.
    target: Option<Type>,
    uses: Vec<(Type, Span)>,
}

/// An operator whose operand type is checked once inference settles it.
#[derive(Debug, Clone)]
struct Operator {
    ty: Type,
    span: Span,
    op: &'static str,
    /// Types the operator accepts, by built-in name; `List` means any list.
    allowed: &'static [&'static str],
}

struct Checker<'a> {
    file: FileId,
    text: &'a str,
    env: Env,
    store: Store,
    kinds: KindStore,
    diagnostics: Vec<Diagnostic>,
    locals: Vec<Local>,
    tyvars: Vec<TyVar>,
    /// Whether the expression being inferred runs a runtime call at this
    /// stage; lambdas and `task` bodies are deferred and do not count.
    runtime: bool,
    /// The stage being checked: what its runtime calls may need.
    stage: effects::Stage,
    sites: Vec<effects::Site>,
    /// Row constraints of the declaration being checked.
    subrows: Vec<effects::SubRow>,
    operators: Vec<Operator>,
    recs: Vec<Rec>,
    top: toplevel::TopLevel<'a>,
    options: Options<'a>,
    classes: classes::State,
    regions: regions::State,
}

impl Checker<'_> {
    fn span(&self, node: &Node) -> Span {
        let (start, end) = node.range();
        Span::new(self.file, start, end)
    }

    fn token_span(&self, token: Token) -> Span {
        Span::new(self.file, token.start, token.end)
    }

    /// Source text of a node with its line breaks and runs of spaces collapsed.
    fn text_of(&self, node: &Node) -> String {
        let (start, end) = node.range();
        self.text[start as usize..end as usize]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn error(&mut self, code: Code, span: Span, message: impl Into<String>) {
        self.diagnostics
            .push(Diagnostic::error(code, message, span));
    }

    fn warning(&mut self, code: Code, span: Span, message: impl Into<String>) {
        self.diagnostics
            .push(Diagnostic::new(code, Severity::Warning, message).with_span(span));
    }

    fn duplicate_label(&mut self, label: &str, span: Span) {
        self.error(
            crate::diagnostics::codes::T014,
            span,
            format!("field `{label}` is given more than once; labels of a record are unique"),
        );
    }
}
