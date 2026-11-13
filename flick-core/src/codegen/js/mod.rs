//! ESM for one module. Expressions become JS expressions; what needs
//! statements (`match`, blocks, branches with statements) is emitted into the
//! current statement list, earlier operands spilled into constants first so
//! that evaluation stays left to right.
//!
//! The code is a document of `flick-pretty` (see [`doc`]), printed once per
//! module: lists that do not fit on their line break one item per line, and
//! source cursors in the document give the source map its exact positions.

/// The cursor mapping the code that follows it to `span`; nothing without
/// source maps.
macro_rules! mark {
    ($gen:expr, $span:expr) => {{
        #[cfg(feature = "native-project")]
        {
            Some(($gen).mark($span))
        }
        #[cfg(not(feature = "native-project"))]
        {
            let _ = &$span;
            None
        }
    }};
}

mod binary;
mod bridge;
mod call;
mod control;
mod decl;
mod doc;
mod exports;
mod expr;
mod foreign;
mod function;
mod generator;
mod group;
mod handle;
mod imports;
mod items;
mod kernel;
mod labels;
mod lift;
mod literal;
mod paths;
mod pattern;
mod release;
mod services;
pub use paths::relative;
mod view;

pub use release::{direct_methods, exported_worker};

use crate::effects::bindings;
use flick_ir::Namespace;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::mem;

#[cfg(feature = "native-project")]
use flick_pretty::Position;
use flick_pretty::{Document, DocumentArena, docvec};

use super::exports::reexports;
use super::tail::{Callee, Target};
use super::{Options, Output, names};
use crate::ir::{Item, ItemKind, Label, Module};
#[cfg(feature = "native-project")]
use crate::source::Span;
use doc::{Js, Stmt, WIDTH, lines};
use exports::sync_exports;

const RUNTIME: &str = bindings::ADAPTER_PREFIX;

/// Compiles `module` to ESM (or TypeScript, see [`Options::typescript`]).
pub fn js(module: &Module, options: &Options) -> Output {
    js_at(module, options, &module.name, &|_, _| None)
}

pub(crate) fn js_at(
    module: &Module,
    options: &Options,
    physical: &str,
    location: &dyn Fn(&str, &str) -> Option<String>,
) -> Output {
    let arena = DocumentArena::new();
    let a = &arena;
    let mut g = Gen::new(module, options, a);
    g.physical = physical;
    g.location = location;
    if !options.rt {
        g.sync_exports = sync_exports(&module.exports);
        // the wrapper of a Sync export is the runtime's
        g.runtime |= !g.sync_exports.is_empty();
    }
    let mut typed = options
        .typescript
        .and_then(|modules| modules.get(physical))
        .cloned();
    if let Some(typed) = &mut typed {
        g.annotations = mem::take(&mut typed.annotations);
    }
    let body = g.items(&module.items);
    let mut header = Vec::new();
    if let Some(typed) = &typed
        && !typed.imports.is_empty()
    {
        header.push(text(a, &typed.imports));
    }
    let here = options.path(physical);
    header.extend(g.imports_of(&here));
    let mut sections = Vec::new();
    if !header.is_empty() {
        sections.push(lines(a, header));
    }
    sections.extend(
        typed
            .iter()
            .flat_map(|t| &t.declarations)
            .map(|d| text(a, d)),
    );
    sections.extend(body);
    let mut code = a.join(sections, a.lines(2));
    // a module exposing what it imports: JS consumers find it here too
    let mut barrel = Vec::new();
    if physical == module.name && !options.builtins.contains(&module.name.as_str()) {
        let path = |m: &str| relative(&here, &options.path(m));
        let entities: Vec<(&str, Namespace, &str)> = (module.exports.reexports.iter())
            .filter(|r| r.namespace != Namespace::Type || options.typescript())
            .map(|r| (r.module.as_str(), r.namespace, r.name.as_str()))
            .collect();
        let (lines, modules) = reexports(&entities, &path);
        if !lines.is_empty() {
            code = docvec![a, code, a.line(), text(a, &lines)];
        }
        barrel = modules;
    }
    let mut output = print(
        docvec![a, code, a.line()],
        // Release preserves its compact list layout; debug is laid out for reading.
        if options.release { usize::MAX } else { WIDTH },
        #[cfg(feature = "native-project")]
        &g.spans,
        #[cfg(feature = "native-project")]
        &g.names,
    );
    output.imports = g.imports.into_iter().collect();
    output
        .imports
        .extend(typed.into_iter().flat_map(|t| t.modules));
    output.imports.extend(barrel);
    output
}

