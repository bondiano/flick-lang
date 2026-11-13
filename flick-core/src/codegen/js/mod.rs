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
        #[cfg(feature = "source-maps")]
        {
            Some(($gen).mark($span))
        }
        #[cfg(not(feature = "source-maps"))]
        {
            let _ = &$span;
            None
        }
    }};
}

mod binary;
mod call;
mod decl;
mod doc;
mod exports;
mod expr;
mod foreign;
mod function;
mod generator;
mod group;
mod imports;
mod items;
mod labels;
mod lift;
mod literal;
mod pattern;
mod release;
mod schema;
mod view;

pub use release::{direct_methods, exported_worker};

#[cfg(feature = "typescript")]
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};

#[cfg(feature = "source-maps")]
use flick_pretty::Position;
use flick_pretty::{Document, DocumentArena, docvec};

use super::exports::reexports;
use super::tail::{Callee, Target};
#[cfg(feature = "typescript")]
use super::ts::Projection;
use super::{Options, Output, names};
use crate::ir::{ItemKind, Label, Module};
#[cfg(feature = "source-maps")]
use crate::source::Span;
use doc::{Js, Stmt, WIDTH, lines};
#[cfg(feature = "typescript")]
use exports::annotations;
use exports::sync_exports;

const RUNTIME: &str = "@flick-lang/runtime";

/// Compiles `module` to ESM (or TypeScript, see [`Options::typescript`]).
pub fn js(module: &Module, options: &Options) -> Output {
    assert!(
        cfg!(feature = "typescript") || !options.typescript,
        "TypeScript output requires the `typescript` Cargo feature"
    );
    let arena = DocumentArena::new();
    let a = &arena;
    let mut g = Gen::new(module, options, a);
    #[cfg(feature = "typescript")]
    let projection = Projection::new(&module.name, options.interface, options.package);
    #[cfg(feature = "typescript")]
    let mut declarations = Vec::new();
    let interface = (options.interface)(&module.name);
    if let Some(interface) = &interface {
        g.sync_exports = sync_exports(interface, &module.name);
        // the wrapper of a Sync export is the runtime's
        g.runtime |= !g.sync_exports.is_empty();
    }
    #[cfg(feature = "typescript")]
    if options.typescript
        && let Some(interface) = &interface
    {
        declarations = projection.declarations(interface).0;
        g.annotations = annotations(&projection, interface);
    }
    let body = g.items(&module.items);
    let mut header = Vec::new();
    #[cfg(feature = "typescript")]
    let typed = RefCell::new(Vec::new());
    #[cfg(feature = "typescript")]
    if options.typescript {
        let here = (options.path)(&module.name);
        let path = |m: &str| {
            typed.borrow_mut().push(m.to_owned());
            relative(&here, &(options.path)(m))
        };
        header.push(text(a, &projection.imports(&path)));
    }
    let here = (options.path)(&module.name);
    header.extend(g.imports_of(&here));
    let mut sections = Vec::new();
    if !header.is_empty() {
        sections.push(lines(a, header));
    }
    #[cfg(feature = "typescript")]
    sections.extend(declarations.iter().map(|d| text(a, d)));
    sections.extend(body);
    let mut code = a.join(sections, a.lines(2));
    // a module exposing what it imports: JS consumers find it here too
    let mut barrel = Vec::new();
    if let Some(interface) = (options.interface)(&module.name) {
        let path = |m: &str| relative(&here, &(options.path)(m));
        let (lines, modules) = reexports(&module.name, &interface, &path, &|_| true, &|_| {
            options.typescript()
        });
        if !lines.is_empty() {
            code = docvec![a, code, a.line(), text(a, &lines)];
        }
        barrel = modules;
    }
    let mut output = print(
        docvec![a, code, a.line()],
        #[cfg(feature = "source-maps")]
        &g.spans,
        #[cfg(feature = "source-maps")]
        &g.names,
    );
    let imports = g.imports.into_iter();
    #[cfg(feature = "typescript")]
    let imports = imports.chain(typed.into_inner());
    output.imports = imports.chain(barrel).collect();
    output
}

