use crate::types::Elided;
use std::hash::{DefaultHasher, Hash, Hasher};

use super::{Effects, Lang, Stage, Type};

#[test]
fn direct_changes_the_stage_and_not_the_type() {
    let plain = Effects::new(Type::empty_row(), Type::empty_row());
    let direct = Effects {
        direct: true,
        ..plain.clone()
    };
    let hash = |e: &Effects| {
        let mut hasher = DefaultHasher::new();
        e.hash(&mut hasher);
        hasher.finish()
    };
    assert_eq!(plain, direct);
    assert_eq!(hash(&plain), hash(&direct));
    assert_eq!(plain.stage(), Stage::Runtime);
    assert_eq!(direct.stage(), Stage::Sync);
    // an interface of the default mode does not change
    let json = |e: &Effects| serde_json::to_string(e).expect("serializes");
    assert!(!json(&plain).contains("direct"));
    assert!(json(&direct).contains("\"direct\":true"));
    let read: Effects = serde_json::from_str(&json(&direct)).expect("reads");
    assert!(read.direct);
}

#[test]
fn substitution_preserves_capture_qualification_kind() {
    for value in [false, true] {
        let ty = Type::Captured {
            ty: Box::new(Type::Var("a".into())),
            captures: Box::new(Type::Var("c".into())),
            value,
            elided: Elided::default(),
        };
        let substituted = ty.substitute(&[
            ("a".into(), Type::lang(Lang::Float)),
            ("c".into(), Type::empty_row()),
        ]);
        assert_eq!(
            substituted,
            Type::Captured {
                ty: Box::new(Type::lang(Lang::Float)),
                captures: Box::new(Type::empty_row()),
                value,
                elided: Elided::default(),
            }
        );
    }
}
