//! The request and result of explicitly searching one proof obligation.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

use crate::source::Span;

pub(crate) const SEARCH_CANDIDATES: usize = 256;
pub(crate) const SEARCH_NORMALIZATION: u64 = 120_000;
pub(crate) const SEARCH_PROBE_STEPS: u64 = 16_000;

#[derive(Debug, Clone)]
pub struct ProofSearchRequest {
    /// The declaration in the editor's current module snapshot.
    pub declaration: (u32, u32),
    pub offset: u32,
    pub cancelled: Arc<AtomicBool>,
}

impl ProofSearchRequest {
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProofSearchStatus {
    Found,
    NoCandidate,
    BudgetExhausted,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProofSearchResult {
    pub status: ProofSearchStatus,
    pub suggestions: Vec<String>,
    /// The candidate applications probed, including rejected applications.
    pub candidates: usize,
    pub normalization_steps: u64,
    /// The selected proof term. An empty body has no term to replace.
    pub span: Option<Span>,
}

impl ProofSearchResult {
    pub fn empty(status: ProofSearchStatus) -> Self {
        Self {
            status,
            suggestions: Vec::new(),
            candidates: 0,
            normalization_steps: 0,
            span: None,
        }
    }
}
