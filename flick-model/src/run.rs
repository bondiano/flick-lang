//! A program of one file: compiled by flick-core, translated to λFlick
//! and run from its `main`, as a runner does (§2.6): `main ()`, and the
//! task it gives awaited.

use std::collections::HashSet;
use std::rc::Rc;
use std::{fs, io, path, slice};

use flick_core::diagnostics::Severity;
use flick_core::ir::memory::Name as MemoryName;
use flick_core::opt::perceus;
use flick_core::project::{self, Compiled, Host};
use flick_core::source::SourceMap;
use flick_core::{build, ir};

use crate::check::check_program;
use crate::decls::Decls;
use crate::eval::{Machine, Outcome, Program};
use crate::memory::{
    Error as MemoryError, ExecutionReport, Heap, ProgramOutcome, Stats, Value,
    execute_main_program_outcome, execute_rooted_main_program_outcome,
};
use crate::syntax::{Name, Path, Term};
use crate::translate;

mod counted_inputs;

struct Files;

impl Host for Files {
    fn read(&self, p: &path::Path) -> io::Result<Vec<u8>> {
        fs::read(p)
    }

    fn display(&self, p: &path::Path) -> String {
        p.display().to_string()
    }
}

/// One module in memory, at [`SOURCE`].
struct Source<'a>(&'a str);

/// Where [`compile_source`] puts its module.
const SOURCE: &str = "/Main.fl";

impl Host for Source<'_> {
    fn read(&self, p: &path::Path) -> io::Result<Vec<u8>> {
        if p == path::Path::new(SOURCE) {
            Ok(self.0.as_bytes().to_vec())
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    }

    fn display(&self, p: &path::Path) -> String {
        p.display().to_string()
    }
}

/// The modules of the program in `file`, or its errors.
pub fn compile(file: &path::Path) -> Result<Vec<Compiled>, String> {
    let file = fs::canonicalize(file).map_err(|e| format!("{}: {e}", file.display()))?;
    compile_with(&Files, &[project::normalize(&file)])
}

/// The modules of a multi-file program in `files`, or its errors.
pub fn compile_files(files: &[path::PathBuf]) -> Result<Vec<Compiled>, String> {
    let roots: Vec<_> = files
        .iter()
        .map(|f| {
            fs::canonicalize(f)
                .map(|p| project::normalize(&p))
                .map_err(|e| format!("{}: {e}", f.display()))
        })
        .collect::<Result<_, _>>()?;
    compile_with(&Files, &roots)
}

/// The modules of the program of the one module `text`, or its errors.
pub fn compile_source(text: &str) -> Result<Vec<Compiled>, String> {
    compile_with(&Source(text), &[path::PathBuf::from(SOURCE)])
}

fn compile_with(host: &dyn Host, roots: &[path::PathBuf]) -> Result<Vec<Compiled>, String> {
    let mut sources = SourceMap::default();
    let (diagnostics, compilation) = project::compile(host, &mut sources, None, roots);
    let errors: Vec<String> = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    if errors.is_empty() {
        Ok(compilation.modules)
    } else {
        Err(errors.join("\n"))
    }
}

/// The program of `modules` and the built-in modules.
pub fn program(modules: &[Compiled]) -> Program {
    let builtins: &[ir::Module] = build::builtin_modules();
    let decls = decls(modules);
    translate::program_with_decls(modules.iter().map(|m| &m.ir).chain(builtins), &decls)
}

/// What the checker knows of `modules` and the built-in modules.
pub fn decls(modules: &[Compiled]) -> Decls {
    let mut decls = Decls::default();
    let builtins = build::builtin_declarations();
    for d in modules.iter().map(|m| &m.declarations).chain(builtins) {
        decls.add_kinds(d);
    }
    for d in modules.iter().map(|m| &m.declarations).chain(builtins) {
        decls.add(d);
    }
    // Raw foreign shims keep the source declaration's scheme. Provenance is
    // explicit IR metadata; generated names are never decoded or guessed.
    for module in modules
        .iter()
        .map(|m| &m.ir)
        .chain(build::builtin_modules())
    {
        for item in &module.items {
            if let ir::ItemKind::Foreign(foreign) = &item.kind
                && let Some(source) = &foreign.source_name
            {
                let source = Path::new(&module.name, source);
                let raw = Path::new(&module.name, &foreign.name);
                if decls.values.contains_key(&source) {
                    decls.raw_aliases.insert(raw.clone(), source);
                    decls.foreign.insert(raw);
                }
            }
        }
    }
    decls.finish();
    decls
}

