//! Conversion from `flick-model` types and schemes to `flick-core` types for
//! hot-swap differential testing.

use flick_core::ir::memory::Name;
use flick_core::types::{
    Atom as CoreAtom, Binder as CoreBinder, Effects as CoreEffects, Elem as CoreElem, Elided,
    Kind as CoreKind, Row as CoreRow, Type as CoreType,
};
use flick_model::syntax::Path;
use flick_model::types::{Caps, Kind, Row, Scheme, Stage, Type, Var};

pub(super) fn path_to_name(p: &Path) -> Name {
    Name::new(&p.module, &p.name)
}

fn convert_kind(k: &Kind) -> CoreKind {
    match k {
        Kind::Type => CoreKind::Type,
        Kind::Region => CoreKind::Region,
        Kind::Captures => CoreKind::CaptureRow,
        Kind::Services => CoreKind::ServiceRow,
        Kind::Errors => CoreKind::ErrorRow,
        Kind::Fields => CoreKind::Row(Box::new(CoreKind::Type)),
        Kind::Arrow(a, b) => CoreKind::arrow(convert_kind(a), convert_kind(b)),
    }
}

fn convert_var(v: &Var) -> CoreType {
    match v {
        Var::Named(n) => CoreType::Var(n.to_string()),
        Var::Meta(m) => CoreType::Var(format!("m{m}")),
    }
}

fn convert_row(row: &Row) -> CoreType {
    let mut entries: Vec<(CoreElem, CoreType)> = row
        .elems
        .keys()
        .map(|p| {
            (
                CoreElem::Named(format!("{}.{}", p.module, p.name)),
                CoreType::con(&p.module, &p.name),
            )
        })
        .collect();
    CoreRow::sort_entries(&mut entries);
    let tail = row.vars.iter().next().map(|v| Box::new(convert_var(v)));
    CoreType::Row(CoreRow {
        entries,
        tail,
        ..CoreRow::default()
    })
}

fn convert_caps(caps: &Caps) -> CoreType {
    let mut entries: Vec<(CoreElem, CoreType)> = caps
        .atoms
        .iter()
        .map(|v| {
            let name = match v {
                Var::Named(n) => n.to_string(),
                Var::Meta(m) => format!("m{m}"),
            };
            (
                CoreElem::Capture(CoreAtom::Var(name.clone())),
                CoreType::Var(name),
            )
        })
        .collect();
    CoreRow::sort_entries(&mut entries);
    let tail = caps.vars.iter().next().map(|v| Box::new(convert_var(v)));
    CoreType::Row(CoreRow {
        entries,
        tail,
        ..CoreRow::default()
    })
}

pub(super) fn convert_type(ty: &Type) -> CoreType {
    match ty {
        Type::Con(path) => CoreType::con(&path.module, &path.name),
        Type::Var(Var::Named(n)) => CoreType::Var(n.to_string()),
        Type::Var(Var::Meta(m)) => CoreType::Var(format!("m{m}")),
        Type::App(h, a) => CoreType::app(convert_type(h), convert_type(a)),
        Type::Tuple(elems) => CoreType::Tuple(elems.iter().map(convert_type).collect()),
        Type::Record(fields) => {
            let mut entries: Vec<(CoreElem, CoreType)> = fields
                .fields
                .iter()
                .map(|(k, v)| (CoreElem::Named(k.to_string()), convert_type(v)))
                .collect();
            CoreRow::sort_entries(&mut entries);
            let tail = fields.tail.as_ref().map(|v| Box::new(convert_var(v)));
            CoreType::Record(CoreRow {
                entries,
                tail,
                ..CoreRow::default()
            })
        }
        Type::Fun(param, stage, result) => {
            let effects = match stage {
                Stage::Pure => None,
                Stage::Runtime(s, e) => {
                    Some(Box::new(CoreEffects::new(convert_row(s), convert_row(e))))
                }
            };
            CoreType::Fun {
                param: Box::new(convert_type(param)),
                effects,
                result: Box::new(convert_type(result)),
            }
        }
        Type::Captured(inner, caps) => {
            if caps.is_empty() {
                convert_type(inner)
            } else {
                CoreType::Captured {
                    ty: Box::new(convert_type(inner)),
                    captures: Box::new(convert_caps(caps)),
                    value: false,
                    elided: Elided::default(),
                }
            }
        }
        other => panic!("unsupported model type in hot-swap diff test: {other:?}"),
    }
}

pub(super) fn convert_scheme(scheme: &Scheme) -> CoreType {
    let body = convert_type(&scheme.body);
    if scheme.vars.is_empty() {
        body
    } else {
        CoreType::Forall {
            vars: scheme
                .vars
                .iter()
                .map(|(name, kind)| CoreBinder {
                    name: name.to_string(),
                    kind: convert_kind(kind),
                })
                .collect(),
            constraints: Vec::new(),
            body: Box::new(body),
        }
    }
}
