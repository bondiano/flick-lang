//! Immediate borrowed receivers are reuse candidates, not promises of reuse.
use flick_core::ir::{
    Lit, Pat,
    memory::{
        Block, DictionaryEntry, Exit, Function, Global, Instruction, Name, Operation, Program,
        ValueRep,
    },
};
use flick_core::opt::perceus;
use flick_core::types::{Constructor, Rep};
use flick_vm::tier0::{
    Error, Heap, ObjectId, ProgramOutcome, Value, execute_program, execute_program_outcome,
};

#[derive(Clone, Copy, Debug)]
enum Receiver {
    Field,
    Structure,
    Method,
}

#[derive(Clone, Copy, Debug)]
enum StructureLayout {
    Tuple,
    Record,
}

fn bind(binding: u32, value: Operation) -> Instruction {
    Instruction::Let { binding, value }
}
fn raw(instructions: Vec<Instruction>, result: u32) -> Function {
    Function {
        params: vec![0, 1],
        body: Block {
            instructions,
            exit: Exit::Return(result),
        },
    }
}
fn insert(function: &Function) -> Function {
    perceus::insert(function).expect("valid allocation-only IR")
}
fn program(body: Function) -> (Name, Program) {
    let initializer = insert(&Function {
        params: vec![],
        body: Block {
            instructions: vec![bind(
                0,
                Operation::Closure {
                    code: 1,
                    captures: vec![],
                },
            )],
            exit: Exit::Return(0),
        },
    });
    let name = Name::new("Receivers", "main");
    let program = Program {
        functions: [(0, initializer), (1, body)].into_iter().collect(),
        globals: vec![Global {
            name: name.clone(),
            initializer: 0,
        }],
    };
    (name, program)
}
fn run(heap: &mut Heap, body: Function, input: Value) -> Value {
    let (name, program) = program(body);
    execute_program(heap, &program, &name, vec![input], 10_000)
        .expect("execute real VM")
        .value
}
fn borrow(receiver: Receiver, count: usize) -> Operation {
    match receiver {
        Receiver::Field => Operation::Field {
            object: 1,
            field: "value".into(),
        },
        Receiver::Structure => Operation::Structure {
            object: 1,
            expected: count,
        },
        Receiver::Method => Operation::Method {
            dictionary: 1,
            field: "value".into(),
        },
    }
}
fn constructor(dicts: bool, live: bool) -> Operation {
    let fields = if live { vec![3, 1] } else { vec![3] };
    if dicts {
        Operation::ConstructWithDicts {
            tag: "M.Box.Box".into(),
            fields,
            dicts: vec![2],
            rep: ValueRep::Cell,
            layout: None,
        }
    } else {
        Operation::Construct {
            tag: "M.Box.Box".into(),
            fields,
            rep: ValueRep::Cell,
            layout: None,
        }
    }
}
fn body(receiver: Receiver, count: usize, dicts: bool, live: bool) -> Function {
    let mut source = vec![];
    if dicts {
        source.push(bind(2, Operation::Literal(Lit::String("evidence".into()))));
    }
    source.push(bind(3, borrow(receiver, count)));
    source.push(bind(4, constructor(dicts, live)));
    let annotated = insert(&raw(source, 4));
    // Pin the entire insertion, including the precise receiver and evidence suffix.
    let mut expected = vec![Instruction::Drop(0)];
    if dicts {
        expected.push(bind(2, Operation::Literal(Lit::String("evidence".into()))));
    }
    expected.push(bind(3, borrow(receiver, count)));
    expected.push(bind(
        4,
        if live {
            constructor(dicts, true)
        } else if dicts {
            Operation::ReuseWithDicts {
                object: 1,
                tag: "M.Box.Box".into(),
                fields: vec![3],
                dicts: vec![2],
                rep: ValueRep::Cell,
                layout: None,
            }
        } else {
            Operation::Reuse {
                object: 1,
                tag: "M.Box.Box".into(),
                fields: vec![3],
                rep: ValueRep::Cell,
                layout: None,
            }
        },
    ));
    assert_eq!(
        annotated.body,
        Block {
            instructions: expected,
            exit: Exit::Return(4)
        }
    );
    annotated
}
fn input(heap: &mut Heap, receiver: Receiver, count: usize, record: bool) -> Value {
    let child = heap.literal(&Lit::String("payload".into())).expect("child");
    match receiver {
        Receiver::Field => heap.record(vec![("value".into(), child)]).expect("record"),
        Receiver::Structure if record => {
            let mut fields = vec![("a".into(), child)];
            if count == 2 {
                fields.push(("b".into(), Value::Int(7)));
            }
            heap.record(fields).expect("record")
        }
        Receiver::Structure => {
            let mut fields = vec![child];
            if count == 2 {
                fields.push(Value::Int(7));
            }
            heap.construct("$tuple", fields).expect("tuple")
        }
        Receiver::Method => run(
            heap,
            insert(&raw(
                vec![bind(
                    3,
                    Operation::Dictionary {
                        entries: vec![("value".into(), DictionaryEntry::Value(1))],
                    },
                )],
                3,
            )),
            child,
        ),
    }
}
fn check_payload(heap: &Heap, value: &Value, receiver: Receiver, count: usize, record: bool) {
    match receiver {
        Receiver::Field | Receiver::Method => assert_eq!(heap.string(value), Ok("payload")),
        Receiver::Structure => {
            assert_eq!(heap.tag(value), Ok("$tuple"));
            assert_eq!(heap.field(value, 0), Ok(&Value::Bool(record)));
            let values = heap.field(value, 1).expect("values tuple");
            assert_eq!(heap.tag(values), Ok("$tuple"));
            assert_eq!(
                heap.string(heap.field(values, 0).expect("first value")),
                Ok("payload")
            );
            let labels = heap.field(value, 2).expect("labels tuple");
            for index in 0..count {
                if record {
                    assert_eq!(
                        heap.string(heap.field(labels, index).expect("label")),
                        Ok(if index == 0 { "a" } else { "b" })
                    );
                } else {
                    assert_eq!(heap.field(labels, index), Ok(&Value::Unit));
                }
            }
            if count == 2 {
                assert_eq!(heap.field(values, 1), Ok(&Value::Int(7)));
            }
        }
    }
}
fn check_layout(heap: &mut Heap, value: &Value, fields: usize, dicts: bool) {
    // Constructor matching checks the ordinary/evidence partition, not only capacity.
    let alias = heap.dup(value).expect("layout-check root");
    let pattern = Pat::Con(
        Constructor {
            module: "M".into(),
            ty: "Box".into(),
            name: "Box".into(),
            arity: fields,
            dicts: usize::from(dicts),
            rep: Rep::Tagged,
            fields: Vec::new(),
            type_args: Vec::new(),
        },
        vec![Pat::Wild; fields + usize::from(dicts)],
    );
    let matched = run(
        heap,
        insert(&raw(
            vec![bind(
                3,
                Operation::Match {
                    object: 1,
                    pattern: Box::new(pattern),
                    rep: ValueRep::Cell,
                },
            )],
            3,
        )),
        alias,
    );
    assert_eq!(heap.field(&matched, 0), Ok(&Value::Bool(true)));
    heap.drop_value(matched).expect("layout result");
}
fn case(receiver: Receiver, dicts: bool, shared: bool, live: bool, layout: StructureLayout) {
    let record = matches!(layout, StructureLayout::Record);
    let count = 1 + usize::from(dicts);
    let mut heap = Heap::new();
    let candidate = input(&mut heap, receiver, count, record);
    let old_id = candidate.object_id().expect("receiver identity");
    let alias = shared.then(|| heap.dup(&candidate).expect("external alias"));
    let before = heap.stats().reuses;
    let result = run(&mut heap, body(receiver, count, dicts, live), candidate);
    let reusable = matches!(receiver, Receiver::Structure) && !record && !shared && !live;
    assert_eq!(
        result.object_id() == Some(old_id),
        reusable,
        "{receiver:?}, evidence={dicts}, shared={shared}, live={live}, record={record}"
    );
    assert_eq!(heap.stats().reuses - before, usize::from(reusable));
    assert_eq!(heap.tag(&result), Ok("M.Box.Box"));
    check_payload(
        &heap,
        heap.field(&result, 0).expect("ordinary payload"),
        receiver,
        count,
        record,
    );
    let ordinary = 1 + usize::from(live);
    if dicts {
        assert_eq!(
            heap.string(heap.field(&result, ordinary).expect("evidence suffix")),
            Ok("evidence")
        );
    }
    if live {
        assert_eq!(
            heap.field(&result, 1)
                .expect("retained receiver")
                .object_id(),
            Some(old_id)
        );
    }
    assert_eq!(heap.is_live(old_id), reusable || shared || live);
    check_layout(&mut heap, &result, ordinary, dicts);
    heap.drop_value(result).expect("owned outcome release");
    if let Some(alias) = alias {
        assert!(heap.is_live(old_id), "external alias outlives result");
        match receiver {
            Receiver::Field => assert_eq!(
                heap.string(heap.named_field(&alias, "value").expect("unchanged record")),
                Ok("payload")
            ),
            Receiver::Structure => {
                if record {
                    assert_eq!(
                        heap.string(heap.named_field(&alias, "a").expect("unchanged record")),
                        Ok("payload")
                    );
                } else {
                    assert_eq!(heap.tag(&alias), Ok("$tuple"));
                    assert_eq!(
                        heap.string(heap.field(&alias, 0).expect("unchanged tuple")),
                        Ok("payload")
                    );
                }
            }
            Receiver::Method => {
                let value = run(
                    &mut heap,
                    insert(&raw(vec![bind(3, borrow(Receiver::Method, count))], 3)),
                    alias,
                );
                assert_eq!(heap.string(&value), Ok("payload"));
                heap.drop_value(value).expect("aliased dictionary method");
                let stats = heap.finish().expect("no occupied heap slots");
                assert_eq!(stats.live_objects, 0);
                assert_eq!(stats.allocations, stats.frees);
                return;
            }
        }
        heap.drop_value(alias).expect("external receiver release");
    }
    let stats = heap.finish().expect("no occupied heap slots");
    assert_eq!(stats.live_objects, 0);
    assert_eq!(stats.allocations, stats.frees);
}

