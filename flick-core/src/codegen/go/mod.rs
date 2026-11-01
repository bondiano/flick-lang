//! Go for a whole program: one `package main` with a file per Flick module
//! and the runtime package `flickrt` beside it. Expressions become Go
//! expressions of static type `flickrt.Value`; what needs statements goes
//! into the current statement list first, as on the JS target. A top-level
//! function is a Go function taking all its arguments at once; a curried
//! value of it is made where one is needed. Top-level values are assigned
//! by an init function per module, run in dependency order, so values that
//! refer to themselves through functions are no Go initialization cycle.
//!
//! Only what `main` reaches is compiled; a construct the target cannot run
//! yet is a diagnostic, never a runtime stub.

mod arith;
mod call;
mod effects;
mod expr;
mod foreign;
mod generate;
mod items;
mod layout;
mod loops;
mod names;
mod pattern;
mod reach;
mod runtime;
mod schema;

pub use runtime::{DRIVER, DRIVER_GO_MOD, DRIVER_GO_SUM, RUNTIME};

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::{mem, slice};

use flick_pretty::{Document, DocumentArena, Documentable, docvec};

use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::ir::{ExprKind, ItemKind, Module, Stage};
use crate::source::Span;
use layout::{Code, block, call, directive, list, statement};
use reach::Key;

/// The Go import path of the runtime package.
pub const RUNTIME_PACKAGE: &str = "flick.app/flickrt";

/// The package of the `PostgreSQL` driver, and the binding that needs it.
const DRIVER_PACKAGE: &str = "flick.app/flickpg";
const DRIVER_MODULE: &str = "@flick-lang/runtime/sql";
const DRIVER_EXPORT: &str = "pgLayer";

/// The bindings that look a named time zone up: a program that reaches one
/// embeds Go's zone database and V8's names of zones; others carry neither.
const ZONE_MODULE: &str = "@flick-lang/runtime/effect/DateTime";
const ZONE_EXPORTS: &[&str] = &[
    "flickZoneMakeNamed",
    "flickZoneFromString",
    "flickSetZoneNamed",
    "flickMakeZonedFromString",
    "zoneMakeLocal",
    "flickJsonZoned",
    "flickJsonZone",
    "flickJsonZoneNamed",
];

/// The source path and line of a span of a module.
pub type Locate<'a> = dyn Fn(&str, Span) -> Option<(String, u32)> + 'a;

/// What a program is made of.
pub struct Program<'a> {
    /// Every module the program may use: its own and the built-in ones.
    pub modules: &'a [&'a Module],
    /// The module whose `main` the program runs.
    pub entry: &'a str,
    /// The test modules whose `tests` the program runs instead, when any.
    pub suites: &'a [String],
    /// The package a module belongs to, for its constructor tags.
    pub package: &'a dyn Fn(&str) -> Option<String>,
    /// Whether a module is the program's own, whose source the build has;
    /// a built-in module's spans are in no source of the build.
    pub own: &'a dyn Fn(&str) -> bool,
    /// The source path and 1-based line of a span of the program's own
    /// module, which `//line` directives give Go's stack traces.
    pub location: &'a Locate<'a>,
    /// The Go side of a foreign module of a module of the program, by its
    /// relative specifier: the file's name in the Go module and its text.
    pub foreign_file: &'a ForeignFile<'a>,
}

/// Finds the Go file beside a foreign JS module: by the Flick module and
/// the specifier, its name in the Go module and its text.
pub type ForeignFile<'a> = dyn Fn(&str, &str) -> Option<(String, String)> + 'a;

/// The Go files of a program, by path in its Go module, and what could not
/// be compiled.
pub struct Output {
    pub files: Vec<(String, String)>,
    pub diagnostics: Vec<Diagnostic>,
    /// Whether the program reaches the `PostgreSQL` driver, which its Go
    /// module then requires.
    pub driver: bool,
}

/// How a top-level name is held in Go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// A variable assigned at initialization.
    Var,
    /// A Go function of the fiber, dictionaries, then parameters; `effects`
    /// has bit `i` set when parameter `i`'s language stage runs effects
    /// (Sync or Runtime), independently of the hidden execution context.
    Func {
        dicts: usize,
        params: usize,
        effects: u64,
    },
}

impl Shape {
    /// Whether the function's last language stage runs effects.
    fn effectful(self) -> bool {
        matches!(self, Shape::Func { params, effects, .. } if params > 0 && effects >> (params - 1) & 1 == 1)
    }
}

/// The effects mask of stages, by whether each runs effects.
fn effects(stages: impl IntoIterator<Item = bool>) -> u64 {
    stages
        .into_iter()
        .enumerate()
        .fold(0, |mask, (i, effectful)| mask | u64::from(effectful) << i)
}

