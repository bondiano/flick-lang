//! A program as the artifacts of a build: one chunk per reached module,
//! each with its source map, in the order they load.
//!
//! - A chunk starts with `local _, ns = ...`: whoever loads it passes the
//!   program's private namespace second, as World of Warcraft passes an
//!   addon's table to each file its `.toc` lists. The chunk puts its module's table there,
//!   `ns["App"] = M`, and defines the reached items into `M` each after what
//!   it uses in the module. No chunk calls `require` or `loadstring`.
//! - Modules load each after the modules its items use, from the roots'
//!   modules first; a cycle between modules is cut where it closes, so a
//!   value one initializes may not read the other's yet, as on the Go
//!   target. Per-item initialization (lexical module prefixes) is refused.
//! - The Lua kernel loads first: its own chunks `_flick/lua-kernel/*.lua`, which
//!   put the table `K` at `ns.k`, then `_flick/lua-kernel/lang.lua`, the
//!   built-in constructors it builds in this program's representation. Each
//!   module's chunk reads `local K = ns.k`.
//! - An own module's chunk is `App/Util.lua`, a built-in one's
//!   `_flick/List.lua`, with `<chunk>.map` beside it: a version 3 map of
//!   each line to the Flick node it starts with, whose sources are the
//!   build's source names. `chunks.txt` lists the chunks in the order they
//!   load, and on LÖVE, `main.lua` loads them in that order.
//! - A test build also writes the runner, `_flick/lua-kernel/runner.lua`, and
//!   `_flick/run-tests.lua`, which an interpreter runs: it loads the chunks of
//!   `chunks.txt` and the runner, and runs the tests of each test module.
//! - A refusal inside a built-in module is reported at the own expression
//!   that reached it, as `check-target` reports what it finds there.

use std::collections::{BTreeMap, BTreeSet};

use flick_backend::{
    Artifact, ArtifactKind, BuildRequest, Diagnostic, Inputs, ModuleRole, Span as SourceSpan,
};
use flick_backend_support::order::post_order;
use flick_backend_support::reach::{self, Key};
use flick_backend_support::sourcemap;
use flick_ir::{FileId, Program, Span};

use super::ast::{Block, Expr, ExprKind, Name, Stmt, StmtKind};
use super::lower::Lowering;
use super::names::{KERNEL, MODULE, NAMESPACE, Namer};
use super::print::{self, Printed};
use super::{Refusal, Target, limits};
use crate::reach::{Blame, Reached};
use crate::refusal;

/// The directory of the chunks of built-in modules; a module's name starts
/// with a capital, so no own chunk is in it.
const BUILTINS: &str = "_flick";

/// The chunks' paths in the order they load, one per line.
const CHUNKS: &str = "chunks.txt";

/// The Lua kernel's own chunks, in the order they load, under
/// `_flick/lua-kernel/`: no module's chunk is there, or at the test entry,
/// as no module's name has a `-`.
const KERNEL_CHUNKS: &[(&str, &str)] = &[
    ("core", include_str!("../../kernel/core.lua")),
    ("strings", include_str!("../../kernel/strings.lua")),
    ("lists", include_str!("../../kernel/lists.lua")),
];

/// The runner a test build loads after every chunk.
const RUNNER: (&str, &str) = ("runner", include_str!("../../kernel/runner.lua"));

/// The entry of a test build, which the interpreter runs.
pub(crate) const TEST_ENTRY: &str = "_flick/run-tests.lua";

/// The path of the kernel chunk `name`.
fn kernel_path(name: &str) -> String {
    format!("{BUILTINS}/lua-kernel/{name}.lua")
}

/// The path of the chunk of `module`.
fn path(module: &str, builtin: bool) -> String {
    let file = format!("{}.lua", module.replace('.', "/"));
    if builtin {
        format!("{BUILTINS}/{file}")
    } else {
        file
    }
}

/// The id of the artifact at `path`.
pub(crate) fn artifact_id(path: &str) -> String {
    format!("lua:{path}")
}

fn artifact(path: String, kind: ArtifactKind, text: String) -> Artifact {
    Artifact {
        id: artifact_id(&path),
        path,
        kind,
        bytes: text.into_bytes(),
    }
}

/// What the build knows of the program.
struct Plan<'p> {
    program: &'p Program,
    reached: &'p Reached,
    builtin: BTreeSet<&'p str>,
    /// Every top-level name, by module and name.
    index: BTreeMap<Key, (usize, usize)>,
}

