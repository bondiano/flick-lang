//! Accumulated memory-IR program for a persistent VM REPL session.
//!
//! Each newly lowered phrase has its `CodeId`s shifted monotonically above all
//! previously loaded code so closures and tasks from earlier phrases remain
//! valid, while already-registered globals keep their original initializers and
//! cached heap values.

use std::collections::{BTreeMap, BTreeSet};

use flick_core::ir::memory::{
    Block, CodeId, Exit, Function, Global, Instruction, Name, Operation, Program, ValueRep,
};

use crate::tier0::SessionSyntheticCodes;

pub(crate) const CODE_AWAIT_THUNK: CodeId = 0;
pub(crate) const CODE_APPLY_THUNK: CodeId = 1;
pub(crate) const CODE_APPLY_AND_AWAIT_THUNK: CodeId = 2;
const FIRST_USER_CODE_ID: CodeId = 3;

/// Accumulated program with reserved synthetic wrapper code and monotonic `CodeId`s.
#[derive(Debug, Clone)]
pub(crate) struct SessionProgram {
    program: Program,
    next_code_id: CodeId,
    known_globals: BTreeSet<Name>,
}

impl SessionProgram {
    pub(crate) fn new() -> Self {
        let mut functions = BTreeMap::new();
        functions.insert(CODE_AWAIT_THUNK, make_await_thunk());
        functions.insert(CODE_APPLY_THUNK, make_apply_thunk(false));
        functions.insert(CODE_APPLY_AND_AWAIT_THUNK, make_apply_thunk(true));
        let program = Program {
            functions,
            ..Program::default()
        };
        Self {
            program,
            next_code_id: FIRST_USER_CODE_ID,
            known_globals: BTreeSet::new(),
        }
    }

    pub(crate) fn synthetic_codes() -> SessionSyntheticCodes {
        SessionSyntheticCodes {
            await_task: CODE_AWAIT_THUNK,
            apply: CODE_APPLY_THUNK,
            apply_and_await: CODE_APPLY_AND_AWAIT_THUNK,
        }
    }

    pub(crate) fn program(&self) -> &Program {
        &self.program
    }

    /// Merges `incoming` into the accumulated program with strictly monotonic
    /// remapped `CodeId`s. Earlier globals are never overwritten so initialized
    /// bindings and stdlib closures stay live.
    pub(crate) fn load(&mut self, incoming: &Program) {
        let mut id_map: BTreeMap<CodeId, CodeId> = BTreeMap::new();
        for &old_id in incoming.functions.keys() {
            let new_id = self.next_code_id;
            self.next_code_id = self
                .next_code_id
                .checked_add(1)
                .expect("REPL CodeId overflow");
            id_map.insert(old_id, new_id);
        }
        for (&old_id, func) in &incoming.functions {
            let new_id = id_map[&old_id];
            self.program
                .functions
                .insert(new_id, remap_function(func, &id_map));
        }
        for global in &incoming.globals {
            if self.known_globals.insert(global.name.clone()) {
                let initializer = id_map
                    .get(&global.initializer)
                    .copied()
                    .unwrap_or(global.initializer);
                self.program.globals.push(Global {
                    name: global.name.clone(),
                    initializer,
                });
            }
        }
    }

    /// Unregisters globals belonging only to rolled-back phrase `module`.
    /// `next_code_id` and loaded functions remain strictly monotonic so any
    /// stdlib/host globals initialized during the attempt keep valid `CodeId`s.
    pub(crate) fn rollback_module(&mut self, module: &str) {
        self.program.globals.retain(|g| g.name.module != module);
        self.known_globals.retain(|name| name.module != module);
    }
}

/// `[self, unit] -> Drop(unit); t = Project(self, 0); Drop(self); r = Await(t); Return(r)`
fn make_await_thunk() -> Function {
    Function {
        params: vec![0, 1],
        body: Block {
            instructions: vec![
                Instruction::Drop(1),
                Instruction::Let {
                    binding: 2,
                    value: Operation::Project {
                        object: 0,
                        field: 0,
                        rep: ValueRep::Cell,
                    },
                },
                Instruction::Drop(0),
                Instruction::Let {
                    binding: 3,
                    value: Operation::Await(2),
                },
            ],
            exit: Exit::Return(3),
        },
    }
}

