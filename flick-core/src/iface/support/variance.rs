//! Recursive validation of serialized parameter variance in interface graphs.

use crate::types::{
    Form, Kind, ModuleRequirement, ModuleSignature, TypeDef, valid_exported_variance,
    variance_param_kinds,
};

enum VarianceNode<'a> {
    Definition(&'a str, &'a str, &'a TypeDef),
    Signature(&'a str, &'a ModuleSignature),
    Requirement(&'a str, &'a ModuleRequirement),
}

pub(super) fn validate_definition_variances(
    module: &str,
    name: &str,
    definition: &TypeDef,
) -> Result<(), String> {
    validate_variance_nodes(vec![VarianceNode::Definition(module, name, definition)])
}

pub(super) fn validate_signature_variances(
    module: &str,
    signature: &ModuleSignature,
) -> Result<(), String> {
    validate_variance_nodes(vec![VarianceNode::Signature(module, signature)])
}

fn validate_variance_nodes(mut pending: Vec<VarianceNode<'_>>) -> Result<(), String> {
    while let Some(node) = pending.pop() {
        match node {
            VarianceNode::Definition(module, name, definition) => {
                validate_variance(module, name, definition)?;
                match &definition.form {
                    Form::ModuleContract { requirements } => pending.extend(
                        requirements
                            .iter()
                            .map(|req| VarianceNode::Requirement(module, req)),
                    ),
                    Form::ModuleSignature { signature } => {
                        pending.push(VarianceNode::Signature(module, signature));
                    }
                    _ => {}
                }
            }
            VarianceNode::Signature(module, signature) => match signature {
                ModuleSignature::Structure { requirements } => pending.extend(
                    requirements
                        .iter()
                        .map(|req| VarianceNode::Requirement(module, req)),
                ),
                ModuleSignature::Parameterized {
                    parameter, result, ..
                } => {
                    pending.push(VarianceNode::Signature(module, parameter));
                    pending.push(VarianceNode::Signature(module, result));
                }
            },
            VarianceNode::Requirement(module, requirement) => match requirement {
                ModuleRequirement::Module { signature, .. } => {
                    pending.push(VarianceNode::Signature(module, signature));
                }
                ModuleRequirement::Kind {
                    name,
                    identity,
                    definition,
                } => {
                    let (owner, member) = identity.as_ref().map_or((module, name.as_str()), |e| {
                        (e.module.as_str(), e.name.as_str())
                    });
                    pending.push(VarianceNode::Definition(owner, member, definition));
                }
                ModuleRequirement::Nominal {
                    name,
                    identity,
                    definition: Some(definition),
                    ..
                } => {
                    let (owner, member) = identity.as_ref().map_or((module, name.as_str()), |e| {
                        (e.module.as_str(), e.name.as_str())
                    });
                    pending.push(VarianceNode::Definition(owner, member, definition));
                }
                _ => {}
            },
        }
    }
    Ok(())
}

fn validate_variance(module: &str, name: &str, definition: &TypeDef) -> Result<(), String> {
    if definition.variance.is_empty() {
        return Ok(());
    }
    if !matches!(
        definition.form,
        Form::Nominal { .. } | Form::Adt { .. } | Form::Error
    ) {
        return Err(format!("unexpected parameter variance on {module}.{name}"));
    }
    let kinds: Vec<Kind> = variance_param_kinds(&definition.kind)
        .into_iter()
        .cloned()
        .collect();
    if valid_exported_variance(&kinds, &definition.variance).is_none() {
        return Err(format!("invalid parameter variance on {module}.{name}"));
    }
    Ok(())
}
