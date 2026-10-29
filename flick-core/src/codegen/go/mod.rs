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
mod expr;
mod foreign;
mod generate;
mod items;
mod loops;
mod names;
mod pattern;
mod reach;
mod runtime;
mod schema;

pub use runtime::RUNTIME;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write;
use std::{mem, slice};

use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::ir::{ExprKind, ItemKind, Module, Stage};
use crate::source::Span;
use reach::Key;

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
}

/// How a top-level name is held in Go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// A variable assigned at initialization.
    Var,
    /// A Go function of the dictionaries, then the parameters; `effects`
    /// has bit `i` set when parameter `i`'s stage runs effects (Sync or
    /// Runtime). A function whose last stage does takes the fiber first.
    Func {
        dicts: usize,
        params: usize,
        effects: u64,
    },
}

impl Shape {
    /// Whether the Go function takes the fiber first.
    fn fiber(self) -> bool {
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
    if !program.suites.is_empty() {
        return compile(
            program,
            &index,
            &tests_start(program.suites),
            program.suites,
        );
    }
    let main: Key = (program.entry.to_owned(), "main".to_owned());
    let mut diagnostics = Vec::new();
    let main_shape = index
        .get(&main)
        .map(|&(m, i)| shape(&modules[m].items[i].kind));
    let Some(
        main_shape @ Shape::Func {
            dicts: 0,
            params: 1,
            ..
        },
    ) = main_shape
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
        };
    };
    let fb = if main_shape.fiber() { "fb, " } else { "" };
    let start = format!("{}({fb}flickrt.Unit)", names::func(&main.0, &main.1));
    compile(
        program,
        &index,
        &start,
        slice::from_ref(&program.entry.to_owned()),
    )
}

/// What a test program runs: `flickrt.RunTests` on the `tests` of each suite.
fn tests_start(suites: &[String]) -> String {
    let list: Vec<String> = suites
        .iter()
        .map(|m| format!("{{{:?}, {}}}", m, names::var(m, "tests")))
        .collect();
    format!(
        "flickrt.RunTests(fb, []flickrt.Suite{{{}}})",
        list.join(", ")
    )
}

/// The program of the items `roots` reach, whose `main.go` initializes
/// the modules of `roots` and runs `start`.
fn compile(
    program: &Program,
    index: &HashMap<Key, (usize, usize)>,
    start: &str,
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
        let mut g = Gen::new(&module.name, &shapes, program.package);
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
    let mut main_go = format!(
        "package main\n\nimport \"{RUNTIME_PACKAGE}\"\n\nfunc main() {{\n\tflickrt.Main(func(fb *flickrt.Fiber) {{\n"
    );
    for module in order.iter().filter(|m| deps.contains_key(*m)) {
        let _ = writeln!(main_go, "\t\t{}()", names::init(module));
    }
    let _ = writeln!(main_go, "\t\t{start}\n\t}})\n}}");
    files.push(("main.go".to_owned(), main_go));
    files.extend(sources);
    dedup(&mut diagnostics);
    Output { files, diagnostics }
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

/// The code generator of one module.
struct Gen<'a> {
    module: &'a str,
    shapes: &'a HashMap<Key, Shape>,
    package: &'a dyn Fn(&str) -> Option<String>,
    /// Statements of the block being emitted.
    buf: Vec<String>,
    temps: u32,
    /// Dictionaries in scope: as `Dict::Given` names them, and the variable.
    givens: Vec<(String, String)>,
    /// Where the elements of the `for` being emitted go.
    collector: Option<generate::Collector>,
    /// Inside the function of a lazy `for`, which returns nothing.
    generator: bool,
    /// The modules whose top-level values the code reads.
    modules: BTreeSet<String>,
    /// What the target cannot compile, and where.
    errors: Vec<(String, Option<Span>)>,
    /// The fiber in scope, in a body whose effects run.
    fiber: Option<&'static str>,
    /// The loop a tail call in the code being emitted jumps in.
    tail: Option<loops::Tail>,
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
    ) -> Self {
        Gen {
            module,
            shapes,
            package,
            buf: Vec::new(),
            temps: 0,
            givens: Vec::new(),
            collector: None,
            generator: false,
            modules: BTreeSet::new(),
            errors: Vec::new(),
            fiber: None,
            tail: None,
            groups: Vec::new(),
            grouped: HashMap::new(),
            location: None,
            foreign_file: None,
            sources: BTreeMap::new(),
            natives: HashMap::new(),
        }
    }

    fn temp(&mut self) -> String {
        self.temps += 1;
        format!("t{}", self.temps)
    }

    /// Records a construct the target cannot compile; the placeholder
    /// keeps the generated code well formed.
    fn unsupported(&mut self, span: Span, what: &str) -> String {
        self.errors.push((
            format!("{what} is not available on the go target yet"),
            Some(span),
        ));
        "flickrt.Unit".to_owned()
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
            self.buf.push(format!("//line {path}:{line}"));
        }
    }

    /// Notes that the code reads a top-level value of `module`.
    fn reads(&mut self, module: &str) {
        if module != self.module {
            self.modules.insert(module.to_owned());
        }
    }

    /// Binds the dictionaries `shown` to fresh variables; the variables.
    fn bind_givens(&mut self, shown: &[String]) -> Vec<String> {
        shown
            .iter()
            .map(|g| {
                let p = format!("d{}", self.givens.len());
                self.givens.push((g.clone(), p.clone()));
                p
            })
            .collect()
    }

    /// Runs `f` with an empty statement list; its statements and result.
    fn block<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> (Vec<String>, T) {
        let saved = mem::take(&mut self.buf);
        let result = f(self);
        (mem::replace(&mut self.buf, saved), result)
    }
}

/// Statements as the lines of a block, indented one level; a `//line`
/// directive stays at the start of its line, where Go reads it.
fn indent(stmts: &[String]) -> String {
    let mut out = String::new();
    for stmt in stmts {
        for line in stmt.lines() {
            if !line.starts_with("//line ") {
                out.push('\t');
            }
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// The parameter list of a Go function taking the fiber, when `fiber`,
/// then `names`, each a `Value`.
fn signature(fiber: bool, names: &[String]) -> String {
    let params = param_list(names);
    match (fiber, params.is_empty()) {
        (false, _) => params,
        (true, true) => "fb *flickrt.Fiber".to_owned(),
        (true, false) => format!("fb *flickrt.Fiber, {params}"),
    }
}

/// The parameter list of Go function taking `names`, each a `Value`.
fn param_list(names: &[String]) -> String {
    if names.is_empty() {
        String::new()
    } else {
        format!("{} flickrt.Value", names.join(", "))
    }
}

#[cfg(test)]
mod tests;
