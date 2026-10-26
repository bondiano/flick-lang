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
mod builtins;
mod calls;
mod classes;
pub mod cli;
mod codecs;
mod component;
mod comprehension;
mod convert;
mod debug;
mod decls;
mod display;
mod do_notation;
mod effects;
mod env;
mod exhaust;
mod exports;
mod ffi;
mod generalize;
mod handlers;
mod infer;
mod instances;
mod irrefutable;
mod kinds;
mod makers;
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
mod rows;
mod schemas;
mod services;
mod sql;
mod stub;
mod sugar_lints;
mod toplevel;
mod ty;
mod typing;
mod ui;
mod unify;

pub use applicative_do::{and_keys, group_key, pure_argument};
pub use bindings::{LetParts, let_parts, signature_name};
pub use builtins::{builtin_sources, builtins};
pub use classes::Dict;
pub(crate) use classes::{Match, match_type};
pub use component::{instance_key as component_instance, parts as component_parts};
pub use comprehension::{comprehension, for_parts};
pub use convert::is_type;
pub use decls::class_defaults;
pub use display::show_type;
pub use do_notation::{bind_key, guard_key};
pub use effects::NO_EFFECTS;
pub use env::{Abi, Form, Fundep, Instance, JsClass, Operation, Rep, TypeDef};
pub use kinds::Kind;
pub use patterns::is_pattern;
pub use question::Question;
pub use ranges::range_list;
pub use schemas::{companion_name, witness_entity};
pub(crate) use sql::base_type;
pub use sql::dialect::{self, Dialect};
pub use ty::{Binder, Effects, PRELUDE, Row, SYNC_SERVICES, Stage, Type, stage_of};
pub use typing::{
    CodecBlock, Constructor, ForeignInfo, Identity, InstanceInfo, Lift, Owner, Range, Ref,
    SchemaItem, SchemaPlan, SqlColumn, SqlForm, SqlPlan, SqlTable, SqlValue, Typing,
    default_method, passes_dictionary,
};

use convert::TyVar;
use env::Env;
use kinds::KindStore;
use std::collections::HashMap;
use unify::Store;

use crate::diagnostics::{Code, Diagnostic, Severity, codes};
use crate::iface::Interface;
use crate::resolve::Module;
use crate::source::{FileId, Span};
use crate::syntax::cst::{Node, Token};
use crate::syntax::kind::SyntaxKind as K;

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
    /// The SQL dialect of `sql table` and `sql { … }`.
    pub sql: &'static Dialect,
    /// `[lint] no-let-assert`: every `let assert` is an error.
    pub no_let_assert: bool,
    /// Whether runtime code may run: `false` under `effect = "none"`.
    pub effects: bool,
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

fn run(
    module: &Module,
    imported: &dyn Fn(&str) -> Option<Interface>,
    builtins: &[Interface],
    options: Options,
    interface: &mut Interface,
) -> (Vec<Diagnostic>, Typing, Vec<(Span, Dict)>) {
    let env = Env::new(module.name, module.syntax, imported, builtins);
    let mut checker = Checker {
        file: module.file,
        text: module.text,
        env,
        store: Store::default(),
        kinds: KindStore::default(),
        diagnostics: Vec::new(),
        locals: Vec::new(),
        ui: 0,
        tyvars: Vec::new(),
        runtime: false,
        stage: effects::Stage::default(),
        sites: Vec::new(),
        subrows: Vec::new(),
        operators: Vec::new(),
        equalities: Vec::new(),
        recs: Vec::new(),
        top: toplevel::TopLevel::default(),
        options,
        classes: classes::State::default(),
        regions: regions::State::default(),
        typing: Typing::default(),
        predicates: HashMap::new(),
        field_path: Vec::new(),
        holes: Vec::new(),
        hole_ok: false,
        sugar: Vec::new(),
    };
    checker.typing.sql_dialect = options.sql;
    checker.module(module.tree);
    checker.report_holes();
    checker.fill(interface);
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
    let guessed = |d: &Diagnostic| {
        d.code == codes::T013
            || unfinished(&d.message)
            || d.fixes.iter().any(|f| unfinished(&f.message))
    };
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
    ty: Type,
    /// A function of a recursive group used before its type is proved, by
    /// index into [`Checker::recs`].
    rec: Option<usize>,
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
    /// How many `ui { … }` blocks enclose the expression: inside one, the
    /// names `Ui` exports need no `Ui.`.
    ui: usize,
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
    /// `==` and `!=` with the type of their operands.
    equalities: Vec<(Type, Span)>,
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
    /// The fields of the record literals around the value being checked,
    /// outermost first: where a nested record's field is missing.
    field_path: Vec<String>,
    /// Each `_` written in a type, and the variable that stands for it.
    holes: Vec<(Span, Type)>,
    /// Whether the type being converted may have holes.
    hole_ok: bool,
}

impl Checker<'_> {
    fn span(&self, node: &Node) -> Span {
        let (start, end) = node.range();
        Span::new(self.file, start, end)
    }

    fn token_span(&self, token: Token) -> Span {
        Span::new(self.file, token.start, token.end)
    }

    /// The span of a node's own (possibly qualified) name tokens, without
    /// the arguments a pattern or type application carries as child nodes.
    fn name_span(&self, node: &Node) -> Span {
        let mut names = node
            .child_tokens()
            .filter(|t| matches!(t.kind, K::Con | K::Ident | K::Dot));
        let Some(first) = names.next() else {
            return self.span(node);
        };
        let end = names.last().map_or(first.end, |t| t.end);
        Span::new(self.file, first.start, end)
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

    /// An error that offers `guess` as the replacement of `span`'s text.
    fn error_guess(
        &mut self,
        code: Code,
        span: Span,
        message: impl Into<String>,
        guess: Option<&str>,
    ) {
        let guess = guess.map(str::to_owned);
        self.diagnostics
            .push(Diagnostic::error(code, message, span).did_you_mean(span, guess));
    }

    fn warning(&mut self, code: Code, span: Span, message: impl Into<String>) {
        self.diagnostics
            .push(Diagnostic::new(code, Severity::Warning, message).with_span(span));
    }

    fn duplicate_label(&mut self, label: &str, span: Span) {
        self.error(
            codes::T014,
            span,
            format!("field `{label}` is given more than once; labels of a record are unique"),
        );
    }
}