fn shape(kind: &ItemKind) -> Shape {
    match kind {
        ItemKind::Value { dicts, body, .. } => match &body.kind {
            ExprKind::Lambda { params, .. } => Shape::Func {
                dicts: dicts.len(),
                params: params.len(),
                effects: effects(params.iter().map(|p| p.stage != Stage::Pure)),
            },
            _ if dicts.is_empty() => Shape::Var,
            _ => Shape::Func {
                dicts: dicts.len(),
                params: 0,
                effects: 0,
            },
        },
        ItemKind::Foreign(f) => {
            let params = foreign::parameters(f);
            if f.dicts == 0 && params == 0 {
                Shape::Var
            } else {
                Shape::Func {
                    dicts: f.dicts,
                    params,
                    effects: effects(
                        (0..params).map(|i| f.stages.get(i).copied().unwrap_or(false)),
                    ),
                }
            }
        }
        ItemKind::Schema(s) if s.witnesses > 0 => Shape::Func {
            dicts: 0,
            params: s.witnesses,
            effects: 0,
        },
        ItemKind::Instance { info, .. } if !info.context.is_empty() => Shape::Func {
            dicts: info.context.len(),
            params: 0,
            effects: 0,
        },
        _ => Shape::Var,
    }
}

/// Compiles the program that runs `main` of `program.entry`, or the tests
/// of `program.suites`.
pub fn go(program: &Program) -> Output {
    let modules = program.modules;
    let index = reach::index(modules);
    // main.go's documents
    let a = &DocumentArena::new();
    if !program.suites.is_empty() {
        return compile(
            program,
            &index,
            a,
            tests_start(a, program.suites),
            program.suites,
        );
    }
    let main: Key = (program.entry.to_owned(), "main".to_owned());
    let mut diagnostics = Vec::new();
    let main_shape = index
        .get(&main)
        .map(|&(m, i)| shape(&modules[m].items[i].kind));
    let Some(Shape::Func {
        dicts: 0,
        params: 1,
        ..
    }) = main_shape
    else {
        diagnostics.push(Diagnostic::new(
            codes::P012,
            Severity::Error,
            format!(
                "the go target runs a `main` of one parameter, and `{}` has none",
                program.entry
            ),
        ));
        return Output {
            files: Vec::new(),
            diagnostics,
            driver: false,
        };
    };
    let fb = a.text("fb");
    let unit = a.text("flickrt.Unit");
    let start = call(a, names::func(&main.0, &main.1), [fb, unit]);
    compile(
        program,
        &index,
        a,
        start,
        slice::from_ref(&program.entry.to_owned()),
    )
}

/// What a test program runs: `flickrt.RunTests` on the `tests` of each suite.
fn tests_start<'a>(a: &'a DocumentArena<'a>, suites: &[String]) -> Document<'a> {
    let suites = suites.iter().map(|m| {
        let suite = [format!("{m:?}").to_doc(a), names::var(m, "tests").to_doc(a)];
        list(a, "{", suite, "}")
    });
    let suites = list(a, "[]flickrt.Suite{", suites, "}");
    call(a, "flickrt.RunTests", [a.text("fb"), suites])
}

