//! Pure transport of production inference occurrences across lexical binders.

use super::{Kind, Scope, Type, WitnessScope};
use crate::types::ty::{LexicalCoordinate, LexicalScope};
use crate::types::{Atom, Binder, Elem, MetaOccurrence};

fn map_kind_metas(
    kind: &Kind,
    map: &mut impl FnMut(&MetaOccurrence, bool) -> MetaOccurrence,
) -> Kind {
    match kind {
        Kind::Meta(meta) => Kind::Meta(map(meta, true)),
        Kind::Arrow(a, b) => Kind::arrow(map_kind_metas(a, map), map_kind_metas(b, map)),
        Kind::Row(inner) => Kind::Row(Box::new(map_kind_metas(inner, map))),
        _ => kind.clone(),
    }
}

fn map_label(label: &mut Elem, map: &mut impl FnMut(&MetaOccurrence, bool) -> MetaOccurrence) {
    match label {
        Elem::Capture(Atom::Meta(meta))
        | Elem::Service(Atom::Meta(meta))
        | Elem::Error(Atom::Meta(meta)) => *meta = map(meta, false),
        Elem::Provided(inner) => map_label(inner, map),
        _ => {}
    }
}

pub(in crate::types) fn map_type_metas(
    ty: &Type,
    map: &mut impl FnMut(&MetaOccurrence, bool) -> MetaOccurrence,
) -> Type {
    let mut mapped = ty.map_children(&mut |child| map_type_metas(child, map));
    match &mut mapped {
        Type::Meta(meta) => *meta = map(meta, false),
        Type::Forall { vars, .. } => {
            for binder in vars {
                binder.kind = map_kind_metas(&binder.kind, map);
            }
        }
        Type::Row(row) | Type::Record(row) => {
            for (label, _) in &mut row.entries {
                map_label(label, map);
            }
            row.sort();
        }
        Type::Without { gone, .. } => {
            for label in gone.iter_mut() {
                map_label(label, map);
            }
            gone.sort();
        }
        _ => {}
    }
    mapped
}

pub(in crate::types) fn map_type(
    ty: &Type,
    map: &mut impl FnMut(&LexicalCoordinate) -> Option<LexicalCoordinate>,
) -> Type {
    map_type_metas(ty, &mut |meta, _| meta.map_lexical(&mut *map))
}

pub(in crate::types) fn rebase_targets(
    ty: &Type,
    mappings: &[(LexicalScope, LexicalScope)],
) -> Type {
    map_type(ty, &mut |coordinate| {
        let mut coordinate = coordinate.clone();
        if let Some((_, target)) = mappings
            .iter()
            .find(|(from, _)| from.id == coordinate.target.id)
        {
            coordinate.target = target.clone();
        }
        Some(coordinate)
    })
}

fn names(ty: &Type, mappings: &[(&str, &str)]) -> Type {
    WitnessScope {
        from: "",
        to: "",
        mappings,
        shadow: None,
        changed: false,
    }
    .ty(ty)
}

fn namespace_occurs(ty: &Type, namespace: &str) -> bool {
    let mut occupied = super::witness_scope_occurs(ty, namespace);
    visit_type_metas(ty, &mut |meta, _| {
        if let Some(lexical) = meta.lexical() {
            occupied |= lexical.view.iter().any(|coordinate| {
                coordinate.target.namespace == namespace
                    || coordinate
                        .target
                        .namespace
                        .strip_prefix(namespace)
                        .is_some_and(|suffix| suffix.starts_with('.'))
            });
        }
    });
    occupied
}

fn avoid_capture(ty: &Type, mappings: &[(LexicalScope, LexicalScope)]) -> Type {
    if let Type::Exists {
        scope,
        binder,
        contract,
        body,
    } = ty
    {
        let contract = avoid_capture(contract, mappings);
        let closed = format!("$exists:{binder}");
        let mut body = (**body).clone();
        let collision = mappings.iter().any(|(from, to)| {
            if to.namespace != closed || from.id == scope.identity() {
                return false;
            }
            let mut viewed = false;
            map_type(&body, &mut |coordinate| {
                viewed |= coordinate.target.id == from.id;
                Some(coordinate.clone())
            });
            viewed
                || (from.namespace != closed && super::witness_scope_occurs(&body, &from.namespace))
        });
        let mut binder = binder.clone();
        if collision {
            let mut index = 0;
            let renamed = loop {
                let candidate = format!("{binder}$Closed{index}");
                let namespace = format!("$exists:{candidate}");
                if !namespace_occurs(&body, &namespace)
                    && !body.contains(
                        &mut |ty| matches!(ty, Type::Exists { binder, .. } if binder == &candidate),
                    )
                    && mappings
                        .iter()
                        .all(|(from, to)| from.namespace != namespace && to.namespace != namespace)
                {
                    break candidate;
                }
                index += 1;
            };
            let namespace = format!("$exists:{renamed}");
            body = super::open_existential_type(&body, &binder, &namespace);
            body = rebase_targets(
                &body,
                &[(
                    LexicalScope {
                        id: scope.identity(),
                        namespace: closed,
                    },
                    LexicalScope {
                        id: scope.identity(),
                        namespace,
                    },
                )],
            );
            binder = renamed;
        }
        return Type::Exists {
            scope: *scope,
            binder,
            contract: Box::new(contract),
            body: Box::new(avoid_capture(&body, mappings)),
        };
    }
    ty.map_children(&mut |child| avoid_capture(child, mappings))
}