/// Lines of text another part of the compiler wrote, without the newline
/// that ends them.
fn text<'d>(a: &'d DocumentArena<'d>, lines: &str) -> Document<'d> {
    a.text(a.alloc_str(lines.trim_end_matches('\n')))
}

impl<'a, 'd> Gen<'a, 'd> {
    fn new(module: &'a Module, options: &'a Options<'a>, a: &'d DocumentArena<'d>) -> Self {
        let calls = kernel::calls(options.rt);
        let groups = group::groups(&module.name, &module.items, calls);
        let grouped = group::grouped(&module.name, &module.items, &groups, calls);
        Gen {
            a,
            module: &module.name,
            physical: &module.name,
            location: &|_, _| None,
            options,
            labels: &module.labels,
            #[cfg(feature = "native-project")]
            names: HashMap::new(),
            imports: BTreeSet::new(),
            foreign: Vec::new(),
            runtime: false,
            effect: false,
            kernel: options.rt,
            temps: 0,
            givens: Vec::new(),
            #[cfg(feature = "native-project")]
            spans: Vec::new(),
            buf: Vec::new(),
            generator: false,
            sync: false,
            limit: None,
            collector: None,
            owner: None,
            tail: None,
            groups,
            grouped,
            views: HashMap::new(),
            vars: HashSet::new(),
            rests: HashMap::new(),
            annotations: HashMap::new(),
            sync_exports: HashMap::new(),
            workers: if options.release {
                let exported = |name: &str| {
                    module.exports.values.iter().any(|v| v.name == name)
                        || module
                            .initialization
                            .prefixes
                            .iter()
                            .any(|p| p.bindings.iter().any(|n| n == name))
                };
                let exported_arity = |name: &str| options.worker(&module.name, name);
                release::workers(&module.name, &module.items, &exported, &exported_arity)
            } else {
                HashMap::new()
            },
            aliases: HashMap::new(),
            constructors: module
                .items
                .iter()
                .filter_map(|item| match &item.kind {
                    ItemKind::Constructor(c) => Some(names::ident(&c.name)),
                    _ => None,
                })
                .collect(),
            direct_foreigns: if options.release && !options.typescript() {
                foreign::plain_foreigns(&module.items, &module.name, options)
            } else {
                HashMap::new()
            },
            direct: if options.release {
                release::direct_constants(&module.items)
            } else {
                HashMap::new()
            },
        }
    }
}

/// The module's code, printed once, with the position of each source
/// cursor as a mapping: the span and Flick name of its mark.
fn print(
    code: Document<'_>,
    width: usize,
    #[cfg(feature = "native-project")] spans: &[Span],
    #[cfg(feature = "native-project")] names: &HashMap<usize, String>,
) -> Output {
    let mut out = String::new();
    #[cfg(feature = "native-project")]
    {
        let mut mappings = Vec::new();
        let mut observe = |mark: usize, at: Position| {
            mappings.push((
                u32::try_from(at.line).unwrap_or(u32::MAX),
                u32::try_from(at.utf16_column).unwrap_or(u32::MAX),
                spans[mark],
                names.get(&mark).cloned(),
            ));
        };
        // writing into a `String` cannot fail
        let _ = code.pretty_print_observed(width, &mut out, &mut observe);
        Output {
            code: out,
            mappings,
            imports: Vec::new(),
        }
    }
    #[cfg(not(feature = "native-project"))]
    {
        let _ = code.pretty_print(width, &mut out);
        Output {
            code: out,
            imports: Vec::new(),
        }
    }
}

