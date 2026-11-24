//! Editor evidence retained alongside a proof check.

use std::collections::HashSet;

use super::{LoweredProof, Run, replay_lemmas, statement};
use crate::logic::{self, Lemma, ProofStep};
use crate::resolve;
use crate::types::{ProofEvidence, ProofInfo, ProofStatus};
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
