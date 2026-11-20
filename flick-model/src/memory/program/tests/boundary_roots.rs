#[test]
fn exceptional_boundaries_use_saved_owner_not_current_selection() {
    let block = Block {
        instructions: vec![],
        exit: Exit::Return(0),
    };
    let program = Program {
        functions: BTreeMap::new(),
        globals: vec![],
    };
    let mut heap = Heap::new();
    let first = heap.allocate(Data::Arena {
        execution: 0,
        contents: vec![],
    });
    let selected = heap.allocate(Data::Arena {
        execution: 0,
        contents: vec![],
    });
    let failure = heap
        .cause(&[Reason::Fail(&Value::Unit)], &mut 3)
        .expect("typed fixture cause");
    let frame_with = |destination| Frame {
        recovery_id: None,
        evidence: None,
        block: &block,
        at: 0,
        locals: Locals::new(),
        vars: vec![],
        iterators: vec![],
        destination,
    };
    let mut machine = fixture_machine(&program, &mut heap);
    machine.owners = vec![
        OwnerState::default(),
        OwnerState {
            closing: false,
            closed: false,
            finalizers: vec![],
            arenas: vec![first],
        },
        OwnerState {
            closing: false,
            closed: false,
            finalizers: vec![],
            arenas: vec![selected],
        },
    ];
    machine.current_owner = 2;
    machine.frames.push(frame_with(Destination::CloseScope {
        owner: 1,
        previous: 0,
        binding: Some(0),
    }));
    machine.failure = Some(failure);
    machine.fuel = 100;
    machine.main_stage = MainStage::Done;
    machine.unwind().expect("valid typed-failure test");
    assert!(machine.owners[1].closed);
    assert!(!machine.owners[2].closed);
    assert_eq!(machine.current_owner, 0);
    assert_eq!(machine.heap.stats().arenas_freed, 1);
    machine.current_owner = 2;
    machine.frames.push(frame_with(Destination::RestoreOwner {
        previous: 0,
        binding: 0,
    }));
    machine.unwind().expect("valid typed-failure test");
    assert_eq!(machine.current_owner, 0);
    assert!(
        !machine.owners[2].closed,
        "selection restores without closing storage"
    );
    assert_eq!(machine.heap.stats().arenas_freed, 1);
    machine.cleanup().expect("valid typed-failure test");
    machine.heap.finish().expect("valid typed-failure test");
}

#[test]
fn catching_releases_frame_locals_before_handler_execution() {
    let caller = function(
        vec![],
        vec![Instruction::Let {
            binding: 0,
            value: Operation::Literal(Lit::Unit),
        }],
        0,
    );
    let handler = function(vec![0, 1], vec![], 1);
    let program = Program {
        functions: BTreeMap::from([(0, caller), (1, handler)]),
        globals: vec![],
    };
    let mut heap = Heap::new();
    let local = heap
        .literal(&Lit::String("unwound local".into()))
        .expect("valid typed-failure test");
    let local_id = local.object_id().expect("unwound local identity");
    let handler = heap.allocate(Data::Closure {
        execution: 0,
        code: 1,
        captures: vec![],
    });
    let handler_id = handler.object_id().expect("selector closure identity");
    let block = &program.functions[&0].body;
    let failure = heap
        .cause(&[Reason::Fail(&Value::Unit)], &mut 3)
        .expect("typed fixture cause");
    let cause_id = heap
        .objects
        .iter()
        .enumerate()
        .find_map(|(index, slot)| {
            slot.object
                .as_ref()
                .filter(|object| matches!(object.data, Data::Cause(_)))
                .map(|_| ObjectId {
                    heap: heap.identity,
                    index,
                    generation: slot.generation,
                })
        })
        .expect("single original cause");
    let mut machine = fixture_machine(&program, &mut heap);
    machine.fuel = 100;
    let mut catch_locals = Locals::new();
    catch_locals.insert_one(0, local);
    machine.frames = vec![
        Frame {
            recovery_id: None,
            evidence: None,
            block,
            at: 0,
            locals: Locals::new(),
            vars: vec![],
            iterators: vec![],
            destination: Destination::Top,
        },
        Frame {
            recovery_id: None,
            evidence: None,
            block,
            at: 0,
            locals: catch_locals,
            vars: vec![],
            iterators: vec![],
            destination: Destination::Catch {
                handler,
                binding: 1,
            },
        },
    ];
    machine.failure = Some(failure);
    machine.unwind().expect("valid typed-failure test");
    assert_eq!(
        machine.frames.len(),
        2,
        "handler just installed, not executed"
    );
    assert_eq!(
        machine.heap.stats().live_objects,
        2,
        "exact original C and selector closure remain"
    );
    assert!(
        !machine.heap.is_live(local_id),
        "unwound local reclaimed before selector execution"
    );
    assert!(machine.heap.is_live(handler_id));
    assert!(machine.heap.is_live(cause_id));
    assert_eq!(machine.recoveries.len(), 1);
    assert!(machine.failure.is_none());
    machine.cleanup().expect("valid typed-failure test");
    machine.heap.finish().expect("valid typed-failure test");
}
#[test]
fn owner_selection_validates_identity_but_not_resource_admission() {
    let program = Program {
        functions: BTreeMap::new(),
        globals: vec![],
    };
    let mut heap = Heap::new();
    let mut machine = Machine {
        heap: &mut heap,
        program: &program,
        execution: 7,
        globals: BTreeMap::new(),
        cells: vec![],
        lazies: vec![],
        owners: vec![OwnerState {
            closing: false,
            closed: true,
            finalizers: vec![],
            arenas: vec![],
        }],
        current_owner: 0,
        current_evidence: None,
        frames: vec![],
        activations: BTreeMap::new(),
        providers: BTreeMap::new(),
        segments: BTreeMap::new(),
        continuations: BTreeMap::new(),
        generators: BTreeMap::new(),
        invocations: BTreeMap::new(),
        discard: None,
        ready: None,
        next_handler: 0,
        arguments: VecDeque::new(),
        scratch: vec![],
        pending: None,
        failure: None,
        failure_elected: false,
        recoveries: BTreeMap::new(),
        next_recovery: 0,
        pending_handler: None,
        top: None,
        stdout: String::new(),
        fuel: 100,
        main_stage: MainStage::Done,
        root: None,
    };
    assert_eq!(
        machine.owner_index(&Value::Int(42)),
        Err(Error::ExpectedOwner)
    );
    assert_eq!(
        machine.owner_index(&Value::Owner(machine.owner_handle(0))),
        Ok(0)
    );
    let mut owner = machine.owner_handle(0);
    owner.index = usize::MAX;
    assert_eq!(
        machine.owner_index(&Value::Owner(owner)),
        Err(Error::ForeignOwner)
    );
    let mut owner = machine.owner_handle(0);
    owner.execution += 1;
    assert_eq!(
        machine.owner_index(&Value::Owner(owner)),
        Err(Error::ForeignOwner)
    );
    let other = Heap::new();
    let mut owner = machine.owner_handle(0);
    owner.heap = other.identity;
    assert_eq!(
        machine.owner_index(&Value::Owner(owner)),
        Err(Error::ForeignOwner)
    );
    machine
        .cleanup()
        .expect("retired owner cleanup stays idempotent");
    machine.heap.finish().expect("no retained objects");
    other.finish().expect("foreign heap untouched");
}
