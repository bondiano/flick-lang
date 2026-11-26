//! Checking `@native "<binding>" when <cond>` overrides on top-level `let` definitions.

use std::collections::BTreeSet;

use flick_ir::ForeignMode;

use super::imply::OverrideSelection;
use super::pending::PendingForeign;
use super::spec::{ForeignBackendSpec, core_foreign_meta};
use super::{Checker, ForeignInfo};
use crate::diagnostics::{Diagnostic, codes, suggest};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::when;
use crate::types::bindings::let_parts;
use crate::types::ty::Type;
use crate::types::typing::Range;

/// Selected `@native` override metadata for a top-level `let` declaration.
#[derive(Debug, Clone)]
pub struct NativeOverrideInfo {
    /// Byte range of the `K::NativeOverride` CST node: the selected one, or,
    /// when the module is checked for every environment (the built-in ones),
    /// any one of them.
    pub override_range: Range,
    /// Calling convention and adapter metadata derived from the signature and override lines.
    pub info: ForeignInfo,
    /// Backend id whose adapter rules validated this override (e.g. `"flick/js"`, `"flick/go"`).
    pub backend_id: String,
    /// Whether lowering emits it: false when `--no-native-overrides` disabled
    /// override lowering (validation still ran), and for a clause the active
    /// environment does not select.
    pub enabled: bool,
}

