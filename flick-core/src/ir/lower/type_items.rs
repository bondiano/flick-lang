//! Lowering type and error declarations to constructor items.

use super::{Item, ItemKind, Lower, enums};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{Abi, Constructor, Rep, is_type, written_dicts};

impl Lower<'_> {
    /// The labels of the one field of `variant`, in canonical UTF-16 order,
    /// when that field is written as a closed record.
    fn record_field(&self, variant: &Node) -> Option<Vec<String>> {
        let mut fields = variant.nodes().filter(|n| is_type(n.kind));
        let (record, None) = (fields.next()?, fields.next()) else {
            return None;
        };
        if record.kind != K::RecordType {
            return None;
        }
        let label = |field: &Node| {
            (field.kind == K::RecordTypeField).then(|| self.token_text(field, K::Ident))?
        };
        // a tail, or a field without a name, and the labels are not all known
        let mut labels: Vec<String> = record.nodes().map(label).collect::<Option<_>>()?;
        flick_ir::sort_strings_utf16(&mut labels);
        Some(labels)
    }

    /// The constructors of a type or error; its
    /// witness `T$schema` is the code std's plugin derives.
    pub(super) fn type_items(&mut self, decl: &Node, span: Span, items: &mut Vec<Item>) {
        let Some(ty) = self.token_text(decl, K::Con) else {
            return;
        };
        let abi = self.typing.abis.get(&ty).cloned().unwrap_or_default();
        let mut constructors = Vec::new();
        for (i, variant) in decl.nodes().filter(|n| n.kind == K::Variant).enumerate() {
            let Some(name) = self.token_text(variant, K::Con) else {
                continue;
            };
            let fields = self.ir_ctor_fields(&name);
            let constructor = Constructor {
                module: self.module.to_owned(),
                ty: ty.clone(),
                name,
                arity: variant.nodes().filter(|n| is_type(n.kind)).count(),
                rep: abi.rep(i),
                dicts: written_dicts(variant),
                fields,
                type_args: Vec::new(),
            };
            if let Some(labels) = self.record_field(variant) {
                self.records.insert(constructor.name.clone(), labels);
            }
            constructors.push(constructor.clone());
            items.push(Item {
                kind: ItemKind::Constructor(constructor),
                span: self.span(variant),
            });
        }
        if matches!(abi, Abi::Literal(_)) {
            items.extend(enums::companions(&ty, &constructors, span));
        }
        // `type error E` without fields still has its constructor
        if decl.kind == K::ErrorDecl && !decl.nodes().any(|n| n.kind == K::Variant) {
            let constructor = Constructor {
                module: self.module.to_owned(),
                ty: ty.clone(),
                name: ty,
                arity: 0,
                rep: Rep::Tagged,
                dicts: 0,
                fields: Vec::new(),
                type_args: Vec::new(),
            };
            items.push(Item {
                kind: ItemKind::Constructor(constructor),
                span,
            });
        }
    }
}
