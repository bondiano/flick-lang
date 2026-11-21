use super::*;
use flick_core::ir::Lit;

#[test]
fn overflow_after_a_valid_duplicate_prefix_releases_every_accepted_edge() {
    let mut heap = Heap::new();
    let first = heap
        .literal(&Lit::String("first".into()))
        .expect("first root");
    let overflow = heap
        .literal(&Lit::String("overflow".into()))
        .expect("overflow root");
    let id = overflow.object_id().expect("object identity");
    heap.object_mut(id)
        .expect("private overflow fixture")
        .references = usize::MAX;
    assert_eq!(
        heap.cause(&[Reason::Fail(&first), Reason::Die(&overflow)], &mut 100),
        Err(Error::ReferenceOverflow(id))
    );
    assert_eq!(heap.reference_count(&first), Ok(1));
    assert_eq!(heap.reference_count(&overflow), Ok(usize::MAX));
    heap.object_mut(id)
        .expect("restore fixture reference count")
        .references = 1;
    heap.drop_values(vec![first, overflow])
        .expect("original roots released");
    heap.finish().expect("accepted duplicates reclaimed");
}

#[test]
fn malformed_payload_after_valid_prefix_does_not_lose_accepted_duplicates() {
    let mut heap = Heap::new();
    let first = heap
        .literal(&Lit::String("first".into()))
        .expect("first root");
    let stale = heap
        .literal(&Lit::String("stale".into()))
        .expect("stale root");
    let id = stale.object_id().expect("diagnostic identity only");
    heap.drop_value(stale).expect("retire original object");
    // Private integrity injection, deliberately impossible through public token construction.
    let invalid = Value::Object(ObjectToken { id });
    assert_eq!(
        heap.cause(&[Reason::Fail(&first), Reason::Die(&invalid)], &mut 100),
        Err(Error::InvalidObject(id))
    );
    assert_eq!(heap.reference_count(&first), Ok(1));
    heap.drop_value(first).expect("original release");
    heap.finish()
        .expect("failed builder drains accepted duplicates");
}

#[test]
fn cause_duplicate_overflow_and_malformed_kind_do_not_mutate_owned_edges() {
    let mut heap = Heap::new();
    let payload = heap
        .literal(&Lit::String("original".into()))
        .expect("payload");
    let cause = heap
        .cause(&[Reason::Die(&payload)], &mut 10)
        .expect("cause");
    let id = cause.token.id;
    heap.object_mut(id)
        .expect("private overflow fixture")
        .references = usize::MAX;
    assert_eq!(heap.dup_cause(&cause), Err(Error::ReferenceOverflow(id)));
    assert_eq!(heap.reference_count(&payload), Ok(2));
    heap.object_mut(id)
        .expect("restore fixture count")
        .references = 1;
    let invalid = CauseToken {
        token: ObjectToken {
            id: payload.object_id().expect("payload identity"),
        },
    };
    assert_eq!(heap.dup_cause(&invalid), Err(Error::ExpectedCause));
    heap.drop_cause(cause).expect("actual owned cause release");
    heap.drop_value(payload).expect("original release");
    let stats = heap.finish().expect("no dropped cause edge was lost");
    assert_eq!(stats.allocations, stats.frees);
}

#[test]
fn closed_resource_errors_are_not_globally_promoted() {
    assert!(matches!(
        StepFailure::classify(Error::ClosedOwner),
        StepFailure::Evaluator(Error::ClosedOwner)
    ));
    assert!(matches!(
        StepFailure::classify(Error::ClosedArena),
        StepFailure::Evaluator(Error::ClosedArena)
    ));
}