impl Checker<'_> {
    /// Checks all `@native` overrides on top-level `decl`: validates `when` tag
    /// names (`FLICK-P066`), selects the unique strictly most-specific matching
    /// override (`FLICK-F012` on ambiguity), checks adapter metadata, and queues
    /// the selected override for finalization into `Typing::native_overrides`.
    pub(in crate::types) fn check_native_overrides(&mut self, decl: &Node, scheme: Option<&Type>) {
        let overrides: Vec<&Node> = decl
            .nodes()
            .filter(|n| n.kind == K::NativeOverride)
            .collect();
        if overrides.is_empty() {
            return;
        }
        let env = self.options.native_env();
        let name = let_parts(decl, self.text).name.unwrap_or_default();

        // 1. Parse every `when` condition and validate tag vocabulary (`FLICK-P066`).
        let mut nodes_and_conds: Vec<(&Node, &str)> = Vec::with_capacity(overrides.len());
        let mut exprs: Vec<when::Expr> = Vec::with_capacity(overrides.len());
        for &ov in &overrides {
            let Some(cond_node) = ov.nodes().find(|n| n.kind == K::WhenCond) else {
                continue;
            };
            let cond_span = self.span(cond_node);
            let cond_text = &self.text[cond_span.start as usize..cond_span.end as usize];
            let trimmed = cond_text.trim();
            let trim_offset = cond_text.len() - cond_text.trim_start().len();
            let Ok(expr) = when::parse(trimmed) else {
                continue;
            };
            if env.validate_tags {
                self.validate_override_tags(
                    trimmed,
                    cond_span,
                    trim_offset,
                    &expr,
                    &env.vocabulary,
                );
            }
            nodes_and_conds.push((ov, trimmed));
            exprs.push(expr);
        }

        // 2. Select among matching overrides using finite-vocabulary implication.
        let selected = match env.select(&exprs) {
            OverrideSelection::None => None,
            OverrideSelection::Selected(idx) => Some(idx),
            OverrideSelection::Ambiguous(a, b) => {
                let (node_a, cond_a) = nodes_and_conds[a];
                let (node_b, cond_b) = nodes_and_conds[b];
                let span_a = self.override_header_span(node_a);
                let span_b = self.override_header_span(node_b);
                let diag = Diagnostic::error(
                    codes::F012,
                    format!(
                        "ambiguous `@native` overrides on `{name}`: `{cond_a}` and `{cond_b}` both match the active environment and neither implies the other",
                    ),
                    span_a,
                )
                .with_related(
                    span_b,
                    format!("conflicting matching `@native` override (`when {cond_b}`)"),
                );
                self.diagnostics.push(diag);
                None
            }
        };

        // 3. Queue the selected override for full ForeignInfo finalization, and
        //    validate adapter metadata on inactive overrides whose condition uniquely
        //    identifies a known backend. Checked for every environment, each
        //    clause is finalized, by the rules of the backend it names.
        let active_spec = env.active_backend_spec(self.options.foreign_backend);
        let mut queued = false;
        for (idx, ((ov, _), expr)) in nodes_and_conds.iter().zip(&exprs).enumerate() {
            let chosen = selected == Some(idx);
            if chosen || (env.every_clause && scheme.is_some()) {
                if let Some(scheme) = scheme {
                    let spec = if chosen {
                        active_spec
                    } else {
                        env.unique_backend_for(expr).unwrap_or(active_spec)
                    };
                    let pending = self.read_native_meta(
                        decl,
                        ov,
                        &name,
                        scheme,
                        spec,
                        chosen && !env.disable_native_overrides,
                    );
                    self.typing.pending_foreigns.push(pending);
                    queued = true;
                } else {
                    self.check_meta_words_with_spec(ov, active_spec);
                }
            } else if let Some(spec) = env.unique_backend_for(expr) {
                self.check_inactive_override_meta(ov, spec);
            }
        }
        if queued {
            self.settle_operators();
        }
    }

    fn override_header_span(&self, ov: &Node) -> Span {
        let start = ov.range().0;
        let end = ov
            .nodes()
            .find(|n| n.kind == K::WhenCond)
            .map_or_else(|| ov.range().1, |w| w.range().1);
        Span::new(self.file, start, end)
    }

    fn validate_override_tags(
        &mut self,
        cond: &str,
        cond_span: Span,
        trim_offset: usize,
        expr: &when::Expr,
        vocabulary: &BTreeSet<String>,
    ) {
        let mut known_list = String::new();
        for (i, t) in vocabulary.iter().enumerate() {
            if i > 0 {
                known_list.push_str(", ");
            }
            known_list.push('`');
            known_list.push_str(t);
            known_list.push('`');
        }
        for (tag, range) in expr.tags() {
            if vocabulary.contains(tag) {
                continue;
            }
            let start = cond_span.start + u32::try_from(trim_offset + range.0).unwrap_or(0);
            let end = cond_span.start + u32::try_from(trim_offset + range.1).unwrap_or(0);
            let span = Span::new(self.file, start, end.max(start));
            let message = format!(
                "unknown environment tag `{tag}` in condition `{cond}`; known tags: {known_list}"
            );
            let mut diag = Diagnostic::error(codes::P066, message, span);
            if let Some(closest) = suggest::closest(tag, vocabulary.iter().map(String::as_str)) {
                diag = diag.with_fix(|| (format!("did you mean `{closest}`?"), Vec::new()));
            }
            self.diagnostics.push(diag);
        }
    }

    /// Reads metadata indented under a selected `@native` clause into a `PendingForeign`.
    fn read_native_meta(
        &mut self,
        decl: &Node,
        ov: &Node,
        name: &str,
        scheme: &Type,
        spec: &ForeignBackendSpec,
        enabled: bool,
    ) -> PendingForeign {
        let mut pending = PendingForeign {
            decl: decl.clone(),
            scheme: scheme.clone(),
            info: ForeignInfo::default(),
            retention: None,
            arity: Vec::new(),
            catch: Vec::new(),
            span: self.override_header_span(ov),
            name: name.to_owned(),
            unknown: None,
            native_override: Some((ov.range(), spec.backend_id.clone(), enabled)),
        };
        let pure = Self::is_pure_scheme(scheme);
        self.read_meta_items(ov, pure, true, spec, &mut pending);
        pending
    }

    /// Validates adapter word names and argument shapes on an inactive `@native`
    /// override whose `when` condition uniquely targets `spec`.
    fn check_inactive_override_meta(&mut self, ov: &Node, spec: &ForeignBackendSpec) {
        let mut seen_backend = BTreeSet::new();
        let mut dummy_words = Vec::new();
        for item in ov
            .nodes()
            .filter(|n| n.kind == K::ForeignMeta)
            .flat_map(Node::nodes)
        {
            let Some(word_tok) = item.child_tokens().find(|t| t.kind == K::Ident) else {
                continue;
            };
            let word = word_tok.text(self.text);
            if core_foreign_meta(word).is_some() {
                continue;
            }
            let args: Vec<&Node> = item.nodes().collect();
            self.read_backend_meta_item_with_spec(
                item,
                word_tok,
                word,
                &args,
                spec,
                &mut seen_backend,
                &mut dummy_words,
            );
        }
    }

    fn is_pure_scheme(scheme: &Type) -> bool {
        let body = match scheme {
            Type::Forall { body, .. } => body.as_ref(),
            other => other,
        };
        super::suspends::native_override_mode(body) == ForeignMode::Pure
    }
}