impl Plan<'_> {
    /// The reached items `at` uses.
    fn uses(&self, at: (usize, usize)) -> Vec<(usize, usize)> {
        let mut keys = Vec::new();
        reach::uses(&self.program.modules[at.0].items[at.1], &mut keys);
        let mut out: Vec<(usize, usize)> = (keys.iter())
            .filter_map(|key| self.index.get(key).copied())
            .filter(|at| self.reached.contains_key(at))
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// The reached items of module `m`, in index order.
    fn items(&self, m: usize) -> Vec<usize> {
        (self.reached.keys())
            .filter(|&&(mi, _)| mi == m)
            .map(|&(_, i)| i)
            .collect()
    }

    /// The reached modules, each after those its items use.
    fn modules(&self, request: &BuildRequest) -> Vec<usize> {
        let program = self.program;
        let by_name = |name: &str| program.modules.iter().position(|m| m.name == name);
        let roots = (request.entries.iter().map(|e| &e.symbol))
            .chain(&request.test_roots)
            .chain(request.exports.iter().map(|e| &e.symbol))
            .filter_map(|s| by_name(&s.module));
        let reached: BTreeSet<usize> = self.reached.keys().map(|&(m, _)| m).collect();
        let starts: Vec<usize> = roots.chain(reached.iter().copied()).collect();
        let mut deps = |m: usize| {
            let mut out: Vec<usize> = (self.items(m).into_iter())
                .flat_map(|i| self.uses((m, i)))
                .map(|(mi, _)| mi)
                .filter(|&mi| mi != m)
                .collect();
            out.sort_unstable();
            out.dedup();
            out
        };
        post_order(program.modules.len(), starts, &mut deps)
            .into_iter()
            .filter(|m| reached.contains(m))
            .collect()
    }

    /// The reached items of module `m`, each after what it uses in `m`.
    fn order(&self, m: usize) -> Vec<usize> {
        let items = self.items(m);
        let mut deps = |i: usize| {
            (self.uses((m, i)).into_iter())
                .filter(|&(mi, _)| mi == m)
                .map(|(_, j)| j)
                .collect()
        };
        post_order(self.program.modules[m].items.len(), items, &mut deps)
    }

    /// `refused` of module `m` as a diagnostic: at its own span in an own
    /// module, and in a built-in one at the own expression that reached
    /// the item it is in, or, for what belongs to the chunk rather than to
    /// one item (a limit of its main function), the module's first item.
    /// The walk blames every reached item of a built-in module, so the
    /// diagnostic without a span is only a guard.
    fn locate(&self, m: usize, refused: Refusal) -> Diagnostic {
        let module = &self.program.modules[m];
        if !self.builtin.contains(module.name.as_str()) {
            return refused.diagnostic();
        }
        let blame = (module.items.iter().enumerate())
            .filter(|(_, item)| {
                item.span.file == refused.span.file
                    && item.span.start <= refused.span.start
                    && refused.span.end <= item.span.end
            })
            .find_map(|(i, _)| self.reached.get(&(m, i)).cloned().flatten())
            .or_else(|| (self.items(m).iter()).find_map(|&i| self.reached[&(m, i)].clone()));
        match blame {
            Some(Blame { span, via }) => refusal(
                refused.message,
                Some(SourceSpan {
                    file: span.file.index(),
                    start: span.start,
                    end: span.end,
                }),
                vec![format!("reached through `{via}`")],
            ),
            None => refusal(
                refused.message,
                None,
                vec![format!("in the built-in module `{}`", module.name)],
            ),
        }
    }
}

/// `local _, ns = ...` and `local K = ns.k`.
fn kernel_prologue(span: Span) -> Vec<Stmt> {
    let (ns, k) = (
        Name::new(NAMESPACE.to_owned()),
        Name::new(KERNEL.to_owned()),
    );
    let table = Expr::name(&ns, span).field("k");
    vec![
        Stmt::new(StmtKind::Args(vec![Namer::unused(), ns]), span),
        Stmt::new(StmtKind::Local(k, Some(table)), span),
    ]
}

/// The kernel's prologue, `local M = {}` and `ns["App"] = M`.
fn prologue(module: &str, span: Span) -> Vec<Stmt> {
    let (ns, m) = (
        Name::new(NAMESPACE.to_owned()),
        Name::new(MODULE.to_owned()),
    );
    let slot = Expr::name(&ns, span).index(Expr::str(module, span));
    let mut stmts = kernel_prologue(span);
    stmts.push(Stmt::new(
        StmtKind::Local(m.clone(), Some(Expr::new(ExprKind::Table(vec![]), span))),
        span,
    ));
    stmts.push(Stmt::new(
        StmtKind::Assign(slot, Expr::name(&m, span)),
        span,
    ));
    stmts
}