/// Where a tail call jumps: the function, or the members of its group (see
/// [`group`]), with their arity; the loop variables; the local each
/// parameter of the member being emitted binds, if it is a name.
struct Tail {
    members: Vec<(Target, usize)>,
    /// The member being emitted.
    current: usize,
    vars: Vec<String>,
    binds: Vec<Option<String>>,
    /// In a group, the variable naming the member that runs next.
    state: Option<String>,
    /// The offset variable of each position holding a walked list (see
    /// [`view`]).
    offsets: Vec<Option<String>>,
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "each flag is a separate fact about the code being emitted"
)]
struct Gen<'a, 'd> {
    /// Where the documents of the module's code live.
    a: &'d DocumentArena<'d>,
    module: &'a str,
    physical: &'a str,
    location: &'a dyn Fn(&str, &str) -> Option<String>,
    options: &'a Options<'a>,
    /// The module's labelled top-level values (see [`Gen::labelled`]).
    labels: &'a BTreeMap<String, Label>,
    #[cfg(feature = "native-project")]
    /// The Flick name at a cursor, by its mark.
    names: HashMap<usize, String>,
    /// Modules whose exports are used.
    imports: BTreeSet<String>,
    /// Foreign imports: specifier, export, alias.
    foreign: Vec<(String, String, String)>,
    runtime: bool,
    effect: bool,
    kernel: bool,
    temps: u32,
    /// Dictionaries in scope: as `Dict::Given` names them, and the parameter.
    givens: Vec<(String, String)>,
    #[cfg(feature = "native-project")]
    /// The span of each source cursor, by its mark.
    spans: Vec<Span>,
    /// Statements of the block being emitted.
    buf: Vec<Stmt<'d>>,
    /// Inside the generator of a runtime stage, where `yield*` runs a call.
    generator: bool,
    /// Inside a Sync stage, where calls run at once.
    sync: bool,
    /// The list a strict `[for …]` pushes its elements to; `None` in a lazy
    /// one, which yields them.
    collector: Option<String>,
    /// The limit of the lazy `for` being emitted and its count of elements.
    limit: Option<(Js<'d>, String)>,
    /// The owner the next runtime call acquires into, from `Within`.
    owner: Option<String>,
    tail: Option<Tail>,
    /// The groups of mutually tail-recursive functions of the module.
    groups: Vec<group::Group>,
    /// The group and member of each grouped function, by its global name.
    grouped: HashMap<String, (usize, usize)>,
    /// The walked lists in scope, by their JS name: the offset variable and
    /// the loop position (see [`view`]).
    views: HashMap<String, (String, usize)>,
    vars: HashSet<String>,
    /// The `...rest` of a walked list, by its local: the loop position and
    /// the offset it starts at.
    rests: HashMap<String, (usize, String)>,
    /// Release: the top-level functions of the module with a worker taking
    /// all their arguments at once, by name, with its arity and whether its
    /// curried form is still needed.
    workers: HashMap<String, (usize, bool)>,
    /// Release: the methods of the module's canonical instances emitted as
    /// constants of their own, by instance and method.
    direct: HashMap<(String, String), String>,
    /// Release, JS: the plain pure foreigns of the module (see
    /// [`foreign::plain`]), by name, with their arity and import alias.
    direct_foreigns: HashMap<String, (usize, String)>,
    /// The name each imported module goes by (see [`Gen::alias`]).
    aliases: HashMap<String, String>,
    /// The constructors of the module, which a module alias must not shadow.
    constructors: Vec<String>,
    /// TypeScript types of the exported values, by their JS name.
    annotations: HashMap<String, String>,
    /// Exported functions whose last stage is Sync, by their JS name, with
    /// how many arguments reach it.
    sync_exports: HashMap<String, usize>,
}

/// Includes optional release workers and direct instance methods, whose
/// physical owner is the same as the binding that defines them.
pub(crate) fn runtime_exports(item: &Item) -> Vec<String> {
    let mut names = items::defined(item);
    if let Some((name, arity)) = release::exported_worker(item) {
        names.push(format!("{}${arity}", self::names::ident(name)));
    }
    names.extend(
        release::direct_methods(item)
            .into_iter()
            .map(|(instance, method)| format!("{}${method}", self::names::instance(instance))),
    );
    names
}