/// The program of the items `roots` reach, whose `main.go`, of documents
/// of `a`, initializes the modules of `roots` and runs `start`.
fn compile<'a>(
    program: &Program,
    index: &HashMap<Key, (usize, usize)>,
    a: &'a DocumentArena<'a>,
    start: Document<'a>,
    roots: &[String],
) -> Output {
    let modules = program.modules;
    let mut diagnostics = Vec::new();
    let keys: Vec<Key> = if program.suites.is_empty() {
        vec![(program.entry.to_owned(), "main".to_owned())]
    } else {
        roots
            .iter()
            .map(|m| (m.clone(), "tests".to_owned()))
            .collect()
    };
    let (reached, missing) = reach::reachable(modules, index, &keys);
    for module in missing {
        diagnostics.push(Diagnostic::new(
            codes::P012,
            Severity::Error,
            format!("the module `{module}` of a package is not available on the go target yet"),
        ));
    }
    let mut shapes = HashMap::new();
    for &(m, i) in &reached {
        let item = &modules[m].items[i];
        for name in reach::defined(item) {
            shapes.insert((modules[m].name.clone(), name), shape(&item.kind));
        }
    }
    let mut files = Vec::new();
    let mut sources = BTreeMap::new();
    // the modules each module's initialization reads
    let mut deps: HashMap<String, BTreeSet<String>> = HashMap::new();
    let mut seen = HashSet::new();
    for (m, module) in modules.iter().enumerate() {
        let items: Vec<usize> = (0..module.items.len())
            .filter(|&i| reached.contains(&(m, i)))
            .collect();
        if items.is_empty() || !seen.insert(module.name.clone()) {
            continue;
        }
        let arena = DocumentArena::new();
        let mut g = Gen::new(&module.name, &shapes, program.package, &arena);
        if (program.own)(&module.name) {
            g.location = Some(program.location);
            g.foreign_file = Some(program.foreign_file);
        }
        let order = item_order(module, &items, index);
        let code = g.module(module, &order);
        diagnostics.extend(g.errors.into_iter().map(|(message, span)| {
            let d = Diagnostic::new(codes::P012, Severity::Error, message);
            match span.filter(|_| (program.own)(&module.name)) {
                Some(span) => d.with_span(span),
                None => d,
            }
        }));
        deps.insert(module.name.clone(), g.modules);
        sources.extend(g.sources);
        files.push((names::file(&module.name), code));
    }
    let mut order = Vec::new();
    let mut visited = HashSet::new();
    for root in roots {
        init_order(root, &deps, &mut visited, &mut order);
    }
    let inits = order
        .iter()
        .filter(|m| deps.contains_key(*m))
        .map(|m| statement(a, docvec![a, names::init(m), "()"]));
    let run = block(a, inits.chain([statement(a, start)]));
    let run = call(
        a,
        "flickrt.Main",
        [docvec![a, "func(fb *flickrt.Fiber) ", run]],
    );
    // the driver registers itself with the runtime as it is imported
    let driver = reached.iter().any(|&(m, i)| {
        matches!(&modules[m].items[i].kind, ItemKind::Foreign(f)
            if f.module.as_deref() == Some(DRIVER_MODULE) && f.export == DRIVER_EXPORT)
    });
    let import_driver = if driver {
        docvec![a, "import _ \"", DRIVER_PACKAGE, "\"", a.lines(2)]
    } else {
        a.empty()
    };
    let zones = reached.iter().any(|&(m, i)| {
        matches!(&modules[m].items[i].kind, ItemKind::Foreign(f)
            if f.module.as_deref() == Some(ZONE_MODULE) && ZONE_EXPORTS.contains(&f.export.as_str()))
    });
    let (import_zones, use_zones) = if zones {
        (
            docvec![a, "import _ \"time/tzdata\"", a.lines(2)],
            Some(statement(a, "flickrt.UseZones()")),
        )
    } else {
        (a.empty(), None)
    };
    let main_go = docvec![
        a,
        "package main",
        a.lines(2),
        "import \"",
        RUNTIME_PACKAGE,
        "\"",
        a.lines(2),
        import_driver,
        import_zones,
        "func main() ",
        block(a, use_zones.into_iter().chain([statement(a, run)])),
        a.line()
    ];
    files.push(("main.go".to_owned(), layout::print(main_go)));
    files.extend(sources);
    dedup(&mut diagnostics);
    Output {
        files,
        diagnostics,
        driver,
    }
}

/// The modules in an order where each one's initialization runs after
/// that of the modules it reads.
fn init_order(
    module: &str,
    deps: &HashMap<String, BTreeSet<String>>,
    visited: &mut HashSet<String>,
    out: &mut Vec<String>,
) {
    if !visited.insert(module.to_owned()) {
        return;
    }
    for dep in deps.get(module).into_iter().flatten() {
        init_order(dep, deps, visited, out);
    }
    out.push(module.to_owned());
}

/// The reached items of a module in an order where everything an item uses
/// in the module is initialized first.
fn item_order(
    module: &Module,
    items: &[usize],
    index: &HashMap<Key, (usize, usize)>,
) -> Vec<usize> {
    let mut state: HashMap<usize, bool> = HashMap::new();
    let mut out = Vec::new();
    for &i in items {
        visit(module, i, index, &mut state, &mut out);
    }
    out
}

fn visit(
    module: &Module,
    i: usize,
    index: &HashMap<Key, (usize, usize)>,
    state: &mut HashMap<usize, bool>,
    out: &mut Vec<usize>,
) {
    if state.contains_key(&i) {
        return;
    }
    state.insert(i, false);
    let mut used = Vec::new();
    reach::uses(&module.name, &module.items[i], &mut used);
    for key in used.iter().filter(|(m, _)| *m == module.name) {
        if let Some(&(_, j)) = index.get(key) {
            visit(module, j, index, state, out);
        }
    }
    state.insert(i, true);
    out.push(i);
}

/// The same message at the same place once.
fn dedup(diagnostics: &mut Vec<Diagnostic>) {
    let mut seen = HashSet::new();
    diagnostics.retain(|d| seen.insert(format!("{:?}{}", d.span, d.message)));
}