/// `_flick/run-tests.lua`: the chunks of `chunks.txt` loaded in order from the
/// directory of the entry's parent, then the runner, which runs the tests
/// of each of `suites` by the interpreter's arguments and gives the exit
/// code. A suite is a module name, which needs no quoting in Lua.
fn test_entry(suites: &[&str]) -> String {
    let runs: Vec<String> = (suites.iter())
        .map(|m| format!("{{ \"{m}\", ns[\"{m}\"].tests }}"))
        .collect();
    format!(
        "-- `flick test` on a Lua interpreter: the chunks in order, then the runner\n\
         local root = string.match(arg[0], \"^(.*)[/\\\\]_flick[/\\\\]run%-tests%.lua$\") or \".\"\n\
         local ns = {{}}\n\
         for path in io.lines(root .. \"/{CHUNKS}\") do\n\
         \x20 assert(loadfile(root .. \"/\" .. path))(nil, ns)\n\
         end\n\
         assert(loadfile(root .. \"/{runner}\"))(nil, ns)\n\
         os.exit(ns.k.run({{ {} }}, {{ ... }}))\n",
        runs.join(", "),
        runner = kernel_path(RUNNER.0),
    )
}

/// LÖVE's `main.lua`: each chunk loaded in order, given the namespace; a
/// chunk that does not load stops it with the reason.
fn love_main(paths: &[String]) -> String {
    let span = Span::new(FileId::new(0), 0, 0);
    let ns = Name::new(NAMESPACE.to_owned());
    let mut block = Block::default();
    let table = Expr::new(ExprKind::Table(vec![]), span);
    block.push(Stmt::new(StmtKind::Local(ns.clone(), Some(table)), span));
    for path in paths {
        let love = Expr::name(&Name::new("love".to_owned()), span);
        let load = love.field("filesystem").field("load");
        let loaded = load.call(vec![Expr::str(path, span)]);
        // the last argument keeps both results: `assert` shows the error
        let assert = Expr::name(&Name::new("assert".to_owned()), span);
        let chunk = assert.call(vec![loaded]);
        let call = chunk.call(vec![Expr::new(ExprKind::Nil, span), Expr::name(&ns, span)]);
        block.push(Stmt::new(StmtKind::Call(call), span));
    }
    print::chunk(&block).code
}

/// The source map of `printed`, written to `file`.
fn map(
    file: &str,
    printed: &Printed,
    request: &BuildRequest,
    texts: &BTreeMap<u32, String>,
) -> String {
    let mappings: Vec<sourcemap::Mapping> = (printed.lines.iter())
        .map(|&(line, span)| (line, 0, span, None))
        .collect();
    let base = file.rsplit('/').next().unwrap_or(file);
    let source_path = |id: FileId| {
        (request.sources.get(id.index() as usize)).map_or_else(String::new, |s| s.name.clone())
    };
    let position = |span: Span| {
        let Some(source) = request.sources.get(span.file.index() as usize) else {
            return (0, 0);
        };
        let line = (source.line_starts)
            .partition_point(|&start| start <= span.start)
            .saturating_sub(1);
        let start = source.line_starts.get(line).copied().unwrap_or(0) as usize;
        let column = (texts.get(&span.file.index()))
            .and_then(|text| text.get(start..span.start as usize))
            .map_or(0, |prefix| prefix.encode_utf16().count());
        (u32::try_from(line).unwrap_or(u32::MAX), column)
    };
    sourcemap::source_map(base, &mappings, &source_path, &position)
}

/// The chunk of module `m` and what the target refuses of it.
fn chunk(
    plan: &Plan<'_>,
    m: usize,
    lowering: &mut Lowering<'_>,
    target: &Target<'_>,
    errors: &mut Vec<Diagnostic>,
) -> (String, Printed) {
    let module = &plan.program.modules[m];
    let order = plan.order(m);
    let span = module.items[order[0]].span;
    if !module.initialization.is_empty() {
        let message = format!(
            "the lexical module prefixes of `{}` are not available on the {} target yet: \
             their values need per-item initialization, which the Lua chunks do not have",
            module.name, target.name
        );
        errors.push(plan.locate(m, Refusal { message, span }));
    }
    let path = path(&module.name, plan.builtin.contains(module.name.as_str()));
    if target.name == "lua/love" && path.eq_ignore_ascii_case("main.lua") {
        let message = format!(
            "the module `{}` is not available on the lua/love target: its chunk would be \
             the game's `main.lua`, which loads the chunks",
            module.name
        );
        errors.push(plan.locate(m, Refusal { message, span }));
    }
    let mut block = Block::default();
    for stmt in prologue(&module.name, span) {
        block.push(stmt);
    }
    let mark = lowering.refusals().len();
    for &i in &order {
        for stmt in lowering.item(m, i) {
            block.push(stmt);
        }
    }
    let refused = lowering.refusals()[mark..].to_vec();
    if refused.is_empty() {
        let over = limits::check(&block, span, target.dialect);
        errors.extend(over.into_iter().map(|r| plan.locate(m, r)));
    }
    errors.extend(refused.into_iter().map(|r| plan.locate(m, r)));
    (path, print::chunk(&block))
}

