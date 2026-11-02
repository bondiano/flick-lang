use std::fmt::Write;

use super::{Ide, Source, as_written, tree};
use crate::ide::Target;
use crate::iface::Entity;
use crate::source::FileId;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{Form, TypeDef, show_type};

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
            _ => format!("type {} : {}", entity.name, definition.kind),
        })
    }
}
