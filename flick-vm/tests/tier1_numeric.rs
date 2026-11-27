//! Numeric boundary and defect integration tests for `flick_vm::tier1`.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use flick_core::diagnostics::Severity;
use flick_core::ir::memory::{
    Block, Exit, FixedWidthIntrinsic, Function, Global, Instruction, Intrinsic, Name, Operation,
    Program,
};
use flick_core::ir::{BinOp, Lit};
use flick_core::numeric::{Arith, Num};
use flick_core::opt::perceus;
use flick_core::source::SourceMap;
use flick_core::{build, project};
use flick_vm::tier0::{DefectCode, Heap, ProgramOutcome, Reason, Value};
use flick_vm::tier1;

fn main_name() -> Name {
    Name::new("Test", "main")
}

fn let_op(binding: u32, value: Operation) -> Instruction {
    Instruction::Let { binding, value }
}

fn single_op_program(ops: Vec<Instruction>, ret: u32) -> Program {
    let raw = Function {
        params: vec![],
        body: Block {
            instructions: ops,
            exit: Exit::Return(ret),
        },
    };
    Program {
        functions: BTreeMap::from([(0, perceus::insert(&raw).expect("valid ownership"))]),
        globals: vec![Global {
            name: main_name(),
            initializer: 0,
        }],
        op_spans: BTreeMap::new(),
    }
}

fn copy_imm(v: &Value) -> Value {
    match *v {
        Value::Unit => Value::Unit,
        Value::Bool(x) => Value::Bool(x),
        Value::Int(x) => Value::Int(x),
        Value::Int8(x) => Value::Int8(x),
        Value::Int16(x) => Value::Int16(x),
        Value::Int64(x) => Value::Int64(x),
        Value::UInt8(x) => Value::UInt8(x),
        Value::UInt16(x) => Value::UInt16(x),
        Value::UInt32(x) => Value::UInt32(x),
        Value::UInt64(x) => Value::UInt64(x),
        Value::Float(x) => Value::Float(x),
        Value::Float32(x) => Value::Float32(x),
        _ => panic!("unexpected non-immediate value: {v:?}"),
    }
}

fn run_imm(program: &Program) -> Value {
    let mut heap = Heap::new();
    let exec = tier1::execute_program(&mut heap, program, &main_name(), vec![], 10_000)
        .expect("tier1 prepares")
        .expect("tier1 executes");
    let value = copy_imm(&exec.value);
    heap.drop_value(exec.value).expect("drop value");
    heap.finish().expect("zero leaks");
    value
}

fn eval_bin(op: BinOp, left: Lit, right: Lit) -> Value {
    run_imm(&single_op_program(
        vec![
            let_op(0, Operation::Literal(left)),
            let_op(1, Operation::Literal(right)),
            let_op(
                2,
                Operation::Binary {
                    op,
                    left: 0,
                    right: 1,
                },
            ),
        ],
        2,
    ))
}

fn eval_neg(num: Num, lit: Lit) -> Value {
    run_imm(&single_op_program(
        vec![
            let_op(0, Operation::Literal(lit)),
            let_op(1, Operation::Neg { num, value: 0 }),
        ],
        1,
    ))
}

fn eval_intrinsic(kind: Intrinsic, args: Vec<Lit>) -> Value {
    let mut ops = Vec::new();
    let mut locals = Vec::new();
    for (i, lit) in args.into_iter().enumerate() {
        let local = u32::try_from(i).expect("small arity");
        ops.push(let_op(local, Operation::Literal(lit)));
        locals.push(local);
    }
    let out = u32::try_from(locals.len()).expect("small arity");
    ops.push(let_op(out, Operation::Intrinsic { kind, args: locals }));
    run_imm(&single_op_program(ops, out))
}

fn eval_bigint_bin(op: BinOp, left: &str, right: &str) -> String {
    let program = single_op_program(
        vec![
            let_op(0, Operation::Literal(Lit::BigInt(left.into()))),
            let_op(1, Operation::Literal(Lit::BigInt(right.into()))),
            let_op(
                2,
                Operation::Binary {
                    op,
                    left: 0,
                    right: 1,
                },
            ),
        ],
        2,
    );
    let mut heap = Heap::new();
    let exec = tier1::execute_program(&mut heap, &program, &main_name(), vec![], 10_000)
        .expect("tier1 prepares")
        .expect("tier1 executes");
    let text = heap.bigint(&exec.value).expect("BigInt").to_owned();
    heap.drop_value(exec.value).expect("drop bigint");
    heap.finish().expect("zero leaks");
    text
}

struct MemHost<'a>(&'a str);

impl project::Host for MemHost<'_> {
    fn read(&self, p: &Path) -> io::Result<Vec<u8>> {
        if p == Path::new("/Main.fl") {
            Ok(self.0.as_bytes().to_vec())
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    }

    fn display(&self, p: &Path) -> String {
        p.display().to_string()
    }
}