/// `[self, unit] -> Drop(unit); f = Project(self, 0); a = Project(self, 1); Drop(self); r = Apply(f, a); [Await(r)]; Return`
fn make_apply_thunk(await_result: bool) -> Function {
    let mut instructions = vec![
        Instruction::Drop(1),
        Instruction::Let {
            binding: 2,
            value: Operation::Project {
                object: 0,
                field: 0,
                rep: ValueRep::Cell,
            },
        },
        Instruction::Let {
            binding: 3,
            value: Operation::Project {
                object: 0,
                field: 1,
                rep: ValueRep::Cell,
            },
        },
        Instruction::Drop(0),
        Instruction::Let {
            binding: 4,
            value: Operation::Apply {
                function: 2,
                argument: 3,
            },
        },
    ];
    let ret = if await_result {
        instructions.push(Instruction::Let {
            binding: 5,
            value: Operation::Await(4),
        });
        5
    } else {
        4
    };
    Function {
        params: vec![0, 1],
        body: Block {
            instructions,
            exit: Exit::Return(ret),
        },
    }
}

fn remap_function(func: &Function, map: &BTreeMap<CodeId, CodeId>) -> Function {
    Function {
        params: func.params.clone(),
        body: remap_block(&func.body, map),
    }
}

fn remap_block(block: &Block, map: &BTreeMap<CodeId, CodeId>) -> Block {
    let instructions = block
        .instructions
        .iter()
        .map(|inst| match inst {
            Instruction::Let { binding, value } => Instruction::Let {
                binding: *binding,
                value: remap_operation(value, map),
            },
            Instruction::Dup(local) => Instruction::Dup(*local),
            Instruction::Drop(local) => Instruction::Drop(*local),
        })
        .collect();
    let exit = match &block.exit {
        Exit::Return(local) => Exit::Return(*local),
        Exit::Branch {
            condition,
            then_,
            else_,
        } => Exit::Branch {
            condition: *condition,
            then_: Box::new(remap_block(then_, map)),
            else_: Box::new(remap_block(else_, map)),
        },
    };
    Block { instructions, exit }
}

fn remap_id(id: CodeId, map: &BTreeMap<CodeId, CodeId>) -> CodeId {
    map.get(&id).copied().unwrap_or(id)
}

fn remap_operation(op: &Operation, map: &BTreeMap<CodeId, CodeId>) -> Operation {
    match op {
        Operation::Closure { code, captures } => Operation::Closure {
            code: remap_id(*code, map),
            captures: captures.clone(),
        },
        Operation::Task { code, captures } => Operation::Task {
            code: remap_id(*code, map),
            captures: captures.clone(),
        },
        Operation::Call { code, args } => Operation::Call {
            code: remap_id(*code, map),
            args: args.clone(),
        },
        Operation::Within { owner, code, args } => Operation::Within {
            owner: *owner,
            code: remap_id(*code, map),
            args: args.clone(),
        },
        Operation::WithOwnerEvidence { owner, code, args } => Operation::WithOwnerEvidence {
            owner: *owner,
            code: remap_id(*code, map),
            args: args.clone(),
        },
        Operation::Loop { id, code, args } => Operation::Loop {
            id: id.clone(),
            code: remap_id(*code, map),
            args: args.clone(),
        },
        Operation::FunctionBoundary { code, args } => Operation::FunctionBoundary {
            code: remap_id(*code, map),
            args: args.clone(),
        },
        Operation::GenerateStrict { code, args } => Operation::GenerateStrict {
            code: remap_id(*code, map),
            args: args.clone(),
        },
        Operation::GenerateLazy {
            code,
            captures,
            limit,
            sync,
        } => Operation::GenerateLazy {
            code: remap_id(*code, map),
            captures: captures.clone(),
            limit: *limit,
            sync: *sync,
        },
        Operation::Literal(_)
        | Operation::Construct { .. }
        | Operation::ConstructWithDicts { .. }
        | Operation::Project { .. }
        | Operation::Record { .. }
        | Operation::Field { .. }
        | Operation::Match { .. }
        | Operation::Update { .. }
        | Operation::Await(_)
        | Operation::Apply { .. }
        | Operation::ReadOwnerEvidence
        | Operation::Global(_)
        | Operation::Binary { .. }
        | Operation::Not(_)
        | Operation::Neg { .. }
        | Operation::Intrinsic { .. }
        | Operation::DropReuse(_)
        | Operation::Reuse { .. }
        | Operation::ReuseWithDicts { .. }
        | Operation::ReuseRecord { .. }
        | Operation::BoxAggregate { .. }
        | Operation::UnboxBorrow { .. }
        | Operation::UnboxConsume { .. }
        | Operation::Handle { .. }
        | Operation::Perform { .. }
        | Operation::ServiceTag(_)
        | Operation::Dictionary { .. }
        | Operation::Method { .. }
        | Operation::Structure { .. }
        | Operation::RecoveryResult { .. }
        | Operation::VarMake(_)
        | Operation::VarGet(_)
        | Operation::VarSet { .. }
        | Operation::LoopExit { .. }
        | Operation::UnwrapOption(_)
        | Operation::Yield(_)
        | Operation::SeqOpen(_)
        | Operation::SeqNext(_)
        | Operation::Label(_)
        | Operation::ForeignCall { .. } => op.clone(),
    }
}
