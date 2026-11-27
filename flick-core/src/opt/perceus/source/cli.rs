//! The standard CLI owns its immutable command metadata and consumes the
//! parser dictionaries and handler as Flick values. Only running the command
//! crosses the CLI host's boundary.
use flick_ir::memory::CliIntrinsic;
use flick_ir::{Foreign, ForeignMode, Shape};

use super::{Intrinsic, Name, kept_for_call};

const BINDINGS: &[(CliIntrinsic, &str, &str)] = &[
    (CliIntrinsic::Command, "command", "specCommand"),
    (CliIntrinsic::Description, "description", "specDescription"),
    (CliIntrinsic::Flag, "flag", "specFlag"),
    (CliIntrinsic::Arg, "arg", "specArg"),
    (CliIntrinsic::Subcommand, "subcommand", "specSubcommand"),
    (CliIntrinsic::Handle, "handle", "specHandle"),
    (CliIntrinsic::Group, "group", "specGroup"),
    (CliIntrinsic::Required, "required", "multiRequired"),
    (CliIntrinsic::Optional, "optional", "multiOptional"),
    (CliIntrinsic::Many, "many", "multiMany"),
    (CliIntrinsic::RunVersion, "run_version", "cliRunVersion"),
];

pub(super) fn intrinsic(value: &Foreign, name: &Name) -> Option<Intrinsic> {
    if name.module != "Cli" || value.binding.from() != Some("flick:cli") {
        return None;
    }
    let &(operation, _, _) = BINDINGS.iter().find(|(_, local, host)| {
        value.source_name.as_deref().unwrap_or(&name.name) == *local
            && value.binding.export_name() == Some(*host)
    })?;
    let runtime = operation == CliIntrinsic::RunVersion;
    let dicts = if matches!(operation, CliIntrinsic::Flag | CliIntrinsic::Arg) {
        3
    } else {
        0
    };
    let arity = operation.arity() - dicts;
    let adapter = kept_for_call(&value.adapter);
    if value.dicts != dicts
        || value.arity != arity
        || value.mode
            != if runtime {
                ForeignMode::Effect
            } else {
                ForeignMode::Pure
            }
        || value.stages.len() != arity
        || value.catch.is_some()
        || !adapter.words.is_empty()
        || !adapter.optional.is_empty()
        || !adapter.owned.is_empty()
        || adapter.retention.is_some()
        || adapter.result.nullable
        || adapter.result.unit != runtime
        || adapter.copies.result
        || !adapter.result_shape.is_data()
        || adapter.callbacks.len() != arity
        || adapter.copies.args.len() != arity
        || adapter.copies.callbacks.len() != arity
        || adapter.shapes.len() != arity
    {
        return None;
    }
    for index in 0..arity {
        let callback = operation == CliIntrinsic::Handle && index == 0;
        let copy = matches!(
            (operation, index),
            (CliIntrinsic::Flag, 1) | (CliIntrinsic::RunVersion, 2)
        );
        let valid_shape = if callback {
            matches!(&adapter.shapes[index], Shape::Fun { args, result, runtime: true, sync: false }
                if args == &[Shape::Data] && result.is_data())
        } else {
            adapter.shapes[index].is_data()
        };
        let valid_callback_copy = match &adapter.copies.callbacks[index] {
            Some((args, result)) => callback && args == &[true] && !result,
            None => !callback,
        };
        if value.stages[index] != (runtime && index + 1 == arity)
            || adapter.callbacks[index] != callback.then_some(1)
            || adapter.copies.args[index] != copy
            || !valid_shape
            || !valid_callback_copy
        {
            return None;
        }
    }
    Some(Intrinsic::Cli(operation))
}

#[cfg(test)]
mod tests {
    use flick_ir::{BackendAdapter, FileId, Span};

    use crate::build::builtin_modules;
    use crate::ir::{ItemKind, memory};

    use super::{BINDINGS, Intrinsic, Name, intrinsic};

    #[test]
    fn audited_cli_bindings_keep_their_physical_arity() {
        let module = builtin_modules()
            .iter()
            .find(|module| module.name == "Cli")
            .expect("builtin Cli");
        for &(operation, local, host) in BINDINGS {
            let binding = module.items.iter().find_map(|item| match &item.kind {
                ItemKind::Foreign(value) if value.name == local => Some(value),
                _ => None,
            });
            let binding = binding.expect("standard CLI binding");
            let name = Name::new("Cli", local);
            assert_eq!(binding.binding.export_name(), Some(host));
            assert_eq!(intrinsic(binding, &name), Some(Intrinsic::Cli(operation)));

            let program = super::super::lower([module], &name).expect("lower the standard binding");
            assert_eq!(memory::validate_program(&program), Ok(()));
            assert!(program.functions.values().any(|function| {
                function.body.instructions.iter().any(|instruction| {
                    matches!(instruction, memory::Instruction::Let {
                        value: memory::Operation::Intrinsic { kind: Intrinsic::Cli(actual), args }, ..
                    } if *actual == operation && args.len() == operation.arity())
                })
            }));

            let mut wrong = binding.clone();
            wrong.adapter.words.push(BackendAdapter {
                name: "cancel".into(),
                args: vec![],
                span: Span::new(FileId::new(0), 0, 0),
            });
            assert_eq!(intrinsic(&wrong, &name), None);
            assert_eq!(intrinsic(binding, &Name::new("App", local)), None);
        }
    }
}
