//! Entry point and block-level preparation from `flick-ir::memory::Program` to tier-1 bytecode.

use std::collections::BTreeMap;

use flick_core::ir::memory::{
    self, Block, CodeId, Exit, HostPrimitive, Instruction, Intrinsic, Local, MemorySpan, Name,
    Program,
};

use super::opcode::{CompiledFunction, Instr, PrepareError, Prepared, PreparedOp};
use super::prepare_fuse;

pub(super) struct Interner<T> {
    pub(super) entries: BTreeMap<T, usize>,
    pub(super) values: Vec<T>,
}

impl<T> Default for Interner<T> {
    fn default() -> Self {
        Self {
            entries: BTreeMap::new(),
            values: Vec::new(),
        }
    }
}

impl<T: Ord + Clone> Interner<T> {
    pub(super) fn intern(&mut self, value: T) -> usize {
        *self.entries.entry(value.clone()).or_insert_with(|| {
            let index = self.values.len();
            self.values.push(value);
            index
        })
    }
}

#[derive(Default)]
pub(super) struct Slots {
    pub(super) count: usize,
}

impl Slots {
    pub(super) fn reserve(&mut self) -> usize {
        let slot = self.count;
        self.count += 1;
        slot
    }
}

#[derive(Clone, Default)]
pub(super) struct Tokens(BTreeMap<Local, Vec<usize>>);

impl Tokens {
    pub(super) fn bind(&mut self, local: Local, slot: usize) {
        self.0.entry(local).or_default().push(slot);
    }

    pub(super) fn peek(&self, local: Local) -> usize {
        *self.0[&local].last().expect("validated live local")
    }

    pub(super) fn take(&mut self, local: Local) -> usize {
        self.0
            .get_mut(&local)
            .and_then(Vec::pop)
            .expect("validated owned token")
    }

    pub(super) fn take_fields(&mut self, fields: &[(String, Local)]) -> Vec<(String, usize)> {
        fields
            .iter()
            .map(|(name, local)| (name.clone(), self.take(*local)))
            .collect()
    }

    pub(super) fn take_all(&mut self, locals: &[Local]) -> Vec<usize> {
        locals.iter().map(|&local| self.take(local)).collect()
    }
}

pub(super) struct Lowering<'a> {
    pub(super) program: &'a Program,
    pub(super) initializers: BTreeMap<Name, CodeId>,
    pub(super) codes: Interner<CodeId>,
    pub(super) globals: Interner<Name>,
    pub(super) tags: Interner<String>,
    pub(super) dictionary_fields: Interner<String>,
    pub(super) loops: Interner<String>,
    pub(super) operations: Interner<(String, String)>,
}

/// Validates `program` and prepares a tier-1 register bytecode image for `entry`.
///
/// Only functions reachable from `entry` are translated. Any reachable
/// operation not supported on tier 1 fails immediately with [`PrepareError`].
///
/// # Errors
/// Returns [`PrepareError::Invalid`] if memory-IR validation fails,
/// [`PrepareError::UnknownEntry`] if `entry` is not a global in `program`, or
/// [`PrepareError::Unsupported`] if a reachable operation is unsupported.
pub fn prepare(program: &Program, entry: &Name) -> Result<Prepared, PrepareError> {
    prepare_inner(program, entry, None)
}

/// Validates `program` and prepares a tier-1 register bytecode image for `entry`
/// together with a root service layer global `root` (such as `Host.live`).
///
/// # Errors
/// Returns [`PrepareError::Invalid`] if memory-IR validation fails,
/// [`PrepareError::UnknownEntry`] if `entry` or `root` is missing, or
/// [`PrepareError::Unsupported`] if a reachable operation is unsupported.
pub fn prepare_rooted(
    program: &Program,
    entry: &Name,
    root: &Name,
) -> Result<Prepared, PrepareError> {
    prepare_inner(program, entry, Some(root))
}

