use std::cell::RefCell;
use std::collections::BTreeMap;

use super::*;
use crate::types::Atom;

#[derive(Default)]
struct Kinds {
    constructors: BTreeMap<(String, String), Kind>,
    solutions: RefCell<BTreeMap<u32, Kind>>,
}

impl Kinding for Kinds {
    fn resolve_kind(&self, kind: &Kind) -> Kind {
        match kind {
            Kind::Meta(root) => self
                .solutions
                .borrow()
                .get(root)
                .cloned()
                .unwrap_or_else(|| kind.clone()),
            _ => kind.clone(),
        }
    }

    fn head_kind(&self, ty: &Type, binders: &[Binder]) -> Result<Kind, Incomplete> {
        match ty {
            Type::Con { module, name } => self
                .constructors
                .get(&(module.clone(), name.clone()))
                .cloned()
                .ok_or_else(|| Incomplete::Head(ty.clone())),
            Type::Var(name) => binders
                .iter()
                .rev()
                .find(|binder| binder.name == *name)
                .map(|binder| binder.kind.clone())
                .ok_or_else(|| Incomplete::Head(ty.clone())),
            Type::App { head, .. } => match self.resolve_kind(&self.head_kind(head, binders)?) {
                Kind::Arrow(_, result) => Ok(*result),
                kind => Err(Incomplete::NotFunction(kind)),
            },
            Type::Tuple(_) => Ok(Kind::Type),
            _ => Err(Incomplete::Head(ty.clone())),
        }
    }
}

struct Rename;

impl Identities for Rename {
    fn ty(&mut self, ty: &Type, _: &[Bound]) -> Type {
        match ty {
            Type::Con { module, name } if module == "W" => Type::con("X", name),
            _ => ty.clone(),
        }
    }

    fn kind(&mut self, kind: &Kind, _: &[Bound]) -> Kind {
        match kind {
            Kind::Named { module, name } if module == "W" => Kind::Named {
                module: "X".into(),
                name: name.clone(),
            },
            _ => kind.clone(),
        }
    }

    fn semantic_label(&mut self, label: &Elem, _: &[Bound]) -> Elem {
        match label {
            Elem::Named(name) if name == "W.E" => Elem::named("X.E"),
            Elem::Provided(inner) => Elem::Provided(Box::new(self.semantic_label(inner, &[]))),
            _ => label.clone(),
        }
    }
}

fn kinds() -> Kinds {
    let mut kinds = Kinds::default();
    kinds
        .constructors
        .insert(("W".into(), "E".into()), Kind::Type);
    for name in ["Fields", "Alias", "Opaque"] {
        kinds
            .constructors
            .insert(("G".into(), name.into()), Kind::arrow(fields(), Kind::Type));
    }
    kinds.constructors.insert(
        ("G".into(), "Errors".into()),
        Kind::arrow(Kind::ErrorRow, Kind::Type),
    );
    kinds.constructors.insert(
        ("G".into(), "Curried".into()),
        Kind::arrows([Kind::Type, fields()].into_iter(), Kind::Type),
    );
    kinds
}

fn fields() -> Kind {
    Kind::Row(Box::new(Kind::Type))
}
fn row() -> Type {
    Type::Row(Row::closed(vec![(Elem::named("W.E"), Type::con("W", "E"))]))
}

#[test]
fn identical_row_data_has_distinct_field_and_error_identity_positions() {
    let kinds = kinds();
    let Type::Row(field) = map(&row(), &fields(), &kinds, &mut Rename).expect("field row") else {
        panic!("row")
    };
    let Type::Row(error) = map(&row(), &Kind::ErrorRow, &kinds, &mut Rename).expect("error row")
    else {
        panic!("row")
    };
    assert_eq!(field.entries[0], (Elem::named("W.E"), Type::con("X", "E")));
    assert_eq!(error.entries[0], (Elem::named("X.E"), Type::con("X", "E")));
    let record = Type::Record(Row {
        entries: Vec::new(),
        tail: Some(Box::new(row())),
        ..Row::default()
    });
    let Type::Record(record) = map(&record, &Kind::Type, &kinds, &mut Rename).expect("record tail")
    else {
        panic!("record")
    };
    assert_eq!(*record.tail.expect("tail"), Type::Row(field));
}

