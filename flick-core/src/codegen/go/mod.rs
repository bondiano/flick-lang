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
mod closures;
mod effects;
mod expr;
mod foreign;
mod generate;
mod handle;
mod items;
mod layout;
mod loops;
mod main_file;
mod names;
mod once;
mod own;
mod pattern;
mod runtime;
mod shape;
mod typed;

pub use runtime::{GO_MOD, GO_SUM, PACKAGES, Package, RUNTIME};

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::{mem, slice};

use flick_pretty::{Document, DocumentArena, Documentable, docvec};

use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::ir::{ItemKind, Module, reach};
use crate::source::Span;
use layout::{Code, block, call, directive, list, statement};
use reach::Key;
use shape::{Shape, shape};

/// The Go import path of the runtime package.
pub const RUNTIME_PACKAGE: &str = "flick.app/flickrt";

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
    /// The optional packages the program reaches (`main_file::OPTIONAL`), whose
    /// modules its Go module then requires.
    pub packages: Vec<&'static str>,
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
        .map(|&(m, i)| shape(&modules[m].name, &modules[m].items[i].kind));
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
            packages: Vec::new(),
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
            let held = shape(&modules[m].name, &item.kind);
            shapes.insert((modules[m].name.clone(), name), held);
        }
    }
    let reached_items = reached
        .iter()
        .map(|&(m, i)| (&*modules[m].name, &modules[m].items[i]));
    let settled = shape::settled(reached_items, &shapes);
    let item_initializers = modules.iter().any(|m| !m.initialization.is_empty());
    let mut files = Vec::new();
    let mut sources = BTreeMap::new();
    // the modules each module's initialization reads
    let mut deps: HashMap<String, BTreeSet<String>> = HashMap::new();
    let mut seen = HashSet::new();
    let (mut owned, records) = (own::Owned::new(), own::records(modules));
    for (m, module) in modules.iter().enumerate() {
        let items: Vec<usize> = (0..module.items.len())
            .filter(|&i| reached.contains(&(m, i)))
            .collect();
        if items.is_empty() || !seen.insert(module.name.clone()) {
            continue;
        }
        let arena = DocumentArena::new();
        let mut g = Gen::new(&module.name, &shapes, program.package, &arena);
        g.settled = Some(&settled);
        g.own = Some(program.own);
        g.records = Some(&records);
        g.item_initializers = item_initializers;
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
        owned.extend(g.owned);
        sources.extend(g.sources);
        files.push((names::file(&module.name), code));
    }
    let inits: Vec<_> = if item_initializers {
        initialization_order(modules, index, &reached)
            .into_iter()
            .filter(|&(m, i)| has_initializer(&modules[m].items[i].kind))
            .map(|(m, i)| statement(a, docvec![a, names::item_init(&modules[m].name, i), "()"]))
            .collect()
    } else {
        module_inits(a, roots, &deps)
    };
    let run = block(a, inits.into_iter().chain([statement(a, start)]));
    let run = call(
        a,
        "flickrt.Main",
        [docvec![a, "func(fb *flickrt.Fiber) ", run]],
    );
    let packages = main_file::packages(modules, &reached);
    let zones = main_file::zones(modules, &reached);
    files.push((
        "main.go".to_owned(),
        main_file::main_go(a, run, &packages, zones),
    ));
    files.extend(sources);
    if !owned.is_empty() {
        files.push(("own_types.go".to_owned(), own::file(&owned)));
    }
    dedup(&mut diagnostics);
    Output {
        files,
        diagnostics,
        packages,
    }
}

fn has_initializer(item: &ItemKind) -> bool {
    match item {
        // the shape of a value does not depend on its module
        ItemKind::Value { .. } => matches!(shape("", item), Shape::Var),
        ItemKind::Instance { info, .. } => info.context.is_empty(),
        ItemKind::Destructure { .. } => true,
        ItemKind::Foreign(_) | ItemKind::Constructor(_) | ItemKind::Service { .. } => false,
    }
}

/// Lexical module references may form a module-level cycle while their
/// value-initialization graph is acyclic. Schedule that graph directly.
fn initialization_order(
    modules: &[&Module],
    index: &HashMap<Key, (usize, usize)>,
    reached: &HashSet<(usize, usize)>,
) -> Vec<(usize, usize)> {
    fn visit(
        at: (usize, usize),
        modules: &[&Module],
        index: &HashMap<Key, (usize, usize)>,
        reached: &HashSet<(usize, usize)>,
        seen: &mut HashSet<(usize, usize)>,
        out: &mut Vec<(usize, usize)>,
    ) {
        if !seen.insert(at) {
            return;
        }
        let mut used = Vec::new();
        reach::uses(&modules[at.0].items[at.1], &mut used);
        for key in used {
            if let Some(&next) = index.get(&key)
                && reached.contains(&next)
            {
                visit(next, modules, index, reached, seen, out);
            }
        }
        out.push(at);
    }
    let mut items: Vec<_> = reached.iter().copied().collect();
    items.sort_unstable();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for at in items {
        visit(at, modules, index, reached, &mut seen, &mut out);
    }
    out
}

/// The modules in an order where each one's initialization runs after
/// that of the modules it reads.
/// The calls initializing the modules `roots` read, each after those it
/// reads.
fn module_inits<'a>(
    a: &'a DocumentArena<'a>,
    roots: &[String],
    deps: &HashMap<String, BTreeSet<String>>,
) -> Vec<Document<'a>> {
    let mut order = Vec::new();
    let mut visited = HashSet::new();
    for root in roots {
        init_order(root, deps, &mut visited, &mut order);
    }
    order
        .iter()
        .filter(|m| deps.contains_key(*m))
        .map(|m| statement(a, docvec![a, names::init(m), "()"]))
        .collect()
}

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
    reach::uses(&module.items[i], &mut used);
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
    item_initializers: bool,
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
    /// What the module makes once, each a package variable (`once.rs`).
    once: Vec<String>,
    /// The instances whose dictionary is the same whoever builds it.
    settled: Option<&'a HashSet<Key>>,
    /// The Go type the function being emitted returns, when not a value.
    returns: Option<arith::Native>,
    /// Whether a module is the program's own: its constructors with fields
    /// have Go types of their own (`own.rs`).
    own: Option<&'a dyn Fn(&str) -> bool>,
    /// The Go types of constructors the module's code names.
    owned: own::Owned,
    /// The constructors of the program declared over a record.
    records: Option<&'a own::Records>,
    /// The locals of the definition being emitted bound to the record that
    /// lies in a value of a constructor, with its labels in their places.
    held: HashMap<String, Vec<String>>,
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
            item_initializers: false,
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
            once: Vec::new(),
            settled: None,
            returns: None,
            own: None,
            owned: own::Owned::new(),
            records: None,
            held: HashMap::new(),
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
mod compose_tests;
#[cfg(test)]
mod own_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod typed_tests;
