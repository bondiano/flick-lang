//! Kinds and the type core (§4, §5 without classes, §9 "Числовая модель"):
//! kind checking, bidirectional type checking against the full signature of
//! every named function, higher-rank `forall` with predicative instantiation,
//! exact records with row polymorphism, exhaustive `match`, and generalization
//! of provably pure bindings only. Rules: `docs/reference/types.md`.
//!
//! The checker reads the concrete tree after name resolution; other modules
//! are known only through their [`Interface`], which this phase fills with
//! the types and kinds of the exports.

mod bindings;
mod builtins;
mod calls;
mod convert;
mod decls;
mod display;
mod env;
mod exhaust;
mod exports;
mod generalize;
mod infer;
mod instances;
mod kinds;
mod patterns;
mod report;
mod rows;
mod toplevel;
mod ty;
mod unify;

pub use builtins::builtins;
pub use env::{Form, Operation, TypeDef};
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

/// Type-checks `module` and stores the types of its exports in `interface`.
/// `imported` gives the interfaces of its imports, `builtins` those of the
/// built-in modules (see [`builtins`]).
pub fn check(
    module: &Module,
    imported: &dyn Fn(&str) -> Option<Interface>,
    builtins: &[Interface],
    interface: &mut Interface,
) -> Vec<Diagnostic> {
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
        operators: Vec::new(),
        recs: Vec::new(),
        top: toplevel::TopLevel::default(),
    };
    checker.module(module.tree);
    checker.fill(interface);
    checker.diagnostics
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
/// recursive uses must have exactly the type being checked (§3).
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
    operators: Vec<Operator>,
    recs: Vec<Rec>,
    top: toplevel::TopLevel<'a>,
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