/// The texts of the sources the lines of `chunks` map to, by file.
fn texts(
    chunks: &[(String, Printed)],
    request: &BuildRequest,
    inputs: &mut dyn Inputs,
) -> Result<BTreeMap<u32, String>, Vec<Diagnostic>> {
    let mut texts = BTreeMap::new();
    for (_, printed) in chunks {
        for &(_, span) in &printed.lines {
            let file = span.file.index();
            let Some(source) = request.sources.get(file as usize) else {
                continue;
            };
            if texts.contains_key(&file) {
                continue;
            }
            let bytes = inputs.read(&source.input).map_err(|e| {
                let message = format!("the source `{}` cannot be read: {e:?}", source.name);
                vec![refusal(message, None, vec![])]
            })?;
            texts.insert(file, String::from_utf8_lossy(&bytes).into_owned());
        }
    }
    Ok(texts)
}

/// The artifacts of `program`, which `check-target` accepted with what it
/// reaches; or what the target refuses of it. A test build runs the tests
/// of `suites`, its test modules.
pub(crate) fn build(
    program: &Program,
    request: &BuildRequest,
    reached: &Reached,
    target: &Target<'_>,
    suites: Option<&[&str]>,
    inputs: &mut dyn Inputs,
) -> Result<Vec<Artifact>, Vec<Diagnostic>> {
    let mut index = BTreeMap::new();
    for (mi, module) in program.modules.iter().enumerate() {
        for (ii, item) in module.items.iter().enumerate() {
            for name in reach::defined(item) {
                index.insert((module.name.clone(), name), (mi, ii));
            }
        }
    }
    let plan = Plan {
        program,
        reached,
        builtin: (request.modules.iter())
            .filter(|m| m.role == ModuleRole::Builtin)
            .map(|m| m.name.as_str())
            .collect(),
        index,
    };
    let mut errors: Vec<Diagnostic> = Vec::new();
    let mut lowering = Lowering::new(program, target);
    let chunks: Vec<(String, Printed)> = (plan.modules(request).into_iter())
        .map(|m| chunk(&plan, m, &mut lowering, target, &mut errors))
        .collect();
    let mark = lowering.refusals().len();
    let lang = {
        let span = Span::new(FileId::new(0), 0, 0);
        let mut block = Block::default();
        for stmt in kernel_prologue(span).into_iter().chain(lowering.lang(span)) {
            block.push(stmt);
        }
        print::chunk(&block).code
    };
    // what the kernel's constructors refuse has no source
    let refused = lowering.refusals()[mark..].iter();
    errors.extend(refused.map(|r| refusal(r.message.clone(), None, vec![])));
    if !errors.is_empty() {
        let mut seen = BTreeSet::new();
        errors.retain(|d| {
            let span = d.span.as_ref().map(|s| (s.file, s.start, s.end));
            seen.insert((d.message.clone(), span))
        });
        return Err(errors);
    }
    let texts = texts(&chunks, request, inputs)?;
    let mut artifacts = Vec::with_capacity(2 * chunks.len() + KERNEL_CHUNKS.len() + 4);
    let mut paths = Vec::with_capacity(chunks.len() + KERNEL_CHUNKS.len() + 1);
    for &(name, text) in KERNEL_CHUNKS {
        paths.push(kernel_path(name));
        artifacts.push(artifact(
            kernel_path(name),
            ArtifactKind::Source,
            text.to_owned(),
        ));
    }
    paths.push(kernel_path("lang"));
    artifacts.push(artifact(kernel_path("lang"), ArtifactKind::Source, lang));
    for (path, printed) in &chunks {
        let source_map = map(path, printed, request, &texts);
        artifacts.push(artifact(
            path.clone(),
            ArtifactKind::Source,
            printed.code.clone(),
        ));
        artifacts.push(artifact(
            format!("{path}.map"),
            ArtifactKind::SourceMap,
            source_map,
        ));
    }
    paths.extend(chunks.into_iter().map(|(path, _)| path));
    // the load order, for a host package's `.toc` or loader
    let order = paths.iter().fold(String::new(), |mut order, path| {
        order.push_str(path);
        order.push('\n');
        order
    });
    artifacts.push(artifact(CHUNKS.into(), ArtifactKind::Metadata, order));
    if target.name == "lua/love" {
        let main = love_main(&paths);
        artifacts.push(artifact("main.lua".into(), ArtifactKind::Source, main));
    }
    if let Some(suites) = suites {
        let (name, runner) = RUNNER;
        artifacts.push(artifact(
            kernel_path(name),
            ArtifactKind::Source,
            runner.to_owned(),
        ));
        artifacts.push(artifact(
            TEST_ENTRY.into(),
            ArtifactKind::Source,
            test_entry(suites),
        ));
    }
    Ok(artifacts)
}