/// The code generator of one module, building documents in `a`.
struct Gen<'a> {
    a: &'a DocumentArena<'a>,
    module: &'a str,
    shapes: &'a HashMap<Key, Shape>,
    package: &'a dyn Fn(&str) -> Option<String>,
    /// Statements of the block being emitted, each with its newline.
    buf: Vec<Document<'a>>,
    temps: u32,
    /// How many `//line` directives the module has so far.
    lines: usize,
    /// Dictionaries in scope: as `Dict::Given` names them, and the variable.
    givens: Vec<(String, &'a str)>,
    /// Where the elements of the `for` being emitted go.
    collector: Option<generate::Collector<'a>>,
    /// Inside the function of a lazy `for`, which returns nothing.
    generator: bool,
    /// The modules whose top-level values the code reads.
    modules: BTreeSet<String>,
    /// What the target cannot compile, and where.
    errors: Vec<(String, Option<Span>)>,
    /// The fiber in scope, in a body whose effects run.
    fiber: Option<&'static str>,
    /// The caller's execution context, including in language-pure code.
    context: Option<&'static str>,
    /// The loop a tail call in the code being emitted jumps in.
    tail: Option<loops::Tail<'a>>,
    /// The groups of mutually tail-recursive functions of the module.
    groups: Vec<loops::Group>,
    /// The group and member of each grouped function, by name.
    grouped: HashMap<String, (usize, usize)>,
    /// Where a span of this module is in its source, for `//line`.
    location: Option<&'a Locate<'a>>,
    /// The Go files of foreign modules, for the program's own modules.
    foreign_file: Option<&'a ForeignFile<'a>>,
    /// The Go files the module's foreign bindings call, by name.
    sources: BTreeMap<String, String>,
    /// The locals of the definition being emitted whose Go type is not
    /// `flickrt.Value`.
    natives: HashMap<String, arith::Native>,
}

impl<'a> Gen<'a> {
    fn new(
        module: &'a str,
        shapes: &'a HashMap<Key, Shape>,
        package: &'a dyn Fn(&str) -> Option<String>,
        a: &'a DocumentArena<'a>,
    ) -> Self {
        Gen {
            a,
            module,
            shapes,
            package,
            buf: Vec::new(),
            temps: 0,
            lines: 0,
            givens: Vec::new(),
            collector: None,
            generator: false,
            modules: BTreeSet::new(),
            errors: Vec::new(),
            fiber: None,
            context: None,
            tail: None,
            groups: Vec::new(),
            grouped: HashMap::new(),
            location: None,
            foreign_file: None,
            sources: BTreeMap::new(),
            natives: HashMap::new(),
        }
    }

    fn temp(&mut self) -> &'a str {
        self.temps += 1;
        self.a.alloc_str(&format!("t{}", self.temps))
    }

    /// The code of a name or a literal.
    fn code(&self, text: &str) -> Code<'a> {
        Code::leaf(self.a, text)
    }

    fn unit(&self) -> Code<'a> {
        self.code("flickrt.Unit")
    }

    /// The hidden execution context, absent only in module initialization.
    fn context(&self) -> Document<'a> {
        self.a.text(self.context.unwrap_or("nil"))
    }

    /// A Go string literal of `text`.
    fn string(&self, text: &str) -> Document<'a> {
        names::string(text).to_doc(self.a)
    }

    /// Adds a statement to the current statement list.
    fn emit(&mut self, stmt: impl Documentable<'a>) {
        let stmt = statement(self.a, stmt);
        self.buf.push(stmt);
    }

    /// Records a construct the target cannot compile; the placeholder
    /// keeps the generated code well formed.
    fn unsupported(&mut self, span: Span, what: &str) -> Code<'a> {
        self.errors.push((
            format!("{what} is not available on the go target yet"),
            Some(span),
        ));
        self.unit()
    }

    fn shape_of(&self, module: &str, name: &str) -> Shape {
        self.shapes
            .get(&(module.to_owned(), name.to_owned()))
            .copied()
            .unwrap_or(Shape::Var)
    }

    /// A `//line` directive before the next statement, which Go's stack
    /// traces then name by the Flick source of `span`.
    fn line(&mut self, span: Span) {
        if let Some((path, line)) = self.location.and_then(|at| at(self.module, span)) {
            self.buf.push(directive(self.a, path, line));
            self.lines += 1;
        }
    }

    /// Notes that the code reads a top-level value of `module`.
    fn reads(&mut self, module: &str) {
        if module != self.module {
            self.modules.insert(module.to_owned());
        }
    }

    /// Binds the dictionaries `shown` to fresh variables; the variables.
    fn bind_givens(&mut self, shown: &[String]) -> Vec<&'a str> {
        shown
            .iter()
            .map(|g| {
                let p = self.a.alloc_str(&format!("d{}", self.givens.len()));
                self.givens.push((g.clone(), p));
                p
            })
            .collect()
    }

    /// Runs `f` with an empty statement list; its statements and result.
    fn block<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> (Vec<Document<'a>>, T) {
        let saved = mem::take(&mut self.buf);
        let result = f(self);
        (mem::replace(&mut self.buf, saved), result)
    }
}

#[cfg(test)]
mod tests;
