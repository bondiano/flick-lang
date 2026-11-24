//! The primitive kernel (`case = "kernel"`): a backend that implements only
//! the kernel of `flick-backend-support/src/kernel/manifest.txt`. Each pure
//! test of a test root runs in the portable evaluator with the kernel's
//! primitives alone ([`prims`]), and the real test runner reports what it
//! gave.
//!
//! A test of a module under `Kernel.` that reaches a binding outside the
//! kernel fails the build; a test of another module is skipped then. An
//! effect test, and one that reaches a kernel primitive the evaluator does
//! not run ([`prims::NOT_EVALUATED`]), is skipped too, and so is a test of
//! another module the evaluator stops on. `kernel-report.json` gives the
//! outcome of each and why.

mod prims;
mod values;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use flick_backend::{Artifact, ArtifactKind, BuildRequest, Diagnostic, Span};
use flick_backend_support::Known;
use flick_backend_support::kernel::{self, Class, Manifest};
use flick_backend_support::opt::eval::{
    DefTable, DictEnv, Env, EvalBudget, EvalConfig, EvalOutcome, Value, evaluate,
};
use flick_backend_support::reach;
use flick_ir::{BinOp, Expr, ExprKind, Item, ItemKind, Lang, Lit, Module, Program};
use serde_json::json;

use super::diagnostic;

/// A test of a `tests` list, as `Test.test`, `Test.group` and
/// `Test.effect_test` build it.
enum Node<'p> {
    Group(&'p str, Vec<Node<'p>>),
    Pure(&'p str, &'p Expr),
    Effect(&'p str, &'p Expr),
}

enum Outcome {
    Passed,
    Failed(String),
    Skipped(String),
}

/// The tests of `body`, or `None` for a shape this case does not read.
fn nodes(body: &Expr) -> Option<Vec<Node<'_>>> {
    match &body.kind {
        ExprKind::List(items) => items.iter().map(node).collect(),
        ExprKind::Binary(BinOp::ListConcat, a, b) => {
            let mut out = nodes(a)?;
            out.extend(nodes(b)?);
            Some(out)
        }
        _ => None,
    }
}

fn node(e: &Expr) -> Option<Node<'_>> {
    let ExprKind::App { func, arg, .. } = &e.kind else {
        return None;
    };
    let ExprKind::App {
        func: head,
        arg: name,
        ..
    } = &func.kind
    else {
        return None;
    };
    let (
        ExprKind::Global {
            module, name: f, ..
        },
        ExprKind::Lit(Lit::String(name)),
    ) = (&head.kind, &name.kind)
    else {
        return None;
    };
    match (module.as_str(), f.as_str(), &arg.kind) {
        ("Test", "group", _) => Some(Node::Group(name, nodes(arg)?)),
        ("Test", "test", ExprKind::Lambda { params, body }) if params.len() == 1 => {
            Some(Node::Pure(name, body))
        }
        ("Test", "effect_test", _) => Some(Node::Effect(name, arg)),
        _ => None,
    }
}

/// What the program defines, and what reaches what.
struct Index<'p> {
    items: BTreeMap<(&'p str, &'p str), &'p Item>,
    manifest: Manifest<'static>,
}

impl<'p> Index<'p> {
    fn new(program: &'p Program) -> Self {
        let mut items = BTreeMap::new();
        for module in &program.modules {
            for item in &module.items {
                let name = match &item.kind {
                    ItemKind::Value { name, .. } | ItemKind::Service { name, .. } => name,
                    ItemKind::Foreign(f) => &f.name,
                    ItemKind::Instance { info, .. } => &info.name,
                    ItemKind::Destructure { .. } | ItemKind::Constructor(_) => continue,
                };
                items.insert((module.name.as_str(), name.as_str()), item);
            }
        }
        Index {
            items,
            manifest: kernel::manifest(),
        }
    }

