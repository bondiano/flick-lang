//! Literal allocation and representation tests for the counted memory VM.

use flick_core::ir::Lit;
use flick_core::ir::memory::{Block, Exit, Function, Instruction, Operation};
use flick_vm::tier0::{Heap, Value, execute};

fn bind(binding: u32, value: Operation) -> Instruction {
    Instruction::Let { binding, value }
}

fn function(params: Vec<u32>, instructions: Vec<Instruction>, result: u32) -> Function {
    Function {
        params,
        body: Block {
            instructions,
            exit: Exit::Return(result),
        },
    }
}

#[test]
fn every_literal_shape_runs_through_the_shared_operation() {
    for literal in [
        Lit::Unit,
        Lit::Bool(true),
        Lit::Int(-42),
        Lit::Int8(-128),
        Lit::Int16(-32_768),
        Lit::Int64(i64::MIN),
        Lit::UInt8(255),
        Lit::UInt16(65_535),
        Lit::UInt32(u32::MAX),
        Lit::UInt64(u64::MAX),
        Lit::Float(1.5),
        Lit::Float32(1.5),
        Lit::String("text".into()),
        Lit::BigInt("0xffffffffffffffffffffffff".into()),
    ] {
        let mut heap = Heap::new();
        let f = function(
            vec![],
            vec![bind(0, Operation::Literal(literal.clone()))],
            0,
        );
        let result = execute(&mut heap, &f, vec![]).expect("literal operation");
        let allocated = match literal {
            Lit::Unit => {
                assert_eq!(result, Value::Unit);
                0
            }
            Lit::Bool(value) => {
                assert_eq!(result, Value::Bool(value));
                0
            }
            Lit::Int(value) => {
                assert_eq!(result, Value::Int(value));
                0
            }
            Lit::Int8(value) => {
                assert_eq!(result, Value::Int8(value));
                0
            }
            Lit::Int16(value) => {
                assert_eq!(result, Value::Int16(value));
                0
            }
            Lit::Int64(value) => {
                assert_eq!(result, Value::Int64(value));
                0
            }
            Lit::UInt8(value) => {
                assert_eq!(result, Value::UInt8(value));
                0
            }
            Lit::UInt16(value) => {
                assert_eq!(result, Value::UInt16(value));
                0
            }
            Lit::UInt32(value) => {
                assert_eq!(result, Value::UInt32(value));
                0
            }
            Lit::UInt64(value) => {
                assert_eq!(result, Value::UInt64(value));
                0
            }
            Lit::Float(value) => {
                assert_eq!(result, Value::Float(value));
                0
            }
            Lit::Float32(value) => {
                assert_eq!(result, Value::Float32(value));
                0
            }
            Lit::String(ref value) => {
                assert_eq!(heap.string(&result), Ok(value.as_str()));
                1
            }
            Lit::BigInt(ref value) => {
                assert_eq!(heap.bigint(&result), Ok(value.as_str()));
                1
            }
        };
        assert_eq!(heap.stats().allocations, allocated);
        heap.drop_value(result).expect("explicit literal release");
        assert_eq!(heap.finish().expect("balanced literal").frees, allocated);
    }
}
