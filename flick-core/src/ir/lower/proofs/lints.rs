//! Lints for proofs that passed certificate checking in this module.

use std::collections::{HashMap, HashSet};

use crate::diagnostics::{Diagnostic, Edit, Severity, codes};
use crate::logic::{Arg, Cert, Rule};
use crate::resolve;
use crate::source::{FileId, Span};
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{Range, Typing};

pub(super) struct Lints<'s> {
    source: &'s str,
    file: FileId,
    generated: u32,
    unused: HashMap<Range, Vec<Diagnostic>>,
}

impl<'s> Lints<'s> {
    pub(super) fn new(
        module: &resolve::Module<'s>,
        typing: &Typing,
        unused_premises: bool,
    ) -> Self {
        let mut unused = HashMap::new();
        if unused_premises {
            let used: HashSet<_> = typing.binders.values().copied().collect();
            for decl in module.tree.nodes().filter(|n| n.kind == K::ProofDecl) {
                let found = resolve::unused_proof_premises(module, decl, &used);
                if !found.is_empty() {
                    unused.insert(decl.range(), found);
                }
            }
        }
        Self {
            source: module.text,
            file: module.file,
            generated: module.generated,
            unused,
        }
    }

    pub(super) fn premises(&mut self, at: Range, out: &mut Vec<Diagnostic>) {
        if let Some(unused) = self.unused.remove(&at) {
            out.extend(unused);
        }
    }

    pub(super) fn steps(&self, cert: &Cert, out: &mut Vec<Diagnostic>) {
        match &cert.kind {
            Rule::Symm(child) => {
                if let Rule::Symm(original) = &child.kind {
                    if let Some(diagnostic) = self.double_symmetry(cert.span, original) {
                        out.push(diagnostic);
                    }
                    // Advance past both swaps so emitted edits cannot overlap.
                    self.steps(original, out);
                } else {
                    self.steps(child, out);
                }
            }
            Rule::Let(_, p, q) | Rule::Trans(_, p, q) => {
                self.steps(p, out);
                self.steps(q, out);
            }
            Rule::Cong(_, _, p) | Rule::Funext(_, p) => self.steps(p, out),
            Rule::Case(_, arms) => arms.iter().for_each(|(_, p)| self.steps(p, out)),
            Rule::Lemma(_, args) => args.iter().for_each(|arg| {
                if let Arg::Proof(p) = arg {
                    self.steps(p, out);
                }
            }),
            Rule::Refl | Rule::Fact(_) | Rule::Hole(_) => {}
        }
    }

    fn double_symmetry(&self, span: Span, original: &Cert) -> Option<Diagnostic> {
        if span.file != self.file || span.end > self.generated {
            return None;
        }
        let warning = Diagnostic::new(
            codes::T091,
            Severity::Warning,
            "applying `symm` twice leaves the same equation",
        )
        .with_span(span);
        Some(match self.atomic_replacement(span, original) {
            Some(text) => warning.with_safe_fix(|| {
                let edit = Edit {
                    span,
                    replacement: text.to_owned(),
                };
                ("remove both applications of `symm`", vec![edit])
            }),
            None => warning,
        })
    }

    /// Swapping twice preserves the goal and its exact subject. An atomic
    /// child can replace the full application without changing precedence.
    /// Require the source to have just the two applications and their child
    /// so the edit also preserves comments, annotations and layout.
    fn atomic_replacement(&self, outer: Span, original: &Cert) -> Option<&str> {
        let atomic = matches!(original.kind, Rule::Refl | Rule::Fact(_))
            || matches!(&original.kind, Rule::Lemma(_, args) if args.is_empty());
        let inner = original.span;
        if !atomic || inner.file != outer.file || inner.start < outer.start || inner.end > outer.end
        {
            return None;
        }
        let whole = self.source.get(outer.start as usize..outer.end as usize)?;
        if whole.contains(['\n', '\r']) {
            return None;
        }
        let left = (inner.start - outer.start) as usize;
        let right = (inner.end - outer.start) as usize;
        let before = whole.get(..left)?.strip_prefix("symm")?.trim_start();
        let before = before
            .strip_prefix('(')?
            .trim_start()
            .strip_prefix("symm")?;
        let after = whole.get(right..)?.trim();
        (before.trim().is_empty() && after == ")")
            .then(|| whole.get(left..right))
            .flatten()
    }
}