/// The type errors of the top-level values of `modules`.
pub fn check(modules: &[Compiled], program: &Program) -> Vec<String> {
    let checked: HashSet<Name> = modules
        .iter()
        .map(|m| Name::from(m.name.as_str()))
        .collect();
    check_program(program, &decls(modules), &checked)
        .into_iter()
        .map(|(p, e)| format!("{p}: {e}"))
        .collect()
}

/// The `main` of the module that exports one.
pub fn main_of(modules: &[Compiled]) -> Result<Path, String> {
    modules
        .iter()
        .find(|m| m.interface.exports_value("main"))
        .map(|m| Path::new(&m.name, "main"))
        .ok_or_else(|| "no module exports `main`".to_owned())
}

/// What a run printed and how it ended.
pub struct Ran {
    pub outcome: Outcome,
    pub stdout: String,
}

/// The layer the runner builds the root services `Σ₀` of `main` from:
/// `Host.live`, when the program has `Host`.
pub fn root_layer() -> Path {
    Path::new("Host", "live")
}

/// Runs `main` of `program` within `fuel` steps, with the live `Host`
/// services as its root (`docs/formal/core.md`, premise G3).
pub fn run_main(program: &Program, main: &Path, fuel: u64) -> Ran {
    let mut machine = Machine::new(program, fuel);
    let root = root_layer();
    if program.globals.contains_key(&root)
        && let Err(outcome) = machine.provide_roots(Term::Global(root))
    {
        return Ran {
            outcome,
            stdout: machine.stdout,
        };
    }
    let mut outcome = machine.run(Term::Global(main.clone()));
    // `main` is a function of `()`, or a task; a task is awaited
    if let Outcome::Value(v @ (Term::Lam(..) | Term::Fix(..) | Term::Apply(..))) = outcome {
        outcome = machine.run(Term::app(v, Term::unit()));
    }
    if let Outcome::Value(t @ Term::Task(_)) = outcome {
        outcome = machine.run(Term::Await(Rc::new(t)));
    }
    Ran {
        outcome,
        stdout: machine.stdout,
    }
}

/// Compiles and runs the program in `file`.
pub fn run_file(file: &path::Path, fuel: u64) -> Result<Ran, String> {
    let modules = compile(file)?;
    let main = main_of(&modules)?;
    Ok(run_main(&program(&modules), &main, fuel))
}

/// A callable Unit-main's output and the measured heap after explicit
/// outcome/global-root release. A successful result requires zero occupied
/// allocation slots, not just balanced allocation/free counters.
pub struct CountedRan {
    pub stdout: String,
    pub memory: Stats,
}

/// Compiles the counted source path and executes it in the caller's heap.
/// Compilation/lowering errors are outer errors, never substitution fallback.
/// Executed return, typed-failure and fault outcomes all preserve stdout.
/// A returned or raised value remains caller-owned: inspect and explicitly
/// drop it before checking [`Heap::finish`]. External heap roots are untouched.
/// General effect handlers, finalizers and other unsupported source forms
/// still require their own explicit memory semantics.
pub fn run_main_counted_outcome(
    heap: &mut Heap,
    modules: &[Compiled],
    main: &Path,
    fuel: u64,
    release: bool,
) -> Result<ExecutionReport, String> {
    let counted = counted_main(modules, main, release)?;
    Ok(counted.execute(heap, fuel))
}

/// The counted program of a `main` and the root layer it runs under, lowered
/// once: a caller may execute it again, as with every fuel.
pub struct CountedMain {
    pub program: ir::memory::Program,
    pub entry: MemoryName,
    pub root: Option<MemoryName>,
}

impl CountedMain {
    /// Runs `main` with its root services, `Σ₀`, when the program has them.
    pub fn execute(&self, heap: &mut Heap, fuel: u64) -> ExecutionReport {
        match &self.root {
            Some(root) => {
                execute_rooted_main_program_outcome(heap, &self.program, &self.entry, root, fuel)
            }
            None => execute_main_program_outcome(heap, &self.program, &self.entry, fuel),
        }
    }
}

