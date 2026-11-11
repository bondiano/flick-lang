/// A witness owns all of its nested structure identities, with a path boundary.
fn within_witness<'a>(module: &'a str, witness: &str) -> Option<&'a str> {
    let suffix = module.strip_prefix(witness)?;
    (suffix.is_empty() || suffix.starts_with('.')).then_some(suffix)
}

fn abstract_nominals(requirements: &[ModuleRequirement]) -> Vec<String> {
    let mut names = Vec::new();
    for requirement in requirements {
        match requirement {
            ModuleRequirement::Nominal {
                name,
                identity: None,
                ..
            } => names.push(name.clone()),
            ModuleRequirement::Module {
                name, signature, ..
            } => {
                if let super::ModuleSignature::Structure { requirements } = &**signature {
                    names.extend(
                        abstract_nominals(requirements)
                            .into_iter()
                            .map(|child| format!("{name}.{child}")),
                    );
                }
            }
            _ => {}
        }
    }
    names
}

fn packed_fields(
    requirements: &[ModuleRequirement],
    actual: &Interface,
    prefix: &str,
    fields: &mut Vec<(String, String, String)>,
) {
    for requirement in requirements {
        match requirement {
            ModuleRequirement::Value { name, .. } => {
                if let Some(export) = actual.find(Namespace::Value, name) {
                    fields.push((
                        format!("{prefix}{name}"),
                        export.entity.module.clone(),
                        export.entity.name.clone(),
                    ));
                }
            }
            ModuleRequirement::Module {
                name, signature, ..
            } => {
                if let super::ModuleSignature::Structure { requirements } = &**signature
                    && let Some(child) = actual.modules.get(name)
                {
                    packed_fields(requirements, child, &format!("{prefix}{name}."), fields);
                }
            }
            _ => {}
        }
    }
}
