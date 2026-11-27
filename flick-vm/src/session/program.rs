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
    pub(super) program: Program,
    next_code_id: CodeId,
    pub(super) known_globals: BTreeSet<Name>,
    pub(super) code_owner: BTreeMap<CodeId, String>,
    pub(super) retired_codes: BTreeMap<String, Vec<CodeId>>,
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
            code_owner: BTreeMap::new(),
            retired_codes: BTreeMap::new(),
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

    pub(crate) fn code_owner(&self) -> &BTreeMap<CodeId, String> {
        &self.code_owner
    }

    /// Merges `incoming` into the accumulated program with strictly monotonic
    /// remapped `CodeId`s. Earlier globals are never overwritten so initialized
    /// bindings and stdlib closures stay live.
    pub(crate) fn load(&mut self, incoming: &Program) {
        let _ = self.load_with_owners(incoming, None);
    }

    /// Merges `incoming` with explicit module ownership for newly loaded code
    /// and returns the `old -> new` `CodeId` remapping.
    pub(crate) fn load_with_owners(
        &mut self,
        incoming: &Program,
        explicit_owner: Option<&BTreeMap<CodeId, String>>,
    ) -> BTreeMap<CodeId, CodeId> {
        let mut id_map: BTreeMap<CodeId, CodeId> = BTreeMap::new();
        for &old_id in incoming.functions.keys() {
            let new_id = self.next_code_id;
            self.next_code_id = self
                .next_code_id
                .checked_add(1)
                .expect("REPL CodeId overflow");
            id_map.insert(old_id, new_id);
        }
        let inferred_owners = if explicit_owner.is_some() {
            BTreeMap::new()
        } else {
            infer_code_owners(incoming)
        };
        for (&old_id, func) in &incoming.functions {
            let new_id = id_map[&old_id];
            self.program
                .functions
                .insert(new_id, remap_function(func, &id_map));
            let owner = explicit_owner
                .and_then(|m| m.get(&old_id))
                .or_else(|| inferred_owners.get(&old_id));
            if let Some(module) = owner {
                self.code_owner.insert(new_id, module.clone());
            }
        }
        for (&(old_code, idx), &span) in &incoming.op_spans {
            if let Some(&new_code) = id_map.get(&old_code) {
                self.program.op_spans.insert((new_code, idx), span);
            }
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
        id_map
    }

    /// Unregisters globals belonging only to rolled-back phrase `module`.
    /// `next_code_id` and loaded functions remain strictly monotonic so any
    /// stdlib/host globals initialized during the attempt keep valid `CodeId`s.
    pub(crate) fn rollback_module(&mut self, module: &str) {
        self.program.globals.retain(|g| g.name.module != module);
        self.known_globals.retain(|name| name.module != module);
    }

    /// Removes `module`'s globals and marks its code IDs as retired (kept alive
    /// while live closures still reference them, then reclaimed once unreferenced).
    pub(crate) fn unload_module(&mut self, module: &str) -> Vec<Name> {
        let mut removed = Vec::new();
        self.program.globals.retain(|g| {
            if g.name.module == module {
                removed.push(g.name.clone());
                false
            } else {
                true
            }
        });
        self.known_globals.retain(|name| name.module != module);
        let owned: Vec<CodeId> = self
            .code_owner
            .iter()
            .filter_map(|(&id, owner)| (owner == module).then_some(id))
            .collect();
        if !owned.is_empty() {
            self.retired_codes
                .entry(module.to_owned())
                .or_default()
                .extend(owned);
        }
        removed
    }

    /// Reclaims retired code bodies that are no longer referenced by any live
    /// closure, task, or generator on the heap or any remaining global.
    pub(crate) fn reclaim_retired_codes(&mut self, live_heap_codes: &BTreeSet<CodeId>) {
        if self.retired_codes.is_empty() {
            return;
        }
        let mut reachable = live_heap_codes.clone();
        for global in &self.program.globals {
            reachable.insert(global.initializer);
        }
        // Close transitively over remaining reachable functions so helper calls
        // inside a surviving closure stay intact.
        let mut work: Vec<CodeId> = reachable.iter().copied().collect();
        while let Some(id) = work.pop() {
            if let Some(func) = self.program.functions.get(&id) {
                let mut refs = BTreeSet::new();
                collect_block_codes(&func.body, &mut refs);
                for target in refs {
                    if reachable.insert(target) {
                        work.push(target);
                    }
                }
            }
        }
        self.retired_codes.retain(|_, codes| {
            codes.retain(|id| {
                if reachable.contains(id) {
                    true
                } else {
                    self.program.functions.remove(id);
                    self.code_owner.remove(id);
                    self.program.op_spans.retain(|&(c, _), _| c != *id);
                    false
                }
            });
            !codes.is_empty()
        });
    }

    /// Returns all external modules referenced via `Operation::Global` from
    /// functions reachable from `module`'s globals.
    pub(crate) fn module_global_dependencies(&self, module: &str) -> BTreeSet<String> {
        let mut visited = BTreeSet::new();
        let mut work: Vec<CodeId> = self
            .program
            .globals
            .iter()
            .filter(|g| g.name.module == module)
            .map(|g| g.initializer)
            .collect();
        let mut deps = BTreeSet::new();
        while let Some(id) = work.pop() {
            if !visited.insert(id) {
                continue;
            }
            if let Some(func) = self.program.functions.get(&id) {
                collect_block_globals(&func.body, module, &mut deps);
                let mut refs = BTreeSet::new();
                collect_block_codes(&func.body, &mut refs);
                work.extend(refs);
            }
        }
        deps
    }

    /// Extracts a compact, self-contained memory-IR `Program` slice covering all
    /// globals belonging to `modules` and all code transitively reachable from
    /// those globals plus `extra_codes` (such as live heap closures).
    ///
    /// Returns `(sliced_program, live_to_image_code_id, image_code_owners)`.
    pub(crate) fn extract_slice(
        &self,
        modules: &BTreeSet<String>,
        extra_codes: &BTreeSet<CodeId>,
    ) -> (Program, BTreeMap<CodeId, CodeId>, BTreeMap<CodeId, String>) {
        let mut visited = BTreeSet::new();
        let mut work: Vec<CodeId> = self
            .program
            .globals
            .iter()
            .filter(|g| modules.contains(&g.name.module))
            .map(|g| g.initializer)
            .chain(extra_codes.iter().copied())
            .collect();
        while let Some(id) = work.pop() {
            if !visited.insert(id) {
                continue;
            }
            if let Some(func) = self.program.functions.get(&id) {
                let mut refs = BTreeSet::new();
                collect_block_codes(&func.body, &mut refs);
                work.extend(refs);
            }
        }
        let mut live_to_image = BTreeMap::new();
        for (idx, &live_id) in visited.iter().enumerate() {
            let img_id = CodeId::try_from(idx).expect("fewer than u32::MAX functions in slice");
            live_to_image.insert(live_id, img_id);
        }
        let mut functions = BTreeMap::new();
        let mut image_owners = BTreeMap::new();
        for (&live_id, &img_id) in &live_to_image {
            if let Some(func) = self.program.functions.get(&live_id) {
                functions.insert(img_id, remap_function(func, &live_to_image));
            }
            if let Some(owner) = self.code_owner.get(&live_id) {
                image_owners.insert(img_id, owner.clone());
            }
        }
        let mut op_spans = BTreeMap::new();
        for (&(live_code, idx), &span) in &self.program.op_spans {
            if let Some(&img_code) = live_to_image.get(&live_code) {
                op_spans.insert((img_code, idx), span);
            }
        }
        let globals = self
            .program
            .globals
            .iter()
            .filter(|g| modules.contains(&g.name.module))
            .map(|g| Global {
                name: g.name.clone(),
                initializer: live_to_image[&g.initializer],
            })
            .collect();
        (
            Program {
                functions,
                globals,
                op_spans,
            },
            live_to_image,
            image_owners,
        )
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

fn infer_code_owners(program: &Program) -> BTreeMap<CodeId, String> {
    let mut owners = BTreeMap::new();
    for global in &program.globals {
        let mut work = vec![global.initializer];
        while let Some(id) = work.pop() {
            if owners.contains_key(&id) {
                continue;
            }
            owners.insert(id, global.name.module.clone());
            if let Some(func) = program.functions.get(&id) {
                let mut refs = BTreeSet::new();
                collect_block_codes(&func.body, &mut refs);
                work.extend(refs);
            }
        }
    }
    owners
}

pub(crate) fn collect_block_codes(block: &Block, out: &mut BTreeSet<CodeId>) {
    for inst in &block.instructions {
        let Instruction::Let { value, .. } = inst else {
            continue;
        };
        match value {
            Operation::Closure { code, .. }
            | Operation::Task { code, .. }
            | Operation::Call { code, .. }
            | Operation::Within { code, .. }
            | Operation::WithOwnerEvidence { code, .. }
            | Operation::Loop { code, .. }
            | Operation::FunctionBoundary { code, .. }
            | Operation::GenerateStrict { code, .. }
            | Operation::GenerateLazy { code, .. } => {
                out.insert(*code);
            }
            _ => {}
        }
    }
    if let Exit::Branch { then_, else_, .. } = &block.exit {
        collect_block_codes(then_, out);
        collect_block_codes(else_, out);
    }
}

pub(crate) fn collect_block_globals(block: &Block, self_module: &str, out: &mut BTreeSet<String>) {
    for inst in &block.instructions {
        if let Instruction::Let {
            value: Operation::Global(name),
            ..
        } = inst
            && name.module != self_module
        {
            out.insert(name.module.clone());
        }
    }
    if let Exit::Branch { then_, else_, .. } = &block.exit {
        collect_block_globals(then_, self_module, out);
        collect_block_globals(else_, self_module, out);
    }
}
