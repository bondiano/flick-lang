use super::*;

#[test]
fn failed_partial_tail_selection_has_no_counted_allocations_or_retains() {
    let mut heap = Heap::new();
    let text = heap
        .literal(&Lit::String("kept only by input".into()))
        .expect("text");
    let list = heap.construct("$list", vec![text]).expect("list");
    let input = heap
        .construct("$tuple", vec![list, Value::Bool(true)])
        .expect("pair");
    let pattern = Pat::Tuple(vec![
        Pat::List(vec![], Some(Box::new(Pat::Bind("tail".into())))),
        Pat::Lit(Lit::Bool(false)),
    ]);
    let before = heap.stats();
    let selected = select(&heap, &input, &pattern, &mut 1000).expect("selection");
    assert!(selected.is_none());
    assert_eq!(
        heap.stats(),
        before,
        "selection never creates an owned token or a bound tail"
    );
    let mut scratch = vec![];
    let result =
        execute(&mut heap, &input, &pattern, &mut 1000, &mut scratch).expect("failed match result");
    assert_eq!(
        heap.stats().allocations,
        before.allocations + 1,
        "only the false result object allocates"
    );
    assert_eq!(heap.stats().dups, before.dups);
    assert!(scratch.is_empty());
    heap.drop_values(vec![input, result])
        .expect("explicit roots release");
    heap.finish().expect("no failed-tail storage");
}

#[test]
fn update_preserves_base_field_order_and_takes_last_duplicate() {
    let mut heap = Heap::new();
    let base = heap
        .record(vec![
            ("z".into(), Value::Int(1)),
            ("a".into(), Value::Int(2)),
        ])
        .expect("record");
    let mut operands = vec![base, Value::Int(7), Value::Int(8), Value::Int(9)];
    let fields = vec![("a".into(), 1), ("z".into(), 2), ("a".into(), 3)];
    let result = update(&mut heap, &mut operands, &fields, &mut 1000).expect("record update");
    let Value::Object(token) = &result else {
        panic!("updated record")
    };
    let Data::Record(fields) = &heap.object(token.id).expect("record allocation").data else {
        panic!("record storage")
    };
    assert_eq!(
        fields,
        &[("z".into(), Value::Int(8)), ("a".into(), Value::Int(9))]
    );
    operands.push(result);
    heap.drop_values(operands)
        .expect("staged operands and output release");
    heap.finish().expect("no update leak");
}

#[test]
fn deferred_tail_materialization_checks_reserved_collection_metadata() {
    let mut heap = Heap::new();
    let hidden = heap
        .literal(&Lit::String("hidden owned field".into()))
        .expect("hidden field");
    let root = heap.allocate(Data::Tagged {
        tag: "$list".into(),
        ordinary_fields: 1,
        fields: vec![Value::Int(1), hidden],
        layout: None,
    });
    let object = root.object_id().expect("list identity");
    let mut scratch = vec![];
    assert_eq!(
        materialize(
            &mut heap,
            &root,
            &Place::tail(object, 0, 2),
            &mut 1000,
            &mut scratch
        ),
        Err(Error::ReservedConstructorEvidence("$list".into()))
    );
    assert!(scratch.is_empty());
    assert_eq!(heap.stats().dups, 0);
    heap.drop_value(root)
        .expect("all physical list fields release");
    heap.finish()
        .expect("rejected tail has no owned temporaries");
}