/// Lines of text another part of the compiler wrote, without the newline
/// that ends them.
fn text<'d>(a: &'d DocumentArena<'d>, lines: &str) -> Document<'d> {
    a.text(a.alloc_str(lines.trim_end_matches('\n')))
}

/// The import specifier of `to` from `from`, both relative to the output
/// root; `npm:` marks a module of a package.
pub fn relative(from: &str, to: &str) -> String {
    if let Some(bare) = to.strip_prefix("npm:") {
        return bare.to_owned();
    }
    let from_dir: Vec<&str> = from.split('/').collect();
    let from_dir = &from_dir[..from_dir.len() - 1];
    let to_parts: Vec<&str> = to.split('/').collect();
    let common = from_dir
        .iter()
        .zip(&to_parts)
        .take_while(|(a, b)| a == b)
        .count();
    let up = from_dir.len() - common;
    let rest = to_parts[common..].join("/");
    if up == 0 {
        format!("./{rest}")
    } else {
        format!("{}{rest}", "../".repeat(up))
    }
}

impl<'a, 'd> Gen<'a, 'd> {
    fn new(module: &'a Module, options: &'a Options<'a>, a: &'d DocumentArena<'d>) -> Self {
        let groups = group::groups(&module.name, &module.items);
        let grouped = group::grouped(&module.name, &module.items, &groups);
        Gen {
            a,
            module: &module.name,
            options,
            labels: &module.labels,
            #[cfg(feature = "source-maps")]
            names: HashMap::new(),
            imports: BTreeSet::new(),
            foreign: Vec::new(),
            runtime: false,
            effect: false,
            temps: 0,
            givens: Vec::new(),
            #[cfg(feature = "source-maps")]
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
            rests: HashMap::new(),
            #[cfg(feature = "typescript")]
            annotations: HashMap::new(),
            sync_exports: HashMap::new(),
            workers: if options.release {
                let interface = (options.interface)(&module.name);
                let exported =
                    |name: &str| interface.as_ref().is_none_or(|i| i.exports_value(name));
                let exported_arity = |name: &str| (options.workers)(&module.name, name);
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
                foreign::plain_foreigns(&module.items)
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
    #[cfg(feature = "source-maps")] spans: &[Span],
    #[cfg(feature = "source-maps")] names: &HashMap<usize, String>,
) -> Output {
    let mut out = String::new();
    #[cfg(feature = "source-maps")]
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
        let _ = code.pretty_print_observed(WIDTH, &mut out, &mut observe);
        Output {
            code: out,
            mappings,
            imports: Vec::new(),
        }
    }
    #[cfg(not(feature = "source-maps"))]
    {
        let _ = code.pretty_print(WIDTH, &mut out);
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
    options: &'a Options<'a>,
    /// The module's labelled top-level values (see [`Gen::labelled`]).
    labels: &'a HashMap<String, Label>,
    #[cfg(feature = "source-maps")]
    /// The Flick name at a cursor, by its mark.
    names: HashMap<usize, String>,
    /// Modules whose exports are used.
    imports: BTreeSet<String>,
    /// Foreign imports: specifier, export, alias.
    foreign: Vec<(String, String, String)>,
    runtime: bool,
    effect: bool,
    temps: u32,
    /// Dictionaries in scope: as `Dict::Given` names them, and the parameter.
    givens: Vec<(String, String)>,
    #[cfg(feature = "source-maps")]
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
    #[cfg(feature = "typescript")]
    /// TypeScript types of the exported values, by their JS name.
    annotations: HashMap<String, String>,
    /// Exported functions whose last stage is Sync, by their JS name, with
    /// how many arguments reach it.
    sync_exports: HashMap<String, usize>,
}
