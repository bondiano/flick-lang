//! Genuine checked declaration evidence, not an ABI/name allowlist.
use crate::causes;
use flick_core::types::lang::Lang;
use flick_core::{
    build,
    iface::{Interface, Namespace},
    ir::{
        ItemKind, Lit,
        memory::{Name, Program},
    },
    opt::perceus,
    project::Compiled,
    types::{Form, Kind, Type},
};
use flick_model::{
    memory::{Heap, ProgramOutcome, Value, execute_program_outcome},
    run,
};
use std::sync::Arc;
use std::{fmt::Write as _, path::Path};

fn unresolved_kind(kind: &Kind) -> Option<u32> {
    let mut pending = vec![kind];
    while let Some(kind) = pending.pop() {
        match kind {
            Kind::Meta(meta) if meta.legacy_root().is_some() => return meta.legacy_root(),
            Kind::Row(inner) => pending.push(inner),
            Kind::Arrow(input, output) => pending.extend([input.as_ref(), output.as_ref()]),
            _ => {}
        }
    }
    None
}
fn type_kinds(ty: &Type, path: &str, unresolved: &mut Vec<String>) {
    ty.visit(&mut |part| {
        if let Type::Forall { vars, .. } = part {
            for binder in vars {
                if let Some(id) = unresolved_kind(&binder.kind) {
                    unresolved.push(format!(
                        "{path}: forall {} has Kind::Meta({id})",
                        binder.name
                    ));
                }
            }
        }
    });
}

#[test]
fn checked_owner_policy_declarations_resolve_every_retained_kind() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/rt/owner_foreign_policy.fl");
    let modules = run::compile(&file).expect("existing owner policy source remains accepted");
    let mut unresolved = Vec::new();
    for interface in modules
        .iter()
        .map(|module| module.declarations.as_ref())
        .chain(build::builtin_declarations().iter().map(AsRef::as_ref))
    {
        for export in &interface.exports {
            let path = format!(
                "{}.{}, namespace {:?}",
                export.entity.module, export.entity.name, export.entity.namespace
            );
            if let Some(scheme) = &export.scheme {
                type_kinds(scheme, &path, &mut unresolved);
            }
            if export.entity.namespace == Namespace::Type
                && let Some(definition) = &export.definition
            {
                if let Some(id) = unresolved_kind(&definition.kind) {
                    unresolved.push(format!("{path}: definition Kind::Meta({id})"));
                }
                if let Form::Alias { body, .. } = &definition.form {
                    type_kinds(body, &path, &mut unresolved);
                }
            }
        }
    }
    assert!(
        unresolved.is_empty(),
        "accepted checked declarations retain unresolved kinds:\n{}",
        unresolved.join("\n")
    );
}

const BASIC: &str = r#"module App.ScalarContext exposing (convert, answer, plain, text_only)
convert : Int -> Float
foreign pure convert from global "Number"
text_only : String -> Float
foreign pure text_only from global "Number"
answer : Int -> Float
let answer value = convert value
plain : Unit -> Unit
let plain () = ()
"#;

fn facts(modules: &[Compiled]) -> Vec<Interface> {
    modules
        .iter()
        .map(|module| module.declarations.as_ref().clone())
        .collect()
}
fn lower(
    modules: &[Compiled],
    declarations: &[Interface],
    entry: &Name,
    release: bool,
) -> Result<Program, perceus::LowerError> {
    let ir: Vec<_> = modules
        .iter()
        .map(|module| {
            if release {
                build::optimized(modules, module, false)
            } else {
                (*module.ir).clone()
            }
        })
        .collect();
    perceus::lower_with_declarations(
        ir.iter().chain(build::builtin_modules()),
        declarations
            .iter()
            .chain(build::builtin_declarations().iter().map(AsRef::as_ref)),
        entry,
    )
}
fn returned_number(program: &Program, entry: &Name, literal: &Lit, expected: u64) {
    let mut heap = Heap::new();
    let outside = heap.literal(literal).expect("external numeric input");
    let argument = heap.dup(&outside).expect("owned input alias");
    let report = execute_program_outcome(&mut heap, program, entry, vec![argument], 100_000);
    let ProgramOutcome::Returned(Value::Float(value)) = report.outcome else {
        causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
    };
    assert_eq!(value.to_bits(), expected);
    if let Lit::BigInt(text) = literal {
        assert_eq!(heap.bigint(&outside), Ok(text.as_str()));
    }
    heap.drop_value(outside).expect("external owner");
    let stats = heap
        .finish()
        .expect("typed conversion roots explicitly released");
    assert_eq!(stats.live_objects, 0);
    assert_eq!(stats.allocations, stats.frees);
}
include!("memory_scalar_context/checked_aliases.rs");
include!("memory_scalar_context/source_provenance.rs");