    /// The bindings `root` reaches, but through the `Test` module, which
    /// is the harness: the classes of the compared type still count, as
    /// their dictionaries are arguments of the test.
    fn reached(&self, root: &Expr) -> Vec<(&'p str, &'p str)> {
        let mut pending = Vec::new();
        expr_uses(root, &mut pending);
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        while let Some((module, name)) = pending.pop() {
            if module == "Test" || !seen.insert((module.clone(), name.clone())) {
                continue;
            }
            let Some(&item) = self.items.get(&(module.as_str(), name.as_str())) else {
                continue;
            };
            if let ItemKind::Foreign(f) = &item.kind {
                out.push(prims::pair(&f.binding).unwrap_or(("inline", &f.name)));
            }
            reach::uses(item, &mut pending);
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    fn outside(&self, reached: &[(&'p str, &'p str)]) -> Vec<String> {
        (reached.iter())
            .filter(|(s, e)| self.manifest.class(s, e) != Some(Class::Kernel))
            .map(|(s, e)| format!("{s} {e}"))
            .collect()
    }
}

fn expr_uses(e: &Expr, out: &mut Vec<reach::Key>) {
    match &e.kind {
        ExprKind::Global { module, name, .. } => out.push((module.clone(), name.clone())),
        ExprKind::Method { dict, .. } => reach::dict_uses(dict, out),
        ExprKind::Dicts(_, dicts) => dicts.iter().for_each(|d| reach::dict_uses(d, out)),
        _ => {}
    }
    e.each_child(&mut |c| expr_uses(c, out));
}

fn strict(module: &str) -> bool {
    module.starts_with("Kernel.")
}

/// The modules of the test roots of `request`.
pub(super) fn roots(request: &BuildRequest) -> Vec<&str> {
    request
        .test_roots
        .iter()
        .map(|r| r.module.as_str())
        .collect()
}

/// The test roots, with their `tests` lists.
fn suites<'p>(program: &'p Program, roots: &[&str]) -> Vec<(&'p Module, &'p Expr)> {
    (program.modules.iter())
        .filter(|m| roots.contains(&m.name.as_str()))
        .filter_map(|m| {
            m.items.iter().find_map(|item| match &item.kind {
                ItemKind::Value { name, body, .. } if name == "tests" => Some((m, body)),
                _ => None,
            })
        })
        .collect()
}

/// The preflight: every test of a `Kernel.` module reaches only kernel
/// bindings, in a list this case reads.
pub(super) fn check_target(program: &Program, roots: &[&str]) -> Result<(), Vec<Diagnostic>> {
    let index = Index::new(program);
    let mut problems = Vec::new();
    for (module, body) in suites(program, roots) {
        if !strict(&module.name) {
            continue;
        }
        let Some(tests) = nodes(body) else {
            problems.extend(diagnostic(
                &format!(
                    "kernel case reads no literal list of tests in `{}`",
                    module.name
                ),
                Some(span(body)),
            ));
            continue;
        };
        each(&tests, &mut String::new(), &mut |path, test| {
            let root = match test {
                Node::Pure(_, body) | Node::Effect(_, body) => body,
                Node::Group(..) => return,
            };
            let outside = index.outside(&index.reached(root));
            if !outside.is_empty() {
                problems.extend(diagnostic(
                    &format!(
                        "`{} > {path}` reaches bindings outside the primitive kernel {}: {}",
                        module.name,
                        kernel::VERSION,
                        outside.join(", ")
                    ),
                    Some(span(root)),
                ));
            }
        });
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

fn span(e: &Expr) -> Span {
    Span {
        file: e.span.file.index(),
        start: e.span.start,
        end: e.span.end,
    }
}

/// Calls `f` with each test and its full name below its module.
fn each<'p>(tests: &[Node<'p>], path: &mut String, f: &mut dyn FnMut(&str, &Node<'p>)) {
    for test in tests {
        let len = path.len();
        let name = match test {
            Node::Group(name, _) | Node::Pure(name, _) | Node::Effect(name, _) => name,
        };
        if !path.is_empty() {
            path.push_str(" > ");
        }
        path.push_str(name);
        f(path, test);
        if let Node::Group(_, inner) = test {
            each(inner, path, f);
        }
        path.truncate(len);
    }
}

/// The evaluator, with the kernel's primitives and every definition of the
/// program; `show` reaches the kernel's hook, not the evaluator's own.
struct Machine<'p> {
    defs: DefTable,
    lang: Vec<flick_ir::LangItem>,
    kernel: prims::Kernel,
    index: Index<'p>,
}

impl<'p> Machine<'p> {
    fn new(program: &'p Program) -> Option<Self> {
        let mut defs = DefTable::default();
        for module in &program.modules {
            for item in &module.items {
                match &item.kind {
                    ItemKind::Value {
                        name, dicts, body, ..
                    } => defs.insert_value(&module.name, name, dicts.clone(), body.clone()),
                    ItemKind::Instance { info, methods } => {
                        for (method, body) in methods {
                            defs.insert_method(
                                &module.name,
                                &info.name,
                                method,
                                info.context.clone(),
                                body.clone(),
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
        let own = |lang: &str| {
            [Lang::ShowMethod, Lang::DisplayMethod]
                .iter()
                .all(|l| l.name() != lang)
        };
        Some(Machine {
            defs,
            lang: (program.lang.iter())
                .filter(|item| own(&item.lang))
                .cloned()
                .collect(),
            kernel: prims::Kernel::new(program, &kernel::manifest())?,
            index: Index::new(program),
        })
    }

    fn run(&self, module: &str, test: &Node<'_>) -> Outcome {
        let body = match test {
            Node::Effect(..) => {
                return Outcome::Skipped("an effect test needs the control kernel".into());
            }
            Node::Pure(_, body) => body,
            Node::Group(..) => unreachable!("groups are emitted, not run"),
        };
        let reached = self.index.reached(body);
        let outside = self.index.outside(&reached);
        if !outside.is_empty() {
            return Outcome::Skipped(format!("outside the kernel: {}", outside.join(", ")));
        }
        if let Some((s, e)) = reached
            .iter()
            .find(|pair| prims::NOT_EVALUATED.contains(pair))
        {
            return Outcome::Skipped(format!("reaches {s} {e}, which the evaluator does not run"));
        }
        let config = EvalConfig {
            max_trial_steps: 50_000_000,
            max_frame_depth: 100_000,
            max_module_steps: u32::MAX,
            max_constructed_bytes: 1 << 30,
            ..EvalConfig::default()
        };
        let mut budget = EvalBudget::new(&config, config.max_module_steps);
        let lang = Known::of(&self.lang);
        let outcome = evaluate(
            body,
            &self.defs,
            Env::default(),
            DictEnv::default(),
            &lang,
            &self.kernel,
            &mut budget,
        );
        match outcome {
            EvalOutcome::Known(Value::Con { ctor, args, .. }) if ctor.module == "Test" => {
                match (ctor.name.as_str(), args.first()) {
                    ("Pass", None) => Outcome::Passed,
                    ("Failed", Some(Value::String(message))) => {
                        Outcome::Failed(message.to_string())
                    }
                    _ => Outcome::Failed(format!("the test gave `{}`", ctor.name)),
                }
            }
            EvalOutcome::Known(other) => Outcome::Failed(format!("the test gave {other:?}")),
            EvalOutcome::Stopped(reason) if strict(module) => {
                Outcome::Failed(format!("the evaluator stopped: {reason:?}"))
            }
            EvalOutcome::Stopped(reason) => {
                Outcome::Skipped(format!("the evaluator stopped: {reason:?}"))
            }
        }
    }
}

fn quote(text: &str) -> String {
    serde_json::to_string(text).expect("JS string")
}

/// The JS module of `module`'s tests: each as the outcome the evaluator
/// gave, a skipped one marked for the runner to skip.
fn emit(
    machine: &Machine<'_>,
    module: &str,
    tests: &[Node<'_>],
    path: &mut String,
    report: &mut Vec<serde_json::Value>,
) -> String {
    let mut out = Vec::new();
    for test in tests {
        let len = path.len();
        let name = match test {
            Node::Group(name, _) | Node::Pure(name, _) | Node::Effect(name, _) => name,
        };
        if !path.is_empty() {
            path.push_str(" > ");
        }
        path.push_str(name);
        let js = if let Node::Group(_, inner) = test {
            format!(
                "testGroup({}, [{}])",
                quote(name),
                emit(machine, module, inner, path, report)
            )
        } else {
            let (status, reason, js) = match machine.run(module, test) {
                Outcome::Passed => ("passed", None, format!("testPure({}, pass)", quote(name))),
                Outcome::Failed(why) => (
                    "failed",
                    Some(why.clone()),
                    format!("testPure({}, failed({}))", quote(name), quote(&why)),
                ),
                Outcome::Skipped(why) => (
                    "skipped",
                    Some(why),
                    format!("testSkip(testPure({}, pass))", quote(name)),
                ),
            };
            report.push(
                json!({ "test": format!("{module} > {path}"), "status": status, "reason": reason }),
            );
            js
        };
        out.push(js);
        path.truncate(len);
    }
    out.join(", ")
}

/// The sources of the test roots, and the report.
/// The JS source of each test root, by module.
pub(super) type Sources = Vec<(String, Vec<u8>)>;

pub(super) fn generate(
    program: &Program,
    roots: &[&str],
    profile: &str,
) -> Result<(Sources, Artifact), Vec<Diagnostic>> {
    let machine = Machine::new(program)
        .ok_or_else(|| diagnostic("kernel case needs Option, Result and Ordering", None))?;
    let mut sources = Vec::new();
    let mut report = Vec::new();
    for (module, body) in suites(program, roots) {
        let Some(tests) = nodes(body) else {
            continue;
        };
        let list = emit(
            &machine,
            &module.name,
            &tests,
            &mut String::new(),
            &mut report,
        );
        let mut source = String::from(
            "import { testGroup, testPure, testSkip } from \"@flick-lang/runtime/rt\";\n\
             import { tag } from \"@flick-lang/runtime/tags\";\n\
             const pass = (_) => ({ _tag: tag(\"Test.Assertion.Pass\") });\n\
             const failed = (message) => (_) => ({ _tag: tag(\"Test.Assertion.Failed\"), _0: message });\n",
        );
        let _ = writeln!(source, "export const tests = [{list}];");
        sources.push((module.name.clone(), source.into_bytes()));
    }
    let bytes = serde_json::to_vec_pretty(&json!({
        "kernel": kernel::VERSION,
        "profile": profile,
        "tests": report,
    }))
    .map_err(|e| diagnostic(&e.to_string(), None))?;
    Ok((
        sources,
        Artifact {
            id: "kernel-report".into(),
            path: "kernel-report.json".into(),
            kind: ArtifactKind::Metadata,
            bytes,
        },
    ))
}
