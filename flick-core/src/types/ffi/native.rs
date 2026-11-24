//! Checking `@native "<binding>" when <cond>` overrides on top-level `let` definitions.

use std::collections::BTreeSet;

use flick_ir::ForeignMode;

use super::imply::OverrideSelection;
use super::pending::PendingForeign;
use super::spec::{ForeignBackendSpec, core_foreign_meta};
use super::{Checker, ForeignInfo, Retention};
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
    /// Byte range of the selected `K::NativeOverride` CST node.
    pub override_range: Range,
    /// Calling convention and adapter metadata derived from the signature and override lines.
    pub info: ForeignInfo,
    /// Backend id whose adapter rules validated this override (e.g. `"flick/js"`, `"flick/go"`).
    pub backend_id: String,
    /// False when `--no-native-overrides` disabled override lowering (validation still ran).
    pub enabled: bool,
}

struct ParsedOverride<'a> {
    node: &'a Node,
    cond_text: String,
    expr: when::Expr,
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
        let env = self.options.native_env().clone();
        let name = let_parts(decl, self.text).name.unwrap_or_default();

        // 1. Parse every `when` condition and validate tag vocabulary (`FLICK-P066`).
        let mut parsed: Vec<ParsedOverride<'_>> = Vec::with_capacity(overrides.len());
        for &ov in &overrides {
            let Some(cond_node) = ov.nodes().find(|n| n.kind == K::WhenCond) else {
                continue;
            };
            let cond_span = self.span(cond_node);
            let cond_text = cond_node.text(self.text);
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
            parsed.push(ParsedOverride {
                node: ov,
                cond_text: trimmed.to_owned(),
                expr,
            });
        }

        // 2. Select among matching overrides using finite-vocabulary implication.
        let exprs: Vec<when::Expr> = parsed.iter().map(|p| p.expr.clone()).collect();
        let selected = match env.select(&exprs) {
            OverrideSelection::None => None,
            OverrideSelection::Selected(idx) => Some(idx),
            OverrideSelection::Ambiguous(a, b) => {
                let first = &parsed[a];
                let second = &parsed[b];
                let span_a = self.override_header_span(first.node);
                let span_b = self.override_header_span(second.node);
                let diag = Diagnostic::error(
                    codes::F012,
                    format!(
                        "ambiguous `@native` overrides on `{name}`: `{}` and `{}` both match the active environment and neither implies the other",
                        first.cond_text, second.cond_text
                    ),
                    span_a,
                )
                .with_related(
                    span_b,
                    format!("conflicting matching `@native` override (`when {}`)", second.cond_text),
                );
                self.diagnostics.push(diag);
                None
            }
        };

        // 3. Queue the selected override for full ForeignInfo finalization, and
        //    validate adapter metadata on inactive overrides whose condition uniquely
        //    identifies a known backend.
        let active_spec = env
            .active_backend_spec(self.options.foreign_backend)
            .clone();
        for (idx, item) in parsed.iter().enumerate() {
            if selected == Some(idx) {
                if let Some(scheme) = scheme {
                    let pending = self.read_native_meta(
                        decl,
                        item.node,
                        &name,
                        scheme,
                        &active_spec,
                        !env.disable_native_overrides,
                    );
                    self.typing.pending_foreigns.push(pending);
                    self.settle_operators();
                } else {
                    self.check_meta_words_with_spec(item.node, &active_spec);
                }
            } else if let Some(spec) = env.unique_backend_for(&item.expr) {
                self.check_inactive_override_meta(item.node, spec);
            }
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
        let known: Vec<&str> = vocabulary.iter().map(String::as_str).collect();
        for (tag, range) in expr.tags() {
            if vocabulary.contains(tag) {
                continue;
            }
            let start = cond_span.start + u32::try_from(trim_offset + range.0).unwrap_or(0);
            let end = cond_span.start + u32::try_from(trim_offset + range.1).unwrap_or(0);
            let span = Span::new(self.file, start, end.max(start));
            let message = format!(
                "unknown environment tag `{tag}` in condition `{cond}`; known tags: {}",
                known
                    .iter()
                    .map(|t| format!("`{t}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            let mut diag = Diagnostic::error(codes::P066, message, span);
            if let Some(closest) = suggest::closest(tag, known.iter().copied()) {
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
        let pure = Self::is_pure_scheme(scheme);
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
        let info = &mut pending.info;
        let mut retention = None;
        let mut seen_backend: BTreeSet<String> = BTreeSet::new();
        let meta: Vec<&Node> = ov
            .nodes()
            .filter(|n| n.kind == K::ForeignMeta)
            .flat_map(Node::nodes)
            .collect();
        for item in &meta {
            let Some(word_tok) = item.child_tokens().find(|t| t.kind == K::Ident) else {
                continue;
            };
            let args: Vec<&Node> = item.nodes().collect();
            let word = |i: usize| args.get(i).map(|n| n.text(self.text).trim().to_owned());
            let number = |i: usize| word(i).and_then(|w| w.parse::<usize>().ok());
            match word_tok.text(self.text) {
                "arity" => {
                    info.arity = number(0);
                    pending.arity.push(((*item).clone(), info.arity));
                }
                "reviewed" if !word(0).is_some_and(|w| w.starts_with('"')) => self.bad_meta(
                    item,
                    "`reviewed` takes the package and fingerprint the binding was checked against, as a string",
                ),
                "reviewed" => {}
                "nullable" => info.adapter.result.nullable = true,
                "optional" => match number(0) {
                    Some(i) => info.adapter.optional.push(i),
                    None => self.bad_meta(
                        item,
                        "`optional` takes the position of an `Option` argument, from 0",
                    ),
                },
                "owned" => match number(0) {
                    Some(i) => info.adapter.owned.push(i),
                    None => self.bad_meta(item, "`owned` takes the position of an argument, from 0"),
                },
                "retention" => match word(0).as_deref() {
                    Some("call") => retention = Some(Retention::Call),
                    Some("owner") => retention = Some(Retention::Owner),
                    _ => self.bad_meta(item, "write `retention call` or `retention owner`"),
                },
                "catch" => match args.first() {
                    Some(mapping) if !pure => {
                        let error = self.infer_catch_mapping(mapping);
                        pending
                            .catch
                            .push(((*item).clone(), (*mapping).clone(), error));
                        info.catch = Some(mapping.range());
                    }
                    Some(_) => self.bad_meta(
                        item,
                        "a pure function cannot throw: `catch` needs an effectful signature",
                    ),
                    None => self.bad_meta(
                        item,
                        "`catch` takes the function that maps a host exception to an error",
                    ),
                },
                other => {
                    self.read_backend_meta_item_with_spec(
                        item,
                        word_tok,
                        other,
                        &args,
                        spec,
                        &mut seen_backend,
                        &mut info.adapter.words,
                    );
                }
            }
        }
        pending.retention = retention;
        pending
    }

    /// Validates adapter word names and argument shapes on an inactive `@native`
    /// override whose `when` condition uniquely targets `spec`.
    fn check_inactive_override_meta(&mut self, ov: &Node, spec: &ForeignBackendSpec) {
        let mut seen_backend = BTreeSet::new();
        let mut dummy_words = Vec::new();
        let meta: Vec<&Node> = ov
            .nodes()
            .filter(|n| n.kind == K::ForeignMeta)
            .flat_map(Node::nodes)
            .collect();
        for item in &meta {
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