fn lower_source(src: &str, entry: &Name) -> (Program, SourceMap) {
    let mut sources = SourceMap::default();
    let (diags, comp) = project::compile(
        &MemHost(src),
        &mut sources,
        None,
        &[PathBuf::from("/Main.fl")],
    );
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "compile failed: {diags:?}"
    );
    let modules: Vec<_> = comp
        .modules
        .iter()
        .map(|m| (*m.ir).clone())
        .chain(build::builtin_modules().iter().cloned())
        .collect();
    let decls: Vec<_> = comp
        .modules
        .iter()
        .map(|m| m.declarations.as_ref())
        .chain(build::builtin_declarations().iter().map(AsRef::as_ref))
        .collect();
    let program = perceus::lower_roots_with_declarations(modules.iter(), decls, &[entry])
        .expect("perceus lower");
    (program, sources)
}

#[test]
fn signed_and_unsigned_fixed_width_wrapping_boundaries() {
    let add = |n| BinOp::Arith(Arith::Add, n);
    let sub = |n| BinOp::Arith(Arith::Sub, n);
    let mul = |n| BinOp::Arith(Arith::Mul, n);
    let bin_cases = [
        (
            add(Num::Int8),
            Lit::Int8(127),
            Lit::Int8(1),
            Value::Int8(-128),
        ),
        (
            sub(Num::Int8),
            Lit::Int8(-128),
            Lit::Int8(1),
            Value::Int8(127),
        ),
        (
            add(Num::Int16),
            Lit::Int16(i16::MAX),
            Lit::Int16(1),
            Value::Int16(i16::MIN),
        ),
        (
            add(Num::Int),
            Lit::Int(i32::MAX),
            Lit::Int(1),
            Value::Int(i32::MIN),
        ),
        (
            mul(Num::Int),
            Lit::Int(65_536),
            Lit::Int(65_536),
            Value::Int(0),
        ),
        (
            add(Num::Int64),
            Lit::Int64(i64::MAX),
            Lit::Int64(1),
            Value::Int64(i64::MIN),
        ),
        (
            add(Num::UInt8),
            Lit::UInt8(255),
            Lit::UInt8(1),
            Value::UInt8(0),
        ),
        (
            sub(Num::UInt8),
            Lit::UInt8(0),
            Lit::UInt8(1),
            Value::UInt8(255),
        ),
        (
            add(Num::UInt16),
            Lit::UInt16(u16::MAX),
            Lit::UInt16(1),
            Value::UInt16(0),
        ),
        (
            mul(Num::UInt32),
            Lit::UInt32(u32::MAX),
            Lit::UInt32(u32::MAX),
            Value::UInt32(1),
        ),
        (
            add(Num::UInt64),
            Lit::UInt64(u64::MAX),
            Lit::UInt64(1),
            Value::UInt64(0),
        ),
        (
            sub(Num::UInt64),
            Lit::UInt64(0),
            Lit::UInt64(1),
            Value::UInt64(u64::MAX),
        ),
    ];
    for (op, l, r, want) in bin_cases {
        assert_eq!(eval_bin(op, l, r), want);
    }
    let neg_cases = [
        (Num::Int8, Lit::Int8(-128), Value::Int8(-128)),
        (Num::Int16, Lit::Int16(i16::MIN), Value::Int16(i16::MIN)),
        (Num::Int, Lit::Int(i32::MIN), Value::Int(i32::MIN)),
        (Num::Int64, Lit::Int64(i64::MIN), Value::Int64(i64::MIN)),
        (Num::UInt8, Lit::UInt8(1), Value::UInt8(255)),
        (Num::UInt32, Lit::UInt32(1), Value::UInt32(u32::MAX)),
        (Num::UInt64, Lit::UInt64(1), Value::UInt64(u64::MAX)),
    ];
    for (num, lit, want) in neg_cases {
        assert_eq!(eval_neg(num, lit), want);
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn div_rem_pow_shift_bigint_and_float_policy_boundaries() {
    let div = |n| BinOp::Arith(Arith::Div, n);
    let rem = |n| BinOp::Arith(Arith::Rem, n);
    let pow = |n| BinOp::Arith(Arith::Pow, n);
    let cases = [
        (div(Num::Int8), Lit::Int8(-7), Lit::Int8(2), Value::Int8(-3)),
        (div(Num::Int8), Lit::Int8(5), Lit::Int8(0), Value::Int8(0)),
        (
            div(Num::Int8),
            Lit::Int8(-128),
            Lit::Int8(-1),
            Value::Int8(-128),
        ),
        (rem(Num::Int8), Lit::Int8(-7), Lit::Int8(2), Value::Int8(-1)),
        (rem(Num::Int8), Lit::Int8(7), Lit::Int8(-2), Value::Int8(1)),
        (
            rem(Num::Int8),
            Lit::Int8(-128),
            Lit::Int8(-1),
            Value::Int8(0),
        ),
        (
            div(Num::Int),
            Lit::Int(i32::MIN),
            Lit::Int(-1),
            Value::Int(i32::MIN),
        ),
        (
            rem(Num::Int),
            Lit::Int(i32::MIN),
            Lit::Int(-1),
            Value::Int(0),
        ),
        (
            div(Num::Int64),
            Lit::Int64(i64::MIN),
            Lit::Int64(-1),
            Value::Int64(i64::MIN),
        ),
        (
            div(Num::UInt64),
            Lit::UInt64(u64::MAX),
            Lit::UInt64(2),
            Value::UInt64(9_223_372_036_854_775_807),
        ),
        (
            pow(Num::UInt64),
            Lit::UInt64(2),
            Lit::UInt64(4_294_967_296),
            Value::UInt64(0),
        ),
        (
            pow(Num::Int64),
            Lit::Int64(-1),
            Lit::Int64(4_294_967_297),
            Value::Int64(-1),
        ),
        (pow(Num::Int8), Lit::Int8(2), Lit::Int8(-1), Value::Int8(0)),
        (
            pow(Num::UInt8),
            Lit::UInt8(0),
            Lit::UInt8(0),
            Value::UInt8(1),
        ),
        (
            pow(Num::Int),
            Lit::Int(2),
            Lit::Int(31),
            Value::Int(i32::MIN),
        ),
        (
            BinOp::Arith(Arith::Add, Num::Float32),
            Lit::Float32(16_777_216.0),
            Lit::Float32(1.0),
            Value::Float32(16_777_216.0),
        ),
        (
            BinOp::Eq,
            Lit::Float(f64::NAN),
            Lit::Float(f64::NAN),
            Value::Bool(true),
        ),
        (
            BinOp::Eq,
            Lit::Float32(-0.0),
            Lit::Float32(0.0),
            Value::Bool(true),
        ),
        (
            BinOp::Lt,
            Lit::Float(f64::NAN),
            Lit::Float(1.0),
            Value::Bool(false),
        ),
    ];
    for (op, l, r, want) in cases {
        assert_eq!(eval_bin(op, l, r), want);
    }

    assert_eq!(
        eval_intrinsic(
            Intrinsic::FixedWidth(FixedWidthIntrinsic::Int64ShiftLeft),
            vec![Lit::Int64(1), Lit::Int(64)]
        ),
        Value::Int64(1)
    );
    assert_eq!(
        eval_intrinsic(
            Intrinsic::FixedWidth(FixedWidthIntrinsic::Int64ShiftRightUnsigned),
            vec![Lit::Int64(i64::MIN), Lit::Int(63)]
        ),
        Value::Int64(1)
    );

    let shift_src = "\
module App.Main exposing (main)
main : Unit -> Bool
let main () =
  Int.shift_left 1 32 == 1 && Int.shift_right_unsigned (-8) 1 == 2147483644
";
    let entry = Name::new("App.Main", "main");
    let (prog, _) = lower_source(shift_src, &entry);
    let mut heap = Heap::new();
    let exec = tier1::execute_main_program(&mut heap, &prog, &entry, 100_000)
        .expect("prep")
        .expect("exec");
    assert_eq!(exec.value, Value::Bool(true));
    heap.drop_value(exec.value).expect("drop");
    heap.finish().expect("zero leaks");

    assert_eq!(
        eval_bigint_bin(pow(Num::BigInt), "2", "128"),
        "340282366920938463463374607431768211456"
    );
    assert_eq!(eval_bigint_bin(div(Num::BigInt), "-7", "2"), "-3");
    assert_eq!(eval_bigint_bin(rem(Num::BigInt), "-7", "2"), "-1");
    assert_eq!(eval_bigint_bin(div(Num::BigInt), "5", "0"), "0");
}

#[test]
fn defects_on_unsafe_div_zero_and_negative_bigint_exp_leave_zero_leaks() {
    for (program, expected) in [
        (
            single_op_program(
                vec![
                    let_op(0, Operation::Literal(Lit::Int(5))),
                    let_op(1, Operation::Literal(Lit::Int(0))),
                    let_op(
                        2,
                        Operation::Intrinsic {
                            kind: Intrinsic::IntUnsafeDiv,
                            args: vec![0, 1],
                        },
                    ),
                ],
                2,
            ),
            DefectCode::IntDivisionByZero { remainder: false },
        ),
        (
            single_op_program(
                vec![
                    let_op(0, Operation::Literal(Lit::BigInt("2".into()))),
                    let_op(1, Operation::Literal(Lit::BigInt("-1".into()))),
                    let_op(
                        2,
                        Operation::Binary {
                            op: BinOp::Arith(Arith::Pow, Num::BigInt),
                            left: 0,
                            right: 1,
                        },
                    ),
                ],
                2,
            ),
            DefectCode::BigIntNegativeExponent,
        ),
    ] {
        let mut heap = Heap::new();
        let report =
            tier1::execute_program_outcome(&mut heap, &program, &main_name(), vec![], 10_000)
                .expect("tier1 prepares");
        let ProgramOutcome::Failed(cause) = report.outcome else {
            panic!("expected Failed defect, got {:?}", report.outcome);
        };
        let reasons: Vec<_> = heap.cause_reasons(&cause).expect("reasons").collect();
        let [Reason::Die(payload)] = reasons.as_slice() else {
            panic!("expected single Die reason");
        };
        assert_eq!(heap.defect(payload).expect("defect").code(), &expected);
        heap.drop_cause(cause).expect("drop cause");
        heap.finish().expect("zero leaks");
    }
}
