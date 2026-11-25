//! Real checked compiler IR → ownership insertion → counted execution.
//! Expectations come from source values and existing corpus stdout files,
//! never agreement with the substitution evaluator alone.

use std::fs;
use std::path::Path;

use flick_core::build;
use flick_core::ir::memory::{self, Name};
use flick_core::ir::{Expr, ExprKind, ItemKind, Lit, Pat, Stage, Stmt};
use flick_core::opt::perceus;
use flick_model::memory::{Error, Heap, Value, execute_program};
use flick_model::run;

fn source_program(text: &str, release: bool, entry: &Name) -> memory::Program {
    let modules = run::compile_source(text).expect("source checks");
    let ir: Vec<_> = modules
        .iter()
        .map(|module| {
            if release {
                build::optimized(&modules, module, false)
            } else {
                (*module.ir).clone()
            }
        })
        .collect();
    perceus::lower(ir.iter().chain(build::builtin_modules()), entry)
        .expect("source memory lowering")
}
include!("memory_source/closures_tasks.rs");
include!("memory_source/patterns_records.rs");
include!("memory_source/dictionary_callbacks.rs");
