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

mod call;
mod expr;
mod foreign;
mod generate;
mod items;
mod names;
mod pattern;
mod reach;
mod runtime;

pub use runtime::RUNTIME;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Write;
use std::{mem, slice};

use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::ir::{ExprKind, ItemKind, Module};
use crate::source::Span;
use reach::Key;

/// The Go import path of the runtime package.
pub const RUNTIME_PACKAGE: &str = "flick.app/flickrt";

/// What a program is made of.
pub struct Program<'a> {
    /// Every module the program may use: its own and the built-in ones.
    pub modules: &'a [&'a Module],
    /// The module whose `main` the program runs.
    pub entry: &'a str,
    /// The package a module belongs to, for its constructor tags.
    pub package: &'a dyn Fn(&str) -> Option<String>,
    /// Whether a module is the program's own, whose source the build has;
    /// a built-in module's spans are in no source of the build.
    pub own: &'a dyn Fn(&str) -> bool,
}

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
    /// A Go function of the dictionaries, then the parameters.
    Func { dicts: usize, params: usize },
}

fn shape(kind: &ItemKind) -> Shape {
    match kind {
        ItemKind::Value { dicts, body, .. } => match &body.kind {
            ExprKind::Lambda { params, .. } => Shape::Func {
                dicts: dicts.len(),
                params: params.len(),
            },
            _ if dicts.is_empty() => Shape::Var,
            _ => Shape::Func {
                dicts: dicts.len(),
                params: 0,
            },
        },
        ItemKind::Foreign(f) => {
            let params = f.info.stages_taken();
            if f.dicts == 0 && params == 0 {
                Shape::Var
            } else {
                Shape::Func {
                    dicts: f.dicts,
                    params,
                }
            }
        }
        ItemKind::Instance { info, .. } if !info.context.is_empty() => Shape::Func {
            dicts: info.context.len(),
            params: 0,
        },
        _ => Shape::Var,
    }
}

/// Compiles the program that runs `main` of `program.entry`.
pub fn go(program: &Program) -> Output {
    let modules = program.modules;
    let index = reach::index(modules);
    let main: Key = (program.entry.to_owned(), "main".to_owned());
    let mut diagnostics = Vec::new();
    let main_shape = index
        .get(&main)
        .map(|&(m, i)| shape(&modules[m].items[i].kind));
    let runnable = Some(Shape::Func {
        dicts: 0,
        params: 1,
    });
    if main_shape != runnable {
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
    }
    let start = format!("{}(flickrt.Unit)", names::func(&main.0, &main.1));
    let (reached, missing) = reach::reachable(modules, &index, slice::from_ref(&main));
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
        let order = item_order(module, &items, &index);
        let code = g.module(module, &order);
        diagnostics.extend(g.errors.into_iter().map(|(message, span)| {
            let d = Diagnostic::new(codes::P012, Severity::Error, message);
            match span.filter(|_| (program.own)(&module.name)) {
                Some(span) => d.with_span(span),
                None => d,
            }
        }));
        deps.insert(module.name.clone(), g.modules);
        files.push((names::file(&module.name), code));
    }
    let mut order = Vec::new();
    let mut visited = HashSet::new();
    init_order(program.entry, &deps, &mut visited, &mut order);
    let mut main_go = format!(
        "package main\n\nimport \"{RUNTIME_PACKAGE}\"\n\nfunc main() {{\n\tflickrt.Main(func() {{\n"
    );
    for module in order.iter().filter(|m| deps.contains_key(*m)) {
        let _ = writeln!(main_go, "\t\t{}()", names::init(module));
    }
    let _ = writeln!(main_go, "\t\t{start}\n\t}})\n}}");
    files.push(("main.go".to_owned(), main_go));
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
    /// The list a strict `[for …]` appends its elements to.
    collector: Option<String>,
    /// The modules whose top-level values the code reads.
    modules: BTreeSet<String>,
    /// What the target cannot compile, and where.
    errors: Vec<(String, Option<Span>)>,
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
            modules: BTreeSet::new(),
            errors: Vec::new(),
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

/// Statements as the lines of a block, indented one level.
fn indent(stmts: &[String]) -> String {
    let mut out = String::new();
    for stmt in stmts {
        for line in stmt.lines() {
            out.push('\t');
            out.push_str(line);
            out.push('\n');
        }
    }
    out
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