#[test]
fn field_to_construct() {
    case(Receiver::Field, false, false, false, StructureLayout::Tuple);
}
#[test]
fn field_to_construct_with_dicts() {
    case(Receiver::Field, true, false, false, StructureLayout::Tuple);
}
#[test]
fn structure_to_construct() {
    case(
        Receiver::Structure,
        false,
        false,
        false,
        StructureLayout::Tuple,
    );
}
#[test]
fn structure_to_construct_with_dicts() {
    case(
        Receiver::Structure,
        true,
        false,
        false,
        StructureLayout::Tuple,
    );
}
#[test]
fn method_to_construct() {
    case(
        Receiver::Method,
        false,
        false,
        false,
        StructureLayout::Tuple,
    );
}
#[test]
fn method_to_construct_with_dicts() {
    case(Receiver::Method, true, false, false, StructureLayout::Tuple);
}
#[test]
fn structure_record_candidates_fall_back_without_losing_labels_or_values() {
    for dicts in [false, true] {
        case(
            Receiver::Structure,
            dicts,
            false,
            false,
            StructureLayout::Record,
        );
    }
}
#[test]
fn shared_receivers_preserve_external_aliases() {
    for receiver in [Receiver::Field, Receiver::Structure, Receiver::Method] {
        for dicts in [false, true] {
            case(receiver, dicts, true, false, StructureLayout::Tuple);
        }
    }
}
#[test]
fn live_receivers_are_not_emitted_as_reuse_candidates() {
    for receiver in [Receiver::Field, Receiver::Structure, Receiver::Method] {
        for dicts in [false, true] {
            case(receiver, dicts, false, true, StructureLayout::Tuple);
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct PrefixCase {
    receiver: Receiver,
    dicts: bool,
    shared: bool,
    live: bool,
    layout: StructureLayout,
}

impl PrefixCase {
    fn record(self) -> bool {
        matches!(self.layout, StructureLayout::Record)
    }
    fn count(self) -> usize {
        1 + usize::from(self.dicts)
    }
    fn reusable(self) -> bool {
        matches!(self.receiver, Receiver::Structure) && !self.record() && !self.shared && !self.live
    }
    fn check_result(self, heap: &mut Heap, value: &Value, old_id: ObjectId) {
        assert_eq!(value.object_id() == Some(old_id), self.reusable());
        assert_eq!(heap.tag(value), Ok("M.Box.Box"));
        check_payload(
            heap,
            heap.field(value, 0).expect("ordinary payload"),
            self.receiver,
            self.count(),
            self.record(),
        );
        let ordinary = 1 + usize::from(self.live);
        if self.dicts {
            assert_eq!(
                heap.string(heap.field(value, ordinary).expect("evidence suffix")),
                Ok("evidence")
            );
        }
        if self.live {
            assert_eq!(
                heap.field(value, 1).expect("retained receiver").object_id(),
                Some(old_id)
            );
        }
        check_layout(heap, value, ordinary, self.dicts);
    }
    fn check_original(self, heap: &mut Heap, alias: &Value) {
        match self.receiver {
            Receiver::Field => assert_eq!(
                heap.string(heap.named_field(alias, "value").expect("unchanged record")),
                Ok("payload")
            ),
            Receiver::Structure => {
                let first = if self.record() {
                    heap.named_field(alias, "a").expect("unchanged record")
                } else {
                    assert_eq!(heap.tag(alias), Ok("$tuple"));
                    heap.field(alias, 0).expect("unchanged tuple")
                };
                assert_eq!(heap.string(first), Ok("payload"));
            }
            Receiver::Method => {
                let input = heap.dup(alias).expect("independent method lookup token");
                let value = run(
                    heap,
                    insert(&raw(vec![bind(3, borrow(self.receiver, self.count()))], 3)),
                    input,
                );
                assert_eq!(heap.string(&value), Ok("payload"));
                heap.drop_value(value).expect("method lookup result");
            }
        }
    }
}

fn every_prefix(case: PrefixCase) -> u64 {
    let (name, program) = program(body(case.receiver, case.count(), case.dicts, case.live));
    let mut exhausted = false;
    for fuel in 0..=10_000 {
        let mut heap = Heap::new();
        let outside = heap.literal(&Lit::String("external".into())).expect("root");
        let outside_id = outside.object_id().expect("external identity");
        let candidate = input(&mut heap, case.receiver, case.count(), case.record());
        let old_id = candidate.object_id().expect("receiver identity");
        let alias = case
            .shared
            .then(|| heap.dup(&candidate).expect("independent receiver root"));
        let before = heap.stats().reuses;
        let report = execute_program_outcome(&mut heap, &program, &name, vec![candidate], fuel);
        let completed = match &report.outcome {
            ProgramOutcome::Returned(value) => {
                case.check_result(&mut heap, value, old_id);
                assert_eq!(heap.stats().reuses - before, usize::from(case.reusable()));
                true
            }
            ProgramOutcome::Fault(Error::OutOfFuel) => {
                exhausted = true;
                assert!(heap.stats().reuses - before <= usize::from(case.reusable()));
                false
            }
            other => {
                let diagnostic = format!("unexpected {other:?}, {case:?}, fuel={fuel}");
                report
                    .outcome
                    .release(&mut heap)
                    .expect("unexpected outcome");
                if let Some(alias) = alias {
                    heap.drop_value(alias).expect("external receiver");
                }
                heap.drop_value(outside).expect("external root");
                let cleanup = heap.finish();
                panic!("{diagnostic}; cleanup={cleanup:?}");
            }
        };
        assert_eq!(report.stdout, "");
        report
            .outcome
            .release(&mut heap)
            .expect("single outcome release");
        assert_eq!(heap.is_live(old_id), case.shared, "{case:?}, fuel={fuel}");
        if let Some(alias) = alias {
            case.check_original(&mut heap, &alias);
            heap.drop_value(alias).expect("external receiver release");
        }
        assert!(heap.is_live(outside_id), "no sweep of independent roots");
        assert_eq!(heap.string(&outside), Ok("external"));
        heap.drop_value(outside).expect("external root release");
        let stats = heap.finish().expect("zero actual occupied heap slots");
        assert_eq!(stats.live_objects, 0);
        assert_eq!(stats.allocations, stats.frees);
        assert_eq!(stats.cells_allocated, stats.cells_freed);
        assert_eq!(stats.arenas_allocated, stats.arenas_freed);
        if completed {
            assert!(
                exhausted,
                "the consecutive prefix range includes exhaustion"
            );
            eprintln!("receiver prefix terminal={fuel} case={case:?}");
            return fuel + 1;
        }
    }
    panic!("no successful completion within the unchanged 10,000 fuel ceiling: {case:?}");
}

#[test]
fn every_receiver_prefix_releases_vm_roots_without_sweeping_external_aliases() {
    let mut cases = 0;
    let mut prefixes = 0;
    for receiver in [Receiver::Field, Receiver::Structure, Receiver::Method] {
        for dicts in [false, true] {
            for (shared, live) in [(false, false), (true, false), (false, true)] {
                prefixes += every_prefix(PrefixCase {
                    receiver,
                    dicts,
                    shared,
                    live,
                    layout: StructureLayout::Tuple,
                });
                cases += 1;
            }
        }
    }
    for dicts in [false, true] {
        prefixes += every_prefix(PrefixCase {
            receiver: Receiver::Structure,
            dicts,
            shared: false,
            live: false,
            layout: StructureLayout::Record,
        });
        cases += 1;
    }
    assert_eq!(cases, 20, "the original complete receiver scenario matrix");
    eprintln!("all receiver scenarios={cases}, consecutive fuel prefixes={prefixes}");
}
