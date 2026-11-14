use super::Store;
use crate::types::Type;

#[test]
fn bound_alias_constructor_heads_remain_visible_in_application_spines() {
    for name in ["Opaque", "Reveal"] {
        let mut store = Store::default();
        let variable = store.fresh();
        let Type::Meta(meta) = variable else {
            unreachable!()
        };
        let constructor = Type::con("Test", name);
        store.set(meta, constructor.clone());
        let argument = Type::Var("a".to_owned());
        let application = Type::app(variable, argument.clone());
        let (head, args) = store.spine(&application);
        assert_eq!(
            head.as_ref(),
            &constructor,
            "top-only resolution hides {name}"
        );
        assert_eq!(
            args.iter().map(AsRef::as_ref).collect::<Vec<_>>(),
            vec![&argument]
        );
    }
}

#[test]
fn bound_partial_constructor_heads_prepend_their_existing_arguments() {
    let mut store = Store::default();
    let variable = store.fresh();
    let Type::Meta(meta) = variable else {
        unreachable!()
    };
    let constructor = Type::con("Test", "Task");
    let first = Type::Var("scope".to_owned());
    let second = Type::Var("result".to_owned());
    store.set(meta, Type::app(constructor.clone(), first.clone()));
    let application = Type::app(variable, second.clone());
    let (head, args) = store.spine(&application);
    assert_eq!(head.as_ref(), &constructor);
    assert_eq!(
        args.iter().map(AsRef::as_ref).collect::<Vec<_>>(),
        vec![&first, &second]
    );
}