#[test]
fn application_roles_come_from_declared_and_lexically_bound_constructor_kinds() {
    let kinds = kinds();
    for name in ["Fields", "Alias", "Opaque"] {
        let ty = Type::app(Type::con("G", name), row());
        let Type::App { arg, .. } =
            map(&ty, &Kind::Type, &kinds, &mut Rename).expect("declared kind")
        else {
            panic!("app")
        };
        let Type::Row(field) = *arg else {
            panic!("row")
        };
        assert_eq!(field.entries[0].0, Elem::named("W.E"));
    }
    let partial = Type::app(Type::con("G", "Curried"), Type::Tuple(Vec::new()));
    let ty = Type::Forall {
        vars: vec![Binder {
            name: "f".into(),
            kind: Kind::arrow(fields(), Kind::Type),
        }],
        constraints: Vec::new(),
        body: Box::new(Type::Tuple(vec![
            Type::app(Type::Var("f".into()), row()),
            Type::app(partial, row()),
        ])),
    };
    let Type::Forall { body, .. } =
        map(&ty, &Kind::Type, &kinds, &mut Rename).expect("higher-kinded positions")
    else {
        panic!("forall")
    };
    let Type::Tuple(items) = *body else {
        panic!("tuple")
    };
    for item in items {
        let Type::App { arg, .. } = item else {
            panic!("app")
        };
        let Type::Row(field) = *arg else {
            panic!("row")
        };
        assert_eq!(field.entries[0].0, Elem::named("W.E"));
    }
}

#[test]
fn unknown_kind_defers_and_a_later_solution_is_observed_without_a_frozen_role() {
    let kinds = kinds();
    let unknown = Kind::Meta(7);
    assert_eq!(
        map(&row(), &unknown, &kinds, &mut Rename),
        Err(Incomplete::Kind(unknown.clone()))
    );
    assert!(
        kinds.solutions.borrow().is_empty(),
        "reads do not default kinds"
    );
    kinds.solutions.borrow_mut().insert(7, fields());
    let Type::Row(field) = map(&row(), &unknown, &kinds, &mut Rename).expect("later kind solution")
    else {
        panic!("row")
    };
    assert_eq!(field.entries[0].0, Elem::named("W.E"));
    // Simulate rollback before a distinct later inference branch.
    kinds.solutions.borrow_mut().remove(&7);
    assert_eq!(
        map(&row(), &unknown, &kinds, &mut Rename),
        Err(Incomplete::Kind(unknown.clone()))
    );
    kinds.solutions.borrow_mut().insert(7, Kind::ErrorRow);
    let Type::Row(error) = map(&row(), &unknown, &kinds, &mut Rename).expect("fresh observation")
    else {
        panic!("row")
    };
    assert_eq!(error.entries[0].0, Elem::named("X.E"));
}

#[test]
fn omissions_observe_row_kind_while_frozen_atom_names_are_not_qualified_identities() {
    let kinds = kinds();
    let ty = Type::Without {
        row: Box::new(Type::Var("r".into())),
        gone: vec![
            Elem::Provided(Box::new(Elem::named("W.E"))),
            Elem::Capture(Atom::Var("W.E".into())),
            Elem::Error(Atom::Skolem(4)),
        ],
    };
    let Type::Without { gone: literal, .. } =
        map(&ty, &fields(), &kinds, &mut Rename).expect("literal omissions")
    else {
        panic!("without")
    };
    let Type::Without { gone: semantic, .. } =
        map(&ty, &Kind::ErrorRow, &kinds, &mut Rename).expect("semantic omissions")
    else {
        panic!("without")
    };
    assert_eq!(literal[0], Elem::Provided(Box::new(Elem::named("W.E"))));
    assert_eq!(semantic[0], Elem::Provided(Box::new(Elem::named("X.E"))));
    assert_eq!(&literal[1..], &semantic[1..]);
}

#[test]
fn live_payload_has_its_declared_capture_row_kind_only_inside_semantic_rows() {
    let kinds = kinds();
    let services = Type::Row(Row::closed(vec![(Elem::Live, row())]));
    let Type::Row(outer) =
        map(&services, &Kind::ServiceRow, &kinds, &mut Rename).expect("Live captures")
    else {
        panic!("service row")
    };
    let Type::Row(captures) = &outer.entries[0].1 else {
        panic!("capture payload")
    };
    assert_eq!(captures.entries[0].0, Elem::named("X.E"));
    let fields = Type::Row(Row::closed(vec![(Elem::named("W.E"), row())]));
    let Type::Row(fields) = map(
        &fields,
        &Kind::Row(Box::new(Kind::CaptureRow)),
        &kinds,
        &mut Rename,
    )
    .expect("literal field with capture-row value") else {
        panic!("field row")
    };
    assert_eq!(fields.entries[0].0, Elem::named("W.E"));
}
