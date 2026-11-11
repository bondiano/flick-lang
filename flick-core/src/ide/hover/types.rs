use std::fmt::Write;

use super::{Ide, Source, as_written, tree};
use crate::ide::Target;
use crate::iface::Entity;
use crate::source::FileId;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{Form, ModuleSignature, TypeDef, show_type};

impl Ide<'_> {
    pub(super) fn type_hover(
        &self,
        file: FileId,
        entity: &Entity,
        source: Option<&Source>,
        definition: Option<&TypeDef>,
        range: &mut (u32, u32),
    ) -> Option<String> {
        let local_alias = self
            .analysis
            .module(&entity.module)
            .is_some_and(|m| m.typing.aliases.contains_key(&entity.name));
        let public_alias = definition.is_some_and(|d| matches!(&d.form, Form::Alias { .. }));
        if (local_alias || public_alias)
            && let Some(source) = source
            && let Some(decl) = source.tree.nodes().find(|d| {
                d.kind == K::TypeDecl
                    && tree::name_of(d, K::Con, &source.text) == Some(entity.name.as_str())
            })
        {
            if source.target == Target::File(file)
                && decl.child_tokens().any(|t| (t.start, t.end) == *range)
            {
                range.1 = decl
                    .nodes()
                    .filter(|n| n.kind == K::TypeParam)
                    .last()
                    .map_or(range.1, |n| n.range().1);
            }
            return Some(as_written(decl, &source.text));
        }
        let definition = definition?;
        Some(match &definition.form {
            Form::Kind => source
                .and_then(|source| {
                    source
                        .tree
                        .nodes()
                        .find(|d| {
                            d.kind == K::KindDecl
                                && tree::name_of(d, K::Con, &source.text)
                                    == Some(entity.name.as_str())
                        })
                        .map(|decl| as_written(decl, &source.text))
                })
                .unwrap_or_else(|| format!("kind {}", entity.name)),
            Form::KindMember => format!("{} : {}", entity.name, definition.kind),
            Form::Alias { params, body, .. } => {
                let params = params.iter().fold(String::new(), |mut acc, p| {
                    let _ = write!(acc, " {p}");
                    acc
                });
                format!("type alias {}{params} = {}", entity.name, show_type(body))
            }
            Form::Error => format!("type error {}", entity.name),
            Form::Service { .. } => format!("service {}", entity.name),
            Form::ModuleContract { .. } | Form::ModuleSignature { .. } => {
                if let Some(source) = source
                    && let Some(decl) = source.tree.nodes().find(|decl| {
                        decl.kind == K::ModuleTypeDecl
                            && tree::name_of(decl, K::Con, &source.text)
                                == Some(entity.name.as_str())
                    })
                {
                    return Some(as_written(decl, &source.text));
                }
                let signature = match &definition.form {
                    Form::ModuleContract { requirements } => ModuleSignature::Structure {
                        requirements: requirements.clone(),
                    }
                    .display(),
                    Form::ModuleSignature { signature } => signature.display(),
                    _ => unreachable!("module signature form"),
                };
                format!("type module {} =\n{signature}", entity.name)
            }
            _ => format!("type {} : {}", entity.name, definition.kind),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::ide::tests::fixture_of;
    use crate::iface::Namespace;

    #[test]
    fn a_module_signature_hover_without_source_retains_dependent_names() {
        let fixture = fixture_of(&[(
            "src/App/Contracts.fl",
            "module App.Contracts exposing (Factory)\ntype module Input =\n  type Key\ntype module Output =\n  type Value\ntype module Factory = (K : Input) -> Output with type Value = K.Key\n",
        )]);
        let ide = fixture.ide();
        let module = ide.analysis.module("App.Contracts").expect("contracts");
        let factory = module
            .interface
            .find(Namespace::Type, "Factory")
            .expect("factory");
        let shown = ide
            .type_hover(
                module.file,
                &factory.entity,
                None,
                factory.definition.as_ref(),
                &mut (0, 0),
            )
            .expect("hover");
        assert!(shown.contains("type module Factory"), "{shown}");
        assert!(shown.contains("K.Key"), "{shown}");
        assert!(!shown.contains("$module:"), "{shown}");
    }
}