/// Lowers `main` of `modules` and its root layer for the counted executor.
pub fn counted_main(
    modules: &[Compiled],
    main: &Path,
    release: bool,
) -> Result<CountedMain, String> {
    let (program, root) = counted_program(modules, slice::from_ref(main), release)?;
    Ok(CountedMain {
        program,
        entry: MemoryName::new(&main.module, &main.name),
        root,
    })
}

/// Lowers multiple entry roots and the root layer once for the counted executor.
pub fn counted_program(
    modules: &[Compiled],
    entries: &[Path],
    release: bool,
) -> Result<(ir::memory::Program, Option<MemoryName>), String> {
    let ir: Vec<_> = modules
        .iter()
        .map(|module| {
            if release {
                build::optimized(modules, module, false)
            } else {
                module.ir.clone()
            }
        })
        .collect();
    let entry_names: Vec<_> = entries
        .iter()
        .map(|p| MemoryName::new(&p.module, &p.name))
        .collect();
    let declarations = counted_inputs::declarations(modules)?;
    let ir = counted_inputs::modules(&ir);
    // the root services, as the evaluator's `run_main` provides them
    let layer = root_layer();
    let defines = |m: &ir::Module| {
        (m.items.iter())
            .any(|i| matches!(&i.kind, ir::ItemKind::Value { name, .. } if *name == *layer.name))
    };
    let root = (ir.iter())
        .any(|m| *m.name == *layer.module && defines(m))
        .then(|| MemoryName::new(&layer.module, &layer.name));
    let roots: Vec<&MemoryName> = entry_names.iter().chain(&root).collect();
    let program = perceus::lower_roots_with_declarations(
        ir.iter().map(AsRef::as_ref),
        declarations.iter().map(AsRef::as_ref),
        &roots,
    )
    .map_err(|error| error.to_string())?;
    Ok((program, root))
}

/// Success-only convenience runner with its own heap. Function and task
/// stages use one execution arena. Typed failures release their payload
/// before returning an error; use [`run_main_counted_outcome`] to inspect
/// failures or observe stdout preceding a fault.
pub fn run_main_counted(
    modules: &[Compiled],
    main: &Path,
    fuel: u64,
    release: bool,
) -> Result<CountedRan, String> {
    let mut heap = Heap::new();
    let report = run_main_counted_outcome(&mut heap, modules, main, fuel, release)?;
    let executed = match report.outcome {
        ProgramOutcome::Returned(value) => Ok(value),
        ProgramOutcome::Raised(value) => heap
            .drop_value(value)
            .and(Err(MemoryError::UnhandledFailure)),
        ProgramOutcome::Failed(cause) => {
            let error = heap.cause_error(&cause);
            heap.drop_cause(cause).and_then(|()| Err(error?))
        }
        ProgramOutcome::Fault(error) => Err(error),
    };
    let value = match executed {
        Ok(value) => value,
        Err(error) => {
            heap.finish()
                .map_err(|leak| format!("{error}; counted cleanup failed: {leak}"))?;
            return Err(error.to_string());
        }
    };
    let unit = value == Value::Unit;
    heap.drop_value(value).map_err(|error| error.to_string())?;
    let memory = heap.finish().map_err(|error| error.to_string())?;
    if !unit {
        return Err("counted main did not return Unit".into());
    }
    Ok(CountedRan {
        stdout: report.stdout,
        memory,
    })
}

/// Compiles and runs a source file on the counted path, in either profile.
pub fn run_file_counted(file: &path::Path, fuel: u64, release: bool) -> Result<CountedRan, String> {
    let modules = compile(file)?;
    let main = main_of(&modules)?;
    run_main_counted(&modules, &main, fuel, release)
}

/// File-based counterpart of [`run_main_counted_outcome`], preserving
/// caller ownership of both the heap and any returned or raised token.
pub fn run_file_counted_outcome(
    heap: &mut Heap,
    file: &path::Path,
    fuel: u64,
    release: bool,
) -> Result<ExecutionReport, String> {
    let modules = compile(file)?;
    let main = main_of(&modules)?;
    run_main_counted_outcome(heap, &modules, &main, fuel, release)
}
