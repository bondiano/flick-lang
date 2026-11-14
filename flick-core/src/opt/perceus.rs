//! Ownership insertion for the shared memory IR.
//!
//! This runs after source-to-memory ANF lowering, not as a release-only
//! rewrite of typed expressions. [`lower`] accepts either debug or optimized
//! compiler modules; existing JS/Go codegen still has its own typed-IR path.

use std::collections::{BTreeMap, BTreeSet};
use std::{error, fmt};

use crate::ir::memory::{self, Block, Exit, Function, Instruction, Local, Operation};

mod source;
pub use source::{LowerError, lower, lower_with_declarations};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Insertion accepts allocation-only ANF, never an already annotated body.
    Annotated,
    Unbound(Local),
    DuplicateBinding(Local),
    /// Independent validation found a mistake in ownership insertion.
    Balance(memory::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Annotated => f.write_str("ownership insertion requires unannotated memory IR"),
            Self::Unbound(local) => write!(f, "unbound memory local {local}"),
            Self::DuplicateBinding(local) => write!(f, "memory local {local} is bound twice"),
            Self::Balance(error) => error.fmt(f),
        }
    }
}

impl error::Error for Error {}

/// Inserts explicit reference-count operations.
pub fn insert(function: &Function) -> Result<Function, Error> {
    let mut params = BTreeSet::new();
    for &param in &function.params {
        if !params.insert(param) {
            return Err(Error::DuplicateBinding(param));
        }
    }
    source(&function.body, params.clone())?;
    let output = Function {
        params: function.params.clone(),
        body: annotate(&function.body, &analyze(&function.body), params),
    };
    memory::validate(&output).map_err(Error::Balance)?;
    Ok(output)
}

/// The exact local arithmetic proved by `Formal/Memory.lean`: one token
/// plus these copies accounts for all consuming uses and, if needed,
/// one token retained for the continuation. Zero uses without retention
/// instead requires dropping the initial token.
fn copies(consumed: usize, keep: bool) -> usize {
    consumed.saturating_sub(usize::from(!keep))
}

/// Source binding validation is distinct from token validation: before
/// insertion a source local may be read any number of times.
fn source(block: &Block, mut bound: BTreeSet<Local>) -> Result<(), Error> {
    let used = |local, bound: &BTreeSet<Local>| {
        if bound.contains(&local) {
            Ok(())
        } else {
            Err(Error::Unbound(local))
        }
    };
    for instruction in &block.instructions {
        let Instruction::Let { binding, value } = instruction else {
            return Err(Error::Annotated);
        };
        if matches!(
            value,
            Operation::Reuse { .. } | Operation::ReuseWithDicts { .. }
        ) {
            return Err(Error::Annotated);
        }
        for local in value.consumed().into_iter().chain(value.borrowed()) {
            used(local, &bound)?;
        }
        if !bound.insert(*binding) {
            return Err(Error::DuplicateBinding(*binding));
        }
    }
    match &block.exit {
        Exit::Return(local) => used(*local, &bound),
        Exit::Branch {
            condition,
            then_,
            else_,
        } => {
            used(*condition, &bound)?;
            source(then_, bound.clone())?;
            source(else_, bound)
        }
    }
}

struct Liveness {
    entry: BTreeSet<Local>,
    after: Vec<BTreeSet<Local>>,
    branches: Option<(Box<Liveness>, Box<Liveness>)>,
}

/// Local liveness, not syntactic occurrence counts across both branches.
fn before(instruction: &Instruction, live: &mut BTreeSet<Local>) {
    if let Instruction::Let { binding, value } = instruction {
        live.remove(binding);
        live.extend(value.consumed());
        live.extend(value.borrowed());
    }
}

