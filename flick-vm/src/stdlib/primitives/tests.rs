use super::*;
use crate::stdlib::FuelCounter;

#[test]
fn invalid_array_layout_rejects_before_charging_or_reading_fields() {
    for tag in ["$list", "$tuple"] {
        for ordinary_fields in [0, 2] {
            let mut heap = Heap::new();
            let field = heap
                .literal(&Lit::String("owned physical field".into()))
                .expect("field");
            // Private injection only: public construction rejects these layouts.
            let input = heap.allocate(Data::Tagged {
                tag: tag.into(),
                ordinary_fields,
                fields: vec![field],
                layout: None,
            });
            let expected = if ordinary_fields == 0 {
                Error::ReservedConstructorEvidence(tag.into())
            } else {
                Error::InvalidTaggedBoundary {
                    ordinary: 2,
                    physical: 1,
                }
            };
            let mut fuel = 0;
            assert_eq!(
                array_to_list(&mut heap, &input, &mut FuelCounter(&mut fuel)),
                Err(expected)
            );
            assert_eq!(fuel, 0);
            heap.drop_value(input)
                .expect("destruction releases all physical fields");
            let stats = heap.finish().expect("no hidden evidence leaks");
            assert_eq!(stats.allocations, stats.frees);
        }
    }
}

#[test]
fn failed_projection_releases_all_previously_retained_fields() {
    let mut heap = Heap::new();
    let outside = heap
        .literal(&Lit::String("outside".into()))
        .expect("external root");
    let first = heap.dup(&outside).expect("valid first field");
    let dead = heap.construct("Dead", vec![]).expect("old object");
    let dead_id = dead.object_id().expect("old identity");
    let stale = dead.representation();
    heap.drop_value(dead).expect("retire old object");
    // A private corrupted heap exercises cleanup after the first successful dup.
    let input = heap.allocate(Data::Tagged {
        tag: "$tuple".into(),
        ordinary_fields: 2,
        fields: vec![first, stale],
        layout: None,
    });
    let mut fuel = 100;
    assert_eq!(
        array_to_list(&mut heap, &input, &mut FuelCounter(&mut fuel)),
        Err(Error::InvalidObject(dead_id))
    );
    assert_eq!(heap.reference_count(&outside), Ok(2));
    let Data::Tagged { fields, .. } = &mut heap
        .object_mut(input.object_id().expect("input identity"))
        .expect("input object")
        .data
    else {
        panic!("tuple layout")
    };
    // Remove the intentionally unowned stale representation before destruction.
    fields[1] = Value::Unit;
    heap.drop_value(input).expect("valid tuple fields retire");
    assert_eq!(heap.string(&outside), Ok("outside"));
    assert_eq!(heap.reference_count(&outside), Ok(1));
    heap.drop_value(outside).expect("external release");
    let stats = heap.finish().expect("no retained projection tokens");
    assert_eq!(stats.allocations, stats.frees);
}
