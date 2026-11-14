//! Exact private inference carriers, deliberately separate from semantic Eq.

use super::{MetaOccurrence, ScopeId, Type};
use crate::types::env::{Form, ModuleRequirement as R, ModuleSignature, TypeDef};
use crate::types::module_signature::map_definition_types;
use crate::types::{Atom, Elem, Kind};

#[derive(Clone)]
pub(in crate::types) enum Stamp {
    Scope(ScopeId),
    TypeRoot(u32),
    KindRoot(u32),
    TypeView(MetaOccurrence),
    KindView(MetaOccurrence),
}

impl PartialEq for Stamp {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Scope(a), Self::Scope(b)) => a == b,
            (Self::TypeRoot(a), Self::TypeRoot(b)) | (Self::KindRoot(a), Self::KindRoot(b)) => {
                a == b
            }
            (Self::TypeView(a), Self::TypeView(b)) | (Self::KindView(a), Self::KindView(b)) => {
                a.same_carrier(b)
            }
            _ => false,
        }
    }
}

pub(in crate::types) fn definition_stamps(definition: &TypeDef) -> Vec<Stamp> {
    let mut stamps = Vec::new();
    definition_kinds(definition, &mut stamps);
    let mut copy = definition.clone();
    map_definition_types(&mut copy, &mut |ty| {
        type_stamps(ty, &mut stamps);
        ty.clone()
    });
    stamps
}

pub(in crate::types) fn signature_stamps(signature: &ModuleSignature) -> Vec<Stamp> {
    let mut stamps = Vec::new();
    signature_kinds(signature, &mut stamps);
    let mut copy = signature.clone();
    copy.map_types(&mut |ty| {
        type_stamps(ty, &mut stamps);
        ty.clone()
    });
    stamps
}

pub(in crate::types) fn stamps_of_type(ty: &Type) -> Vec<Stamp> {
    let mut stamps = Vec::new();
    type_stamps(ty, &mut stamps);
    stamps
}

fn kind_stamps(kind: &Kind, out: &mut Vec<Stamp>) {
    match kind {
        Kind::Meta(meta) => out.push(match meta.legacy_root() {
            Some(root) => Stamp::KindRoot(root),
            None => Stamp::KindView(meta.clone()),
        }),
        Kind::Arrow(a, b) => {
            kind_stamps(a, out);
            kind_stamps(b, out);
        }
        Kind::Row(inner) => kind_stamps(inner, out),
        _ => {}
    }
}

fn label_stamps(label: &Elem, out: &mut Vec<Stamp>) {
    match label {
        Elem::Capture(Atom::Meta(meta))
        | Elem::Error(Atom::Meta(meta))
        | Elem::Service(Atom::Meta(meta)) => out.push(match meta.legacy_root() {
            Some(root) => Stamp::TypeRoot(root),
            None => Stamp::TypeView(meta.clone()),
        }),
        Elem::Provided(inner) => label_stamps(inner, out),
        _ => {}
    }
}

fn type_stamps(ty: &Type, out: &mut Vec<Stamp>) {
    ty.visit(&mut |ty| match ty {
        Type::Meta(meta) => out.push(match meta.legacy_root() {
            Some(root) => Stamp::TypeRoot(root),
            None => Stamp::TypeView(meta.clone()),
        }),
        Type::Exists { scope, .. } => out.push(Stamp::Scope(scope.identity())),
        Type::Forall { vars, .. } => {
            for variable in vars {
                kind_stamps(&variable.kind, out);
            }
        }
        Type::Record(row) | Type::Row(row) => {
            for (label, _) in &row.entries {
                label_stamps(label, out);
            }
        }
        Type::Without { gone, .. } => {
            for label in gone {
                label_stamps(label, out);
            }
        }
        _ => {}
    });
}

fn definition_kinds(definition: &TypeDef, out: &mut Vec<Stamp>) {
    kind_stamps(&definition.kind, out);
    match &definition.form {
        Form::ModuleContract { requirements } => requirement_kinds(requirements, out),
        Form::ModuleSignature { signature } => signature_kinds(signature, out),
        _ => {}
    }
}

fn signature_kinds(signature: &ModuleSignature, out: &mut Vec<Stamp>) {
    match signature {
        ModuleSignature::Structure { requirements } => requirement_kinds(requirements, out),
        ModuleSignature::Parameterized {
            parameter, result, ..
        } => {
            signature_kinds(parameter, out);
            signature_kinds(result, out);
        }
    }
}

fn requirement_kinds(requirements: &[R], out: &mut Vec<Stamp>) {
    for requirement in requirements {
        match requirement {
            R::Type { params, .. } => {
                for parameter in params {
                    kind_stamps(&parameter.kind, out);
                }
            }
            R::Kind { definition, .. }
            | R::Nominal {
                definition: Some(definition),
                ..
            } => definition_kinds(definition, out),
            R::Module { signature, .. } => signature_kinds(signature, out),
            _ => {}
        }
    }
}
