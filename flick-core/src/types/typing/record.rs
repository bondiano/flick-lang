//! Recording typing facts on `Checker::typing` during type checking.

#[cfg(not(target_arch = "wasm32"))]
use super::Typing;
use super::{Range, Ref, range};
use crate::iface::Entity;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::Checker;
use crate::types::classes::Dict;
use crate::types::kinds::Kind;
use crate::types::ty::Type;
use crate::types::typing::passes_dictionary;

impl Checker<'_> {
    pub fn record_name(&mut self, span: Span, r: Ref) {
        self.typing.names.insert(range(span), r);
    }

    /// Records the dictionaries of the constraints of a definition or
    /// argument checked at `span`.
    pub fn record_givens(&mut self, span: Span, givens: &[Type]) {
        let shown: Vec<String> = givens
            .iter()
            .filter(|g| passes_dictionary(g))
            .map(|g| self.show_exact(&self.canon(g)))
            .collect();
        if !shown.is_empty() {
            self.typing.givens.insert(range(span), shown);
        }
    }

    /// Records the type of a name or binder at `span`, settled once the
    /// module is solved.
    pub fn note_type(&mut self, span: Span, ty: &Type) {
        if !self.store.equalities.is_empty() || !self.store.row_equalities.is_empty() {
            return self.note_refined(range(span), ty, ty);
        }
        self.typing.pending_types.push((range(span), ty.clone()));
    }

    #[cfg(feature = "ide")]
    /// `record` is the record the field label at `span` belongs to.
    pub fn note_field(&mut self, span: Span, record: &Type) {
        self.typing
            .pending_fields
            .push((range(span), record.clone()));
    }

    /// The type of a value code generation makes at `at` without a node of
    /// its own, when it is a type constructor on its own.
    pub fn note_value(&mut self, at: Range, ty: &Type) {
        #[cfg(not(target_arch = "wasm32"))]
        if !self.skip_ir_types {
            self.typing.pending_ir_values.push((at, ty.clone()));
        }
        let observed = self.store.head(ty);
        if let t @ (Type::Con { .. } | Type::Meta(_)) = observed.as_ref() {
            self.typing.pending_values.push((at, t.clone()));
        }
    }

    #[cfg_attr(target_arch = "wasm32", allow(clippy::unused_self))]
    pub(in crate::types) fn note_ir_value(&mut self, at: Range, ty: &Type) {
        #[cfg(not(target_arch = "wasm32"))]
        if !self.skip_ir_types {
            self.typing.pending_ir_values.push((at, ty.clone()));
        }
        #[cfg(target_arch = "wasm32")]
        let _ = (at, ty);
    }

    #[cfg_attr(target_arch = "wasm32", allow(clippy::unused_self))]
    pub(in crate::types) fn record_type_args(&mut self, at: Range, args: Vec<(Kind, Type)>) {
        #[cfg(not(target_arch = "wasm32"))]
        if !self.skip_ir_types && !args.is_empty() {
            self.typing.pending_type_args.push((at, args));
        }
        #[cfg(target_arch = "wasm32")]
        let _ = (at, args);
    }

    #[cfg_attr(target_arch = "wasm32", allow(clippy::unused_self))]
    pub(in crate::types) fn record_con_use(
        &mut self,
        at: Range,
        entity: Entity,
        args: Vec<(Kind, Type)>,
    ) {
        #[cfg(not(target_arch = "wasm32"))]
        if !self.skip_ir_types {
            self.typing.pending_con_uses.push((at, entity, args));
        }
        #[cfg(target_arch = "wasm32")]
        let _ = (at, entity, args);
    }

    #[cfg_attr(target_arch = "wasm32", allow(clippy::unused_self))]
    pub(in crate::types) fn record_coercion_target(&mut self, at: Range, ty: &Type) {
        #[cfg(not(target_arch = "wasm32"))]
        if !self.skip_ir_types {
            self.typing.pending_coercion_targets.push((at, ty.clone()));
        }
        #[cfg(target_arch = "wasm32")]
        let _ = (at, ty);
    }

    #[cfg_attr(target_arch = "wasm32", allow(clippy::unused_self))]
    pub(in crate::types) fn record_ir_local_scheme(&mut self, at: Range, scheme: &Type) {
        #[cfg(not(target_arch = "wasm32"))]
        if !self.skip_ir_types {
            self.typing
                .pending_ir_local_schemes
                .push((at, scheme.clone()));
        }
        #[cfg(target_arch = "wasm32")]
        let _ = (at, scheme);
    }

    /// The type of an expression a keyword opens (`task`, `match`, `fun` …),
    /// recorded at the keyword so an editor can show it there.
    pub fn note_head(&mut self, node: &Node, ty: &Type) {
        self.note_value(node.range(), ty);
        // a body with a module open has no keyword of the user's: its type is
        // at the whole of it, where the head of its block finds it
        if node.kind == K::OpenExpr {
            return self.note_type(self.span(node), ty);
        }
        let head = match node.kind {
            K::TaskExpr => K::TaskKw,
            K::LazyExpr => K::LazyKw,
            K::DoExpr => K::DoKw,
            K::ForExpr => K::ForKw,
            K::ScopeExpr => K::ScopeKw,
            K::ProvideExpr => K::ProvideKw,
            K::TryExpr => K::TryKw,
            K::ResultExpr => K::ResultKw,
            K::CatchExpr => K::CatchKw,
            K::MatchExpr => K::MatchKw,
            K::IfExpr => K::IfKw,
            K::FunExpr => K::FunKw,
            _ => return,
        };
        if let Some(kw) = node.child_tokens().find(|t| t.kind == head) {
            self.note_type(self.token_span(kw), ty);
        }
    }

    pub fn record_dict(&mut self, span: Span, order: u32, dict: Dict) {
        self.typing.pending_dicts.push((range(span), order, dict));
    }

    pub fn record_label(&mut self, span: Span, ty: Type) {
        self.typing.pending_labels.push((range(span), ty));
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Typing {
    /// Drops all IR-lowering type tables once lowering has consumed them (so
    /// cached editor/analysis outputs do not retain them in memory).
    pub(crate) fn clear_ir_types(&mut self) {
        use std::mem;
        let _ = mem::take(&mut self.ir_values);
        let _ = mem::take(&mut self.ir_coercion_targets);
        let _ = mem::take(&mut self.ir_type_args);
        let _ = mem::take(&mut self.ir_con_fields);
        let _ = mem::take(&mut self.ir_type_decls);
        let _ = mem::take(&mut self.ir_top_schemes);
        let _ = mem::take(&mut self.ir_local_schemes);
        let _ = mem::take(&mut self.ir_ctor_fields);
        let _ = mem::take(&mut self.pending_ir_values);
        let _ = mem::take(&mut self.pending_coercion_targets);
        let _ = mem::take(&mut self.pending_type_args);
        let _ = mem::take(&mut self.pending_con_uses);
        let _ = mem::take(&mut self.pending_ir_local_schemes);
    }
}
