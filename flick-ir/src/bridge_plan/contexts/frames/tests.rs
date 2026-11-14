use super::*;
use crate::bridge_plan::{BridgeBinder, BridgeEntry, BridgeKindId, BridgeNodeId, BridgeSelect};

#[test]
fn persistent_name_indexes_preserve_nearest_binding_identity_and_misses() {
    let plan = BridgePlan {
        kinds: vec![],
        witnesses: vec![],
        constructors: vec![],
        nodes: vec![],
        aliases: vec![],
        entries: vec![BridgeEntry {
            root: BridgeNodeId(0),
            free: ["x", "y"]
                .map(|name| BridgeBinder {
                    name: name.into(),
                    kind: BridgeKindId(0),
                })
                .to_vec(),
            select: BridgeSelect::Value,
        }],
    };
    let mut frames = Frames::new(&plan);
    let root = frames.empty(Root::Entry(0));
    let old = BindingId::Entry(0, 0);
    let outer = frames.bind(
        root,
        Binding {
            id: old,
            name: "x".into(),
            kind: BridgeKindId(0),
        },
    );
    let new = BindingId::Forall(outer, 7, 0);
    let shadow = frames.bind(
        outer,
        Binding {
            id: new,
            name: "x".into(),
            kind: BridgeKindId(1),
        },
    );
    let sibling = frames.bind(
        outer,
        Binding {
            id: BindingId::Entry(0, 1),
            name: "y".into(),
            kind: BridgeKindId(0),
        },
    );
    assert!(frames.lookup(root, "x").is_none());
    assert!(frames.lookup(shadow, "y").is_none());
    assert!(frames.lookup(shadow, "absent").is_none());
    assert!(
        frames
            .lookup(outer, "x")
            .is_some_and(|binding| binding.id == old && binding.kind == BridgeKindId(0))
    );
    assert!(
        frames
            .lookup(shadow, "x")
            .is_some_and(|binding| binding.id == new && binding.kind == BridgeKindId(1))
    );
    assert!(
        frames
            .lookup(sibling, "x")
            .is_some_and(|binding| binding.id == old)
    );
    assert!(frames.lookup(sibling, "y").is_some());
    let reused = frames.bind(
        root,
        Binding {
            id: old,
            name: "y".into(),
            kind: BridgeKindId(1),
        },
    );
    assert_eq!(
        reused, outer,
        "interning preserves the original frame identity"
    );
    assert!(frames.lookup(reused, "y").is_none());
}
