//! Per-operation IR-to-bytecode dispatch and core/primitive/control translators.
//!
//! Each supported `flick_ir::memory::Operation` has its own dedicated
//! `translate_*` function so formal translation-correctness proofs (`#980`)
//! can be stated directly per IR operation. Data, aggregate, dictionary,
//! reuse, and variable/loop translators live in [`super::prepare_ops_data`].

use flick_core::ir::memory::{
    CodeId, HostPrimitive, Intrinsic, Local, MemorySpan, Name, Operation,
};
use flick_core::ir::{BinOp, Lit};
use flick_core::numeric::Num;

use super::opcode::{Immediate, Instr, PrepareError};
use super::prepare::{Lowering, Tokens};
use crate::stdlib;

impl Lowering<'_> {
    #[allow(clippy::too_many_lines)]
    pub(super) fn translate_operation(
        &mut self,
        code: CodeId,
        op: &Operation,
        dst: usize,
        span: Option<MemorySpan>,
        tokens: &mut Tokens,
    ) -> Result<Instr, PrepareError> {
        match op {
            Operation::Literal(lit) => Ok(Self::translate_literal(lit, dst)),
            Operation::Construct {
                tag,
                fields,
                rep,
                layout,
            } => Ok(self.translate_construct(tag, fields, *rep, layout.as_ref(), dst, tokens)),
            Operation::ConstructWithDicts {
                tag,
                fields,
                dicts,
                rep,
                layout,
            } => Ok(self.translate_construct_with_dicts(
                tag,
                fields,
                dicts,
                *rep,
                layout.as_ref(),
                dst,
                tokens,
            )),
            Operation::Project { object, field, .. } => {
                Ok(Self::translate_project(*object, *field, dst, tokens))
            }
            Operation::Record { fields } => Ok(Self::translate_record(fields, dst, tokens)),
            Operation::Field { object, field } => {
                Ok(Self::translate_field(*object, field, dst, tokens))
            }
            Operation::Match {
                object, pattern, ..
            } => Ok(Self::translate_match(*object, pattern, dst, tokens)),
            Operation::Update { object, fields } => {
                Ok(Self::translate_update(*object, fields, dst, tokens))
            }
            Operation::Closure {
                code: fn_code,
                captures,
            } => Ok(self.translate_closure(*fn_code, captures, dst, tokens)),
            Operation::Apply { function, argument } => {
                Ok(Self::translate_apply(*function, *argument, dst, tokens))
            }
            Operation::Call {
                code: fn_code,
                args,
            } => Ok(self.translate_call(*fn_code, args, dst, tokens)),
            Operation::Global(name) => Ok(self.translate_global(name, dst)),
            Operation::Binary { op, left, right } => {
                Ok(Self::translate_binary(*op, *left, *right, dst, tokens))
            }
            Operation::Not(value) => Ok(Self::translate_not(*value, dst, tokens)),
            Operation::Neg { num, value } => Ok(Self::translate_neg(*num, *value, dst, tokens)),
            Operation::Intrinsic { kind, args } => {
                Self::translate_intrinsic(code, *kind, args, dst, span, tokens)
            }
            Operation::DropReuse(object) => Ok(Self::translate_drop_reuse(*object, dst, tokens)),
            Operation::Reuse {
                object,
                tag,
                fields,
                rep,
                layout,
            } => self.translate_reuse(
                code,
                *object,
                tag,
                fields,
                *rep,
                layout.as_ref(),
                dst,
                span,
                tokens,
            ),
            Operation::ReuseWithDicts {
                object,
                tag,
                fields,
                dicts,
                rep,
                layout,
            } => self.translate_reuse_with_dicts(
                code,
                *object,
                tag,
                fields,
                dicts,
                *rep,
                layout.as_ref(),
                dst,
                span,
                tokens,
            ),
            Operation::ReuseRecord { object, fields } => {
                Ok(Self::translate_reuse_record(*object, fields, dst, tokens))
            }
            Operation::BoxAggregate { value, layout } => Ok(Self::translate_box_aggregate(
                *value,
                layout.as_ref(),
                dst,
                tokens,
            )),
            Operation::UnboxBorrow { object } => {
                Ok(Self::translate_unbox_borrow(*object, dst, tokens))
            }
            Operation::UnboxConsume { object } => {
                Ok(Self::translate_unbox_consume(*object, dst, tokens))
            }
            Operation::Dictionary { entries } => {
                Ok(self.translate_dictionary(entries, dst, tokens))
            }
            Operation::Method { dictionary, field } => {
                Ok(self.translate_method(*dictionary, field, dst, tokens))
            }
            Operation::Structure { object, expected } => {
                Ok(Self::translate_structure(*object, *expected, dst, tokens))
            }
            Operation::VarMake(value) => Ok(Self::translate_var_make(*value, dst, tokens)),
            Operation::VarGet(var) => Ok(Self::translate_var_get(*var, dst, tokens)),
            Operation::VarSet { var, value } => {
                Ok(Self::translate_var_set(*var, *value, dst, tokens))
            }
            Operation::Loop {
                id,
                code: body_code,
                args,
            } => Ok(self.translate_loop(id, *body_code, args, dst, tokens)),
            Operation::LoopExit { id, value } => Ok(self.translate_loop_exit(id, *value, tokens)),
            Operation::FunctionBoundary {
                code: fn_code,
                args,
            } => Ok(self.translate_function_boundary(*fn_code, args, dst, tokens)),
            Operation::UnwrapOption(option) => {
                Ok(Self::translate_unwrap_option(*option, dst, tokens))
            }
            Operation::Label(value) => Ok(Self::translate_label(value, dst)),
            Operation::ServiceTag(key) => Ok(Self::translate_service_tag(key, dst)),
            Operation::ReadOwnerEvidence => Ok(Self::translate_read_owner_evidence(dst)),
            Operation::RecoveryResult { selected } => {
                Ok(Self::translate_recovery_result(*selected, dst, tokens))
            }
            Operation::Perform {
                effect,
                op: op_name,
                dicts,
                args,
            } => Ok(self.translate_perform(effect, op_name, dicts, args, dst, tokens)),
            Operation::Handle {
                body,
                effects,
                ret,
                errors,
            } => Ok(self.translate_handle(*body, effects, *ret, *errors, dst, tokens)),
            Operation::Task {
                code: fn_code,
                captures,
            } => Ok(self.translate_task(*fn_code, captures, dst, tokens)),
            Operation::Await(task) => Ok(Self::translate_await(*task, dst, tokens)),
            Operation::Within {
                owner,
                code: fn_code,
                args,
            } => Ok(self.translate_within(*owner, *fn_code, args, dst, tokens)),
            Operation::WithOwnerEvidence {
                owner,
                code: fn_code,
                args,
            } => Ok(self.translate_with_owner_evidence(*owner, *fn_code, args, dst, tokens)),
            Operation::GenerateStrict {
                code: fn_code,
                args,
            } => Ok(self.translate_generate_strict(*fn_code, args, dst, tokens)),
            Operation::GenerateLazy {
                code: fn_code,
                captures,
                limit,
                ..
            } => Ok(self.translate_generate_lazy(*fn_code, captures, *limit, dst, tokens)),
            Operation::Yield(value) => Ok(Self::translate_yield(*value, dst, tokens)),
            Operation::SeqOpen(source) => Ok(Self::translate_seq_open(*source, dst, tokens)),
            Operation::SeqNext(iterator) => Ok(Self::translate_seq_next(*iterator, dst, tokens)),
            Operation::ForeignCall { spec, .. } => Err(unsupported(
                code,
                format!("ForeignCall({})", spec.name),
                span,
            )),
        }
    }

    pub(super) fn translate_literal(lit: &Lit, dst: usize) -> Instr {
        match lit {
            Lit::Unit => Instr::Literal {
                dst,
                value: Immediate::Unit,
            },
            Lit::Bool(v) => Instr::Literal {
                dst,
                value: Immediate::Bool(*v),
            },
            Lit::Int(v) => Instr::Literal {
                dst,
                value: Immediate::Int(*v),
            },
            Lit::Int8(v) => Instr::Literal {
                dst,
                value: Immediate::Int8(*v),
            },
            Lit::Int16(v) => Instr::Literal {
                dst,
                value: Immediate::Int16(*v),
            },
            Lit::Int64(v) => Instr::Literal {
                dst,
                value: Immediate::Int64(*v),
            },
            Lit::UInt8(v) => Instr::Literal {
                dst,
                value: Immediate::UInt8(*v),
            },
            Lit::UInt16(v) => Instr::Literal {
                dst,
                value: Immediate::UInt16(*v),
            },
            Lit::UInt32(v) => Instr::Literal {
                dst,
                value: Immediate::UInt32(*v),
            },
            Lit::UInt64(v) => Instr::Literal {
                dst,
                value: Immediate::UInt64(*v),
            },
            Lit::Float(v) => Instr::Literal {
                dst,
                value: Immediate::Float(*v),
            },
            Lit::Float32(v) => Instr::Literal {
                dst,
                value: Immediate::Float32(*v),
            },
            Lit::String(v) => Instr::String {
                dst,
                value: v.clone(),
            },
            Lit::BigInt(v) => Instr::BigInt {
                dst,
                value: v.clone(),
            },
        }
    }

    pub(super) fn translate_closure(
        &mut self,
        fn_code: CodeId,
        captures: &[Local],
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        Instr::Closure {
            dst,
            code: self.codes.intern(fn_code),
            captures: tokens.take_all(captures),
        }
    }

    pub(super) fn translate_apply(
        function: Local,
        argument: Local,
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        Instr::Apply {
            dst,
            function: tokens.take(function),
            argument: tokens.take(argument),
        }
    }

    pub(super) fn translate_call(
        &mut self,
        fn_code: CodeId,
        args: &[Local],
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        Instr::Call {
            dst,
            code: self.codes.intern(fn_code),
            args: tokens.take_all(args),
        }
    }

    pub(super) fn translate_global(&mut self, name: &Name, dst: usize) -> Instr {
        Instr::Global {
            dst,
            global: self.intern_global(name),
        }
    }

    pub(super) fn translate_binary(
        op: BinOp,
        left: Local,
        right: Local,
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        Instr::Binary {
            dst,
            op,
            left: tokens.take(left),
            right: tokens.take(right),
        }
    }

    pub(super) fn translate_not(value: Local, dst: usize, tokens: &mut Tokens) -> Instr {
        Instr::Not {
            dst,
            src: tokens.take(value),
        }
    }

    pub(super) fn translate_neg(num: Num, value: Local, dst: usize, tokens: &mut Tokens) -> Instr {
        Instr::Neg {
            dst,
            num,
            src: tokens.take(value),
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn translate_intrinsic(
        code: CodeId,
        kind: Intrinsic,
        args: &[Local],
        dst: usize,
        span: Option<MemorySpan>,
        tokens: &mut Tokens,
    ) -> Result<Instr, PrepareError> {
        if stdlib::is_metered_primitive(kind) {
            return Ok(Instr::Primitive {
                dst,
                kind,
                args: tokens.take_all(args),
            });
        }
        match (kind, args) {
            (Intrinsic::StringStartsWith | Intrinsic::StringIncludes, [needle, subject]) => {
                Ok(Instr::StringPredicate {
                    dst,
                    kind,
                    needle: tokens.take(*needle),
                    subject: tokens.take(*subject),
                })
            }
            (
                Intrinsic::PrintLine
                | Intrinsic::Show
                | Intrinsic::Host(
                    HostPrimitive::StderrLine
                    | HostPrimitive::StdoutWrite
                    | HostPrimitive::Env
                    | HostPrimitive::Vars
                    | HostPrimitive::Args
                    | HostPrimitive::CurrentTimeMillis
                    | HostPrimitive::CurrentTimeNanos
                    | HostPrimitive::MonotonicTimeNanos
                    | HostPrimitive::RandomNext,
                ),
                [src],
            ) => Ok(Instr::HostIntrinsic {
                dst,
                kind,
                src: tokens.take(*src),
            }),
            (Intrinsic::HostService(op), _) => Ok(Instr::HostService {
                dst,
                op,
                args: tokens.take_all(args),
            }),
            (Intrinsic::Cli(op), _) => Ok(Instr::Cli {
                dst,
                op,
                args: tokens.take_all(args),
            }),
            (Intrinsic::MatchFail, []) => Ok(Instr::MatchFail),
            (Intrinsic::FromResult, [src]) => Ok(Instr::FromResult {
                dst,
                src: tokens.take(*src),
            }),
            (Intrinsic::Raise, [src]) => Ok(Instr::Raise {
                src: tokens.take(*src),
            }),
            (Intrinsic::EffectDie, [src]) => Ok(Instr::Die {
                src: tokens.take(*src),
            }),
            (Intrinsic::FailureRethrow, [src]) => Ok(Instr::FailureRethrow {
                src: tokens.take(*src),
            }),
            (Intrinsic::Try, [body, selector]) => Ok(Instr::Try {
                dst,
                body: tokens.take(*body),
                selector: tokens.take(*selector),
            }),
            (Intrinsic::ScopeRun, [body]) => Ok(Self::translate_scope_run(*body, dst, tokens)),
            (Intrinsic::ScopeCurrent, [unit]) => {
                Ok(Self::translate_scope_current(*unit, dst, tokens))
            }
            (Intrinsic::ScopeAddFinalizer, [task]) => {
                Ok(Self::translate_scope_add_finalizer(*task, dst, tokens))
            }
            (Intrinsic::ScopeAddFinalizerExit, [func]) => {
                Ok(Self::translate_scope_add_finalizer_exit(*func, dst, tokens))
            }
            (Intrinsic::ScopeAddClose, [func]) => {
                Ok(Self::translate_scope_add_close(*func, dst, tokens))
            }
            (Intrinsic::AcquireRelease, [acquire, release]) => Ok(Self::translate_acquire_release(
                *acquire, *release, dst, tokens,
            )),
            (Intrinsic::Bracket, [acquire, use_fn, release]) => Ok(Self::translate_bracket(
                *acquire, *use_fn, *release, dst, tokens,
            )),
            (Intrinsic::Uninterruptible, [task]) => {
                Ok(Self::translate_uninterruptible(*task, dst, tokens))
            }
            (Intrinsic::RefMake, [src]) => Ok(Self::translate_ref_make(*src, dst, tokens)),
            (Intrinsic::RefGet, [cell]) => Ok(Self::translate_ref_get(*cell, dst, tokens)),
            (Intrinsic::RefSet, [cell, value]) => {
                Ok(Self::translate_ref_set(*cell, *value, dst, tokens))
            }
            (Intrinsic::RefModify, [cell, func]) => {
                Ok(Self::translate_ref_modify(*cell, *func, dst, tokens))
            }
            (Intrinsic::RefSame, [left, right]) => {
                Ok(Self::translate_ref_same(*left, *right, dst, tokens))
            }
            (Intrinsic::ArenaMake, [unit]) => Ok(Self::translate_arena_make(*unit, dst, tokens)),
            (Intrinsic::ArenaAlloc, [arena, value]) => {
                Ok(Self::translate_arena_alloc(*arena, *value, dst, tokens))
            }
            (Intrinsic::ArenaGet, [view]) => Ok(Self::translate_arena_get(*view, dst, tokens)),
            (Intrinsic::LazyMake, [task]) => {
                Ok(Self::translate_lazy_make(*task, false, dst, tokens))
            }
            (Intrinsic::LazyMakeLocal, [task]) => {
                Ok(Self::translate_lazy_make(*task, true, dst, tokens))
            }
            (Intrinsic::LazyForce, [lazy]) => Ok(Self::translate_lazy_force(*lazy, dst, tokens)),
            (Intrinsic::SeqOfList, [list]) => Ok(Self::translate_seq_of_list(*list, dst, tokens)),
            (Intrinsic::SeqToList, [seq]) => Ok(Self::translate_seq_to_list(*seq, dst, tokens)),
            (Intrinsic::EffectExit, [task]) => Ok(Self::translate_effect_exit(*task, dst, tokens)),
            (Intrinsic::EffectCatchDefect, [handler, task]) => Ok(
                Self::translate_effect_catch_defect(*handler, *task, dst, tokens),
            ),
            (Intrinsic::LayerSucceed, [tag, implementation]) => Ok(Self::translate_layer_succeed(
                *tag,
                *implementation,
                dst,
                tokens,
            )),
            (Intrinsic::LayerMerge, [left, right]) => {
                Ok(Self::translate_layer_merge(*left, *right, dst, tokens))
            }
            (Intrinsic::LayerRun, [layer, body]) => {
                Ok(Self::translate_layer_run(*layer, *body, dst, tokens))
            }
            (Intrinsic::Async(op), _) => Ok(Self::translate_async_op(op, args, dst, tokens)),
            (Intrinsic::Layer(op), _) => Ok(Self::translate_layer_op(op, args, dst, tokens)),
            _ => Err(unsupported(code, format!("Intrinsic::{kind:?}"), span)),
        }
    }
}

pub(super) fn unsupported(
    code: CodeId,
    operation: impl Into<String>,
    span: Option<MemorySpan>,
) -> PrepareError {
    PrepareError::Unsupported {
        code,
        operation: operation.into(),
        span,
    }
}
