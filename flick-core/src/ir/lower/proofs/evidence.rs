//! Editor evidence retained alongside a proof check.

#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashSet;

#[cfg(not(target_arch = "wasm32"))]
use super::{Lower, statement};
use super::{LoweredProof, Run, replay_lemmas};
#[cfg(not(target_arch = "wasm32"))]
use crate::diagnostics::{Diagnostic, Severity};
use crate::logic::Lemma;
#[cfg(not(target_arch = "wasm32"))]
use crate::logic::{self, ProofStep};
use crate::resolve;
#[cfg(not(target_arch = "wasm32"))]
use crate::source::Span;
#[cfg(not(target_arch = "wasm32"))]
use crate::syntax::cst::{Element, Node, Token};
#[cfg(not(target_arch = "wasm32"))]
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{ProofEvidence, ProofStatus};
#[cfg(not(target_arch = "wasm32"))]
use crate::types::{ProofInfo, Typing};
use flick_ir::Dict;

pub(super) struct Recording<'a> {
    imports: Vec<String>,
    pub out: &'a mut Vec<ProofEvidence>,
}

impl<'a> Recording<'a> {
    pub fn new(module: &resolve::Module, run: &Run, out: &'a mut Vec<ProofEvidence>) -> Self {
        let mut modules: Vec<_> = module
            .syntax
            .imports
            .iter()
            .filter(|import| (run.proven)(&import.module).is_some())
            .map(|import| import.module.clone())
            .collect();
        modules.sort_unstable();
        modules.dedup();
        let imports = replay_lemmas(&modules, run, 0);
        let mut qualified = Vec::new();
        for lemma in &imports {
            if let Some((module_name, name)) = lemma.statement.name.rsplit_once('.') {
                qualified.extend(
                    module
                        .syntax
                        .imports
                        .iter()
                        .filter(|import| import.module == module_name)
                        .map(|import| format!("{}.{}", import.qualifier(), name)),
                );
            }
        }
        qualified.sort_unstable();
        qualified.dedup();
        Self {
            imports: qualified,
            out,
        }
    }

    pub fn begin(&self, lowered: &LoweredProof, lemmas: &[Lemma]) -> ProofEvidence {
        let mut visible = self.imports.clone();
        visible.extend(
            lemmas
                .iter()
                .map(|l| &l.statement.name)
                .filter(|name| !name.contains('.'))
                .cloned(),
        );
        ProofEvidence {
            at: lowered.at,
            status: ProofStatus::Open,
            failure: None,
            steps: Vec::new(),
            lemmas: visible,
            instance: lowered.subject.as_ref().and_then(|dict| match dict {
                Dict::Instance { module, name, .. } => Some(format!("{module}.{name}")),
                _ => None,
            }),
            obligations: Vec::new(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn observe_body(
    lowered: &LoweredProof,
    info: &ProofInfo,
    lemmas: &[Lemma],
    steps: &mut Vec<ProofStep>,
) {
    if let (Ok(statement), Some(body)) = (statement(lowered, info), &lowered.body) {
        let mut recovered = Vec::new();
        logic::observe(&statement, body, &info.data, lemmas, &mut recovered);
        let checked: HashSet<_> = steps.iter().map(|step| step.span).collect();
        steps.extend(
            recovered
                .into_iter()
                .filter(|step| !checked.contains(&step.span)),
        );
    }
}

/// Preserve certificate structure around broken source. Ordinary lowering
/// assumes successful elaboration, so keep broken leaves out of its input.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn lower_drafts(
    module: &resolve::Module,
    typing: &Typing,
    diagnostics: &[Diagnostic],
) -> Vec<LoweredProof> {
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .filter_map(|d| d.span.filter(|span| span.file == module.file))
        .collect();
    let mut lower = Lower::new(module, typing);
    module
        .tree
        .nodes()
        .filter(|node| node.kind == K::ProofDecl)
        .filter_map(|decl| {
            let body = decl.nodes().find(|node| node.kind == K::Block);
            let header_end = decl
                .child_tokens()
                .find(|t| t.kind == K::Eq)
                .map_or(decl.range().1, |t| t.start);
            if errors
                .iter()
                .any(|e| decl.range().0 <= e.start && e.start < header_end)
            {
                return None;
            }
            if let Some(body) = body
                && has_error(body, &errors)
            {
                let mut draft = decl.clone();
                for child in &mut draft.children {
                    if let Element::Node(node) = child
                        && node.kind == K::Block
                    {
                        *node = repair(node, &errors);
                    }
                }
                let mut lowered = lower.proof(&draft);
                lowered.at = decl.range();
                lowered.span = lower.span(decl);
                Some(lowered)
            } else {
                Some(lower.proof(decl))
            }
        })
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
fn has_error(node: &Node, errors: &[Span]) -> bool {
    let (start, end) = node.range();
    errors
        .iter()
        .any(|error| start <= error.start && error.start <= end)
}

#[cfg(not(target_arch = "wasm32"))]
fn repair(node: &Node, errors: &[Span]) -> Node {
    if !has_error(node, errors) {
        return node.clone();
    }
    let keep = match node.kind {
        K::Block | K::RecordExpr | K::RecordField => true,
        K::AppExpr | K::ParenExpr => node.nodes().next().is_some(),
        K::MatchExpr => node
            .nodes()
            .next()
            .is_some_and(|first| first.kind != K::MatchArm),
        K::FunExpr | K::MatchArm | K::LetDecl => {
            node.nodes().any(|part| part.kind == K::Block)
                && !node
                    .nodes()
                    .any(|part| part.kind != K::Block && has_error(part, errors))
        }
        _ => false,
    };
    if keep {
        let mut recovered = node.clone();
        recovered.children.retain_mut(|child| {
            if let Element::Node(part) = child {
                // A branch with a broken pattern contributes no refinement.
                if part.kind == K::MatchArm
                    && part
                        .nodes()
                        .any(|n| n.kind != K::Block && has_error(n, errors))
                {
                    return false;
                }
                if has_error(part, errors) {
                    *part = repair(part, errors);
                }
            }
            true
        });
        recovered
    } else {
        let (start, end) = node.range();
        Node {
            kind: K::UnitExpr,
            children: vec![Element::Token(Token {
                kind: K::LParen,
                start,
                end,
            })],
        }
    }
}