/// Analyze every block once, bottom-up; annotation must not repeatedly
/// rescan complete successor trees for nested conditionals.
fn analyze(block: &Block) -> Liveness {
    let (mut live, branches) = match &block.exit {
        Exit::Return(local) => (BTreeSet::from([*local]), None),
        Exit::Branch {
            condition,
            then_,
            else_,
        } => {
            let then_ = Box::new(analyze(then_));
            let else_ = Box::new(analyze(else_));
            let mut live = then_.entry.clone();
            live.extend(&else_.entry);
            live.insert(*condition);
            (live, Some((then_, else_)))
        }
    };
    let mut after = Vec::with_capacity(block.instructions.len());
    for instruction in block.instructions.iter().rev() {
        after.push(live.clone());
        before(instruction, &mut live);
    }
    after.reverse();
    Liveness {
        entry: live,
        after,
        branches,
    }
}

fn trim(owned: &mut BTreeSet<Local>, live: &BTreeSet<Local>, instructions: &mut Vec<Instruction>) {
    owned.retain(|local| {
        if live.contains(local) {
            true
        } else {
            instructions.push(Instruction::Drop(*local));
            false
        }
    });
}

fn annotate(block: &Block, liveness: &Liveness, mut owned: BTreeSet<Local>) -> Block {
    let mut instructions = Vec::new();
    trim(&mut owned, &liveness.entry, &mut instructions);
    // A just-borrowed, dead receiver may survive exactly to the next
    // constructor. No release is moved across arbitrary work or an edge.
    let mut pending = None;
    for (index, (instruction, after)) in block.instructions.iter().zip(&liveness.after).enumerate()
    {
        let Instruction::Let { binding, value } = instruction else {
            unreachable!("source validation rejects annotations");
        };
        let value = match (pending.take(), value) {
            (Some(object), Operation::Construct { tag, fields }) => Operation::Reuse {
                object,
                tag: tag.clone(),
                fields: fields.clone(),
            },
            (Some(object), Operation::ConstructWithDicts { tag, fields, dicts }) => {
                Operation::ReuseWithDicts {
                    object,
                    tag: tag.clone(),
                    fields: fields.clone(),
                    dicts: dicts.clone(),
                }
            }
            (None, value) => value.clone(),
            (Some(_), _) => unreachable!("only defer to an immediately following constructor"),
        };
        let mut consumed = BTreeMap::<Local, usize>::new();
        for local in value.consumed() {
            *consumed.entry(local).or_default() += 1;
        }
        for (&local, &count) in &consumed {
            for _ in 0..copies(count, after.contains(&local)) {
                instructions.push(Instruction::Dup(local));
            }
            if !after.contains(&local) {
                owned.remove(&local);
            }
        }
        let borrowed = value.borrowed();
        instructions.push(Instruction::Let {
            binding: *binding,
            value,
        });
        if let Some(local) = borrowed
            && !after.contains(&local)
        {
            if matches!(
                block.instructions.get(index + 1),
                Some(Instruction::Let {
                    value: Operation::Construct { .. } | Operation::ConstructWithDicts { .. },
                    ..
                })
            ) {
                pending = Some(local);
            } else {
                instructions.push(Instruction::Drop(local));
                owned.remove(&local);
            }
        }
        owned.insert(*binding);
        if !after.contains(binding) {
            instructions.push(Instruction::Drop(*binding));
            owned.remove(binding);
        }
    }
    let exit = match &block.exit {
        Exit::Return(local) => {
            trim(&mut owned, &BTreeSet::from([*local]), &mut instructions);
            Exit::Return(*local)
        }
        Exit::Branch {
            condition,
            then_,
            else_,
        } => {
            let (then_live, else_live) = liveness.branches.as_ref().expect("analyzed branch");
            let mut successors = then_live.entry.clone();
            successors.extend(&else_live.entry);
            if successors.contains(condition) {
                instructions.push(Instruction::Dup(*condition));
            } else {
                owned.remove(condition);
            }
            trim(&mut owned, &successors, &mut instructions);
            Exit::Branch {
                condition: *condition,
                then_: Box::new(annotate(then_, then_live, owned.clone())),
                else_: Box::new(annotate(else_, else_live, owned)),
            }
        }
    };
    Block { instructions, exit }
}

include!("perceus/ownership_tests.rs");
