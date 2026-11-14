//! Owner evidence is an inert, explicit execution context, not default ownership.
use std::collections::{BTreeMap, BTreeSet};

use flick_ir::memory::{self, Block, Exit, Function, Instruction, Operation};
use flick_ir::{
    Expr, ExprKind, FileId, Item, ItemKind, Lit, Module, Param, Pat, Program, Span, Stage,
};

fn at(kind: ExprKind) -> Expr {
    Expr::new(kind, Span::new(FileId::new(0), 0, 0))
}

fn local(name: &str) -> Expr {
    at(ExprKind::Local(name.into()))
}

fn call(stage: Stage) -> Expr {
    at(ExprKind::App {
        func: Box::new(local("f")),
        arg: Box::new(local("a")),
        stage,
        arg_first: false,
    })
}

fn program(body: Expr) -> Program {
    let body = at(ExprKind::Lambda {
        params: ["f", "a", "owner"]
            .map(|name| Param {
                pat: Pat::Bind(name.into()),
                stage: Stage::Pure,
                ty: None,
            })
            .to_vec(),
        body: Box::new(body),
    });
    Program {
        ir_version: flick_ir::VERSION.into(),
        compiler: "test".into(),
        files: vec!["test.fl".into()],
        modules: vec![Module {
            name: "App.Main".into(),
            items: vec![Item {
                span: body.span,
                kind: ItemKind::Value {
                    name: "test".into(),
                    dicts: vec![],
                    body,
                },
            }],
            labels: BTreeMap::default(),
            initialization: flick_ir::Initialization::default(),
            records: BTreeMap::default(),
            package: None,
        }],
        lang: vec![],
    }
}

fn errors(body: Expr) -> Vec<String> {
    flick_ir::validate(
        &program(body),
        &|_| false,
        &BTreeSet::from([FileId::new(0)]),
    )
}

#[test]
fn evidence_brackets_only_evaluated_effectful_invocations() {
    for invocation in [
        call(Stage::Runtime),
        call(Stage::Sync),
        at(ExprKind::Await(Box::new(local("a")))),
    ] {
        assert!(
            errors(at(ExprKind::WithOwnerEvidence(
                Box::new(at(ExprKind::ReadOwnerEvidence)),
                Box::new(invocation),
            )))
            .is_empty()
        );
    }
    for invocation in [
        call(Stage::Pure),
        at(ExprKind::Task(Box::new(call(Stage::Runtime)))),
        at(ExprKind::Block(vec![], Box::new(call(Stage::Runtime)))),
        at(ExprKind::Await(Box::new(at(ExprKind::ReadOwnerEvidence)))),
    ] {
        assert!(
            !errors(at(ExprKind::WithOwnerEvidence(
                Box::new(local("owner")),
                Box::new(invocation),
            )))
            .is_empty(),
            "the boundary must not defer operand evaluation or task construction"
        );
    }
}

#[test]
fn evidence_children_are_visited_owner_before_invocation() {
    let mut expression = at(ExprKind::WithOwnerEvidence(
        Box::new(local("owner")),
        Box::new(call(Stage::Runtime)),
    ));
    let mut visited = Vec::new();
    expression.each_child(&mut |child| visited.push(child.kind.clone()));
    assert_eq!(visited, [local("owner").kind, call(Stage::Runtime).kind]);
    expression.each_child_mut(&mut |child| child.span.start = 1);
    expression.each_child(&mut |child| assert_eq!(child.span.start, 1));
    at(ExprKind::ReadOwnerEvidence).each_child(&mut |_| panic!("a read has no operands"));
}

#[test]
fn memory_evidence_consumes_owner_and_arguments_without_borrowing() {
    assert!(Operation::ReadOwnerEvidence.consumed().is_empty());
    assert_eq!(Operation::ReadOwnerEvidence.borrowed(), None);
    let operation = Operation::WithOwnerEvidence {
        owner: 0,
        code: 9,
        args: vec![1],
    };
    assert_eq!(operation.consumed(), [0, 1]);
    assert_eq!(operation.borrowed(), None);
    let mut program = memory::Program {
        globals: vec![],
        functions: BTreeMap::from([
            (
                0,
                Function {
                    params: vec![0, 1],
                    body: Block {
                        instructions: vec![Instruction::Let {
                            binding: 2,
                            value: operation,
                        }],
                        exit: Exit::Return(2),
                    },
                },
            ),
            (
                9,
                Function {
                    params: vec![0],
                    body: Block {
                        instructions: vec![],
                        exit: Exit::Return(0),
                    },
                },
            ),
        ]),
    };
    assert_eq!(memory::validate_program(&program), Ok(()));
    program
        .functions
        .get_mut(&9)
        .expect("helper")
        .params
        .push(1);
    assert!(
        matches!(memory::validate_program(&program), Err(memory::Error::InCode { error, .. })
        if matches!(*error, memory::Error::Arity { code: 9, expected: 2, actual: 1 }))
    );
}

#[test]
#[cfg(feature = "json")]
fn evidence_round_trips_and_rejects_the_previous_semantic_version() {
    let value = program(at(ExprKind::WithOwnerEvidence(
        Box::new(at(ExprKind::ReadOwnerEvidence)),
        Box::new(call(Stage::Runtime)),
    )));
    assert_eq!(
        Program::from_json(&value.to_json()).expect("evidence JSON"),
        value
    );
    let mut previous = value.clone();
    previous.ir_version = "0.10.0".into();
    assert!(Program::from_json(&previous.to_json()).is_err());
    let operations = vec![
        Operation::ReadOwnerEvidence,
        Operation::WithOwnerEvidence {
            owner: 0,
            code: 9,
            args: vec![1],
        },
    ];
    let json = serde_json::to_string(&operations).expect("memory evidence JSON");
    assert_eq!(
        serde_json::from_str::<Vec<Operation>>(&json).expect("read memory JSON"),
        operations
    );
    assert_eq!(
        serde_json::to_string(&ExprKind::Within(
            "owner".into(),
            Box::new(at(ExprKind::Lit(Lit::Unit)))
        ))
        .expect("old Within shape"),
        "{\"Within\":[\"owner\",{\"kind\":{\"Lit\":\"Unit\"},\"span\":{\"file\":0,\"start\":0,\"end\":0},\"ty\":null}]}"
    );
    #[cfg(feature = "cbor")]
    {
        assert_eq!(
            Program::from_cbor(&value.to_cbor()).expect("evidence CBOR"),
            value
        );
        let mut bytes = Vec::new();
        ciborium::into_writer(&operations, &mut bytes).expect("memory CBOR");
        assert_eq!(
            ciborium::from_reader::<Vec<Operation>, _>(bytes.as_slice()).expect("read memory CBOR"),
            operations
        );
    }
}
