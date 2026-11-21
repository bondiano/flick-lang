use std::collections::BTreeSet;
use std::mem;

use super::*;
use crate::types::env::{ModuleNominalKind, ModuleRequirement as R};
use crate::types::module_contract::context::Embedding;
use crate::types::services::Operation;
use crate::types::ty::{Binder, MetaOccurrence, Row, Scope, Written};
use crate::types::unify::test_checker;
use crate::types::{Elem, OpClass};

fn member(name: &str, manifest: Option<Type>) -> R {
    R::Type {
        name: name.into(),
        params: Vec::new(),
        manifest,
    }
}

fn define(checker: &mut Checker<'_>, requirements: Vec<R>) {
    checker.env.define_type(
        "S",
        TypeDef {
            kind: Kind::Module,
            form: Form::ModuleContract { requirements },
            variance: Vec::new(),
        },
    );
}

fn service(operations: Vec<Operation>) -> R {
    R::Nominal {
        name: "Clock".into(),
        category: ModuleNominalKind::Service,
        identity: None,
        definition: Some(Box::new(TypeDef {
            kind: Kind::Service,
            form: Form::Service {
                params: Vec::new(),
                operations,
                effect: false,
            },
            variance: Vec::new(),
        })),
        methods: Vec::new(),
    }
}

include!("tests/refresh.rs");
include!("tests/paired_registration.rs");
include!("tests/live_evidence.rs");