fn prepare_inner(
    program: &Program,
    entry: &Name,
    root: Option<&Name>,
) -> Result<Prepared, PrepareError> {
    memory::validate_program(program).map_err(PrepareError::Invalid)?;
    let initializers: BTreeMap<_, _> = program
        .globals
        .iter()
        .map(|global| (global.name.clone(), global.initializer))
        .collect();
    if !initializers.contains_key(entry) {
        return Err(PrepareError::UnknownEntry(entry.clone()));
    }
    if let Some(root_name) = root
        && !initializers.contains_key(root_name)
    {
        return Err(PrepareError::UnknownEntry(root_name.clone()));
    }
    let mut lowering = Lowering {
        program,
        initializers,
        codes: Interner::default(),
        globals: Interner::default(),
        tags: Interner::default(),
        dictionary_fields: Interner::default(),
        loops: Interner::default(),
        operations: Interner::default(),
    };
    let entry_id = lowering.intern_global(entry);
    let root_id = root.map(|root_name| lowering.intern_global(root_name));
    let mut functions = Vec::new();
    while let Some(&code) = lowering.codes.values.get(functions.len()) {
        functions.push(lowering.prepare_function(code, &program.functions[&code])?);
    }
    let globals: Vec<usize> = lowering
        .globals
        .values
        .iter()
        .map(|name| lowering.codes.entries[&lowering.initializers[name]])
        .collect();
    let operations = lowering
        .operations
        .values
        .into_iter()
        .map(|(effect, op)| {
            let console_fallback = if effect == "Host.Console" {
                match op.as_str() {
                    "stdout_line" => Some(Intrinsic::PrintLine),
                    "stdout_write" => Some(Intrinsic::Host(HostPrimitive::StdoutWrite)),
                    "stderr_line" => Some(Intrinsic::Host(HostPrimitive::StderrLine)),
                    _ => None,
                }
            } else {
                None
            };
            PreparedOp {
                effect,
                op,
                console_fallback,
            }
        })
        .collect();
    Ok(Prepared {
        functions,
        ir_codes: lowering.codes.values,
        code_indices: lowering.codes.entries,
        globals,
        global_names: lowering.globals.values,
        entry: entry_id,
        root: root_id,
        tags: lowering.tags.values,
        dictionary_fields: lowering.dictionary_fields.values,
        loops: lowering.loops.values,
        operations,
    })
}

impl Lowering<'_> {
    pub(super) fn intern_global(&mut self, name: &Name) -> usize {
        self.codes.intern(self.initializers[name]);
        self.globals.intern(name.clone())
    }

    pub(super) fn intern_operation(&mut self, effect: &str, op: &str) -> usize {
        self.operations.intern((effect.to_owned(), op.to_owned()))
    }

    fn prepare_function(
        &mut self,
        code: CodeId,
        function: &memory::Function,
    ) -> Result<CompiledFunction, PrepareError> {
        let mut instructions = Vec::new();
        let mut spans = Vec::new();
        let mut slots = Slots::default();
        let mut tokens = Tokens::default();
        let params: Vec<usize> = function
            .params
            .iter()
            .map(|&local| {
                let slot = slots.reserve();
                tokens.bind(local, slot);
                slot
            })
            .collect();
        let mut op_index = 0usize;
        self.prepare_block(
            code,
            &function.body,
            tokens,
            &mut slots,
            &mut op_index,
            &mut instructions,
            &mut spans,
        )?;
        let (instructions, spans) = prepare_fuse::fuse(instructions, spans);
        Ok(CompiledFunction {
            params,
            slots: slots.count,
            instructions,
            spans,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_block(
        &mut self,
        code: CodeId,
        block: &Block,
        mut tokens: Tokens,
        slots: &mut Slots,
        op_index: &mut usize,
        instructions: &mut Vec<Instr>,
        spans: &mut Vec<Option<MemorySpan>>,
    ) -> Result<(), PrepareError> {
        for instruction in &block.instructions {
            let current_op = *op_index;
            *op_index += 1;
            let (instr, instr_span) = match instruction {
                Instruction::Let { binding, value } => {
                    let dst = slots.reserve();
                    let span = self.program.op_spans.get(&(code, current_op)).copied();
                    let instr = self.translate_operation(code, value, dst, span, &mut tokens)?;
                    tokens.bind(*binding, dst);
                    (instr, span)
                }
                Instruction::Dup(local) => {
                    let src = tokens.peek(*local);
                    let dst = slots.reserve();
                    tokens.bind(*local, dst);
                    (Instr::Dup { src, dst }, None)
                }
                Instruction::Drop(local) => (
                    Instr::Drop {
                        src: tokens.take(*local),
                    },
                    None,
                ),
            };
            instructions.push(instr);
            spans.push(instr_span);
        }
        match &block.exit {
            Exit::Return(value) => {
                instructions.push(Instr::Return {
                    value: tokens.take(*value),
                });
                spans.push(None);
            }
            Exit::Branch {
                condition,
                then_,
                else_,
            } => {
                let condition = tokens.take(*condition);
                let branch_pc = instructions.len();
                let then_pc = branch_pc + 1;
                instructions.push(Instr::Branch {
                    condition,
                    then_pc,
                    else_pc: then_pc,
                });
                spans.push(None);
                self.prepare_block(
                    code,
                    then_,
                    tokens.clone(),
                    slots,
                    op_index,
                    instructions,
                    spans,
                )?;
                let else_pc = instructions.len();
                instructions[branch_pc] = Instr::Branch {
                    condition,
                    then_pc,
                    else_pc,
                };
                self.prepare_block(code, else_, tokens, slots, op_index, instructions, spans)?;
            }
        }
        Ok(())
    }
}