pub(in crate::types) fn transport_type(
    ty: &Type,
    mappings: &[(LexicalScope, LexicalScope)],
) -> Type {
    if mappings.iter().all(|(from, to)| from == to) {
        return ty.clone();
    }
    // Temporary namespaces are fresh against the inputs and never escape this
    // operation. Two phases make name rebasing simultaneous, including swaps.
    let mut result = avoid_capture(ty, mappings);
    let mut index = 0;
    let mut temporaries = Vec::new();
    while temporaries.len() < mappings.len() {
        let candidate = format!("$lexical-transport:{index}");
        index += 1;
        let mut occupied = namespace_occurs(ty, &candidate);
        occupied |= mappings.iter().any(|(from, to)| {
            [from, to].iter().any(|scope| {
                scope.namespace == candidate
                    || scope
                        .namespace
                        .strip_prefix(&candidate)
                        .is_some_and(|suffix| suffix.starts_with('.'))
            })
        });
        if !occupied {
            temporaries.push(candidate);
        }
    }
    let first = mappings
        .iter()
        .zip(&temporaries)
        .map(|((from, _), temp)| (from.namespace.as_str(), temp.as_str()))
        .collect::<Vec<_>>();
    result = names(&result, &first);
    let second = mappings
        .iter()
        .zip(&temporaries)
        .map(|((_, to), temp)| (temp.as_str(), to.namespace.as_str()))
        .collect::<Vec<_>>();
    result = names(&result, &second);
    rebase_targets(&result, mappings)
}

pub(in crate::types) fn transport_kind(
    kind: &Kind,
    mappings: &[(LexicalScope, LexicalScope)],
) -> Kind {
    let carrier = Type::Forall {
        vars: vec![Binder {
            name: "$kind".into(),
            kind: kind.clone(),
        }],
        constraints: Vec::new(),
        body: Box::new(Type::Error),
    };
    let Type::Forall { vars, .. } = transport_type(&carrier, mappings) else {
        unreachable!("kind carrier")
    };
    vars[0].kind.clone()
}

pub(in crate::types) fn open_type(
    ty: &Type,
    binder: &str,
    closed: Scope,
    opened: &LexicalScope,
) -> Type {
    transport_type(
        ty,
        &[(
            LexicalScope {
                id: closed.identity(),
                namespace: format!("$exists:{binder}"),
            },
            opened.clone(),
        )],
    )
}

pub(in crate::types) fn close_type(
    ty: &Type,
    opened: &LexicalScope,
    binder: &str,
    closed: Scope,
) -> Type {
    transport_type(
        ty,
        &[(
            opened.clone(),
            LexicalScope {
                id: closed.identity(),
                namespace: format!("$exists:{binder}"),
            },
        )],
    )
}

pub(in crate::types) fn visit_type_metas(ty: &Type, visit: &mut impl FnMut(&MetaOccurrence, bool)) {
    fn kind(value: &Kind, visit: &mut impl FnMut(&MetaOccurrence, bool)) {
        match value {
            Kind::Meta(meta) => visit(meta, true),
            Kind::Arrow(a, b) => {
                kind(a, visit);
                kind(b, visit);
            }
            Kind::Row(inner) => kind(inner, visit),
            _ => {}
        }
    }
    fn label(value: &Elem, visit: &mut impl FnMut(&MetaOccurrence, bool)) {
        match value {
            Elem::Capture(Atom::Meta(meta))
            | Elem::Error(Atom::Meta(meta))
            | Elem::Service(Atom::Meta(meta)) => visit(meta, false),
            Elem::Provided(inner) => label(inner, visit),
            _ => {}
        }
    }
    match ty {
        Type::Meta(meta) => visit(meta, false),
        Type::Forall { vars, .. } => {
            for binder in vars {
                kind(&binder.kind, visit);
            }
        }
        Type::Row(row) | Type::Record(row) => {
            for (element, _) in &row.entries {
                label(element, visit);
            }
        }
        Type::Without { gone, .. } => {
            for element in gone {
                label(element, visit);
            }
        }
        _ => {}
    }
    ty.for_each_child(&mut |child| visit_type_metas(child, visit));
}
