//! Hover on expression keywords: the type of the whole expression and what the
//! keyword does; at `ignore` and `await`, the type of the operand.

use std::fmt::Write;

use super::super::{Ide, tree};
use crate::source::FileId;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::Lang;
use crate::types::{show_signature, show_type};

impl Ide<'_> {
    /// A keyword at `offset` that opens an expression: the type of the whole
    /// expression and what the keyword does; at `ignore` and `await` the
    /// type of the operand.
    pub(super) fn keyword(
        &self,
        file: FileId,
        offset: u32,
    ) -> Option<(String, String, (u32, u32))> {
        let m = self.module(file)?;
        let parent = *tree::path_to(&m.tree, offset).last()?;
        let kw = parent
            .child_tokens()
            .find(|t| t.start <= offset && offset <= t.end && t.kind != K::Ident)?;
        let at = (kw.start, kw.end);
        if matches!(kw.kind, K::DeferKw | K::ErrdeferKw) {
            return self.defer_hover(file, kw.kind == K::ErrdeferKw, at);
        }
        let ty = m.typing.types.get(&at)?;
        let shown = show_signature(ty);
        let (head, note) = match kw.kind {
            K::TaskKw => (
                "task { … }",
                "Builds this computation without running it; `await` runs it.",
            ),
            K::ScopeKw => (
                "scope { … }",
                "Runs its body in a fresh region and releases what the body \
                 acquired when it ends; the value may not mention the region.",
            ),
            K::ProvideKw => (
                "provide … { … }",
                "Runs its body with the services the layer builds; they leave \
                 the row, and what building the layer needs is added.",
            ),
            K::TryKw => (
                "try … with",
                "Handles the errors of its body: an error leaves the row when an \
                 arm without a guard matches every value of it.",
            ),
            K::ResultKw => {
                let mut note = String::from(
                    "Runs its body eagerly in the current fiber at runtime, wrapping its value \
                     in `Ok` and catching its typed failures as `Err`.",
                );
                if let Some(services) = m.typing.result_services.get(&at) {
                    let services = show_type(services);
                    let _ = write!(note, "\n\nRemaining services: `{services}`.");
                } else {
                    note.push_str("\n\nRemaining services: `none`.");
                }
                return Some((format!("result {{ … }} : {shown}"), note, at));
            }
            K::CatchKw => (
                "… catch | …",
                "Handles typed errors of the expression on its left: an error leaves \
                 the row when an arm without a guard matches every value of it.",
            ),
            K::MatchKw => ("match … with", "The value of the arm that matches."),
            K::IfKw => ("if … then … else …", "The value of the branch taken."),
            K::FunKw => ("fun … -> …", "An anonymous function."),
            K::LetKw if parent.kind == K::LetDecl && parent.has_token(K::ElseKw) => (
                "let … = … else …",
                "Binds the pattern for the remainder of the current block; if the pattern \
                 does not match, the `else` fallback becomes the result of the current \
                 block and skips the remaining statements. Pattern bindings are monomorphic \
                 (as in `match`) and only in scope after `else`.",
            ),
            K::ElseKw if parent.kind == K::LetDecl => (
                "let … = … else …",
                "If the pattern does not match, this fallback value becomes the result of \
                 the current block, skipping the remaining statements. Pattern bindings are \
                 monomorphic (as in `match`) and only in scope after `else`.",
            ),
            K::AwaitKw => {
                let ran = match ty.spine() {
                    (h, args) if h.is_lang(Lang::Eff) && args.len() == 3 => show_type(args[2]),
                    _ => shown.clone(),
                };
                return Some((
                    format!("await … : {ran}"),
                    format!("Runs `{shown}` once in the current fiber."),
                    (kw.start, kw.end),
                ));
            }
            K::IgnoreKw => {
                let mut note = format!(
                    "Evaluates its operand and discards its value of type `{shown}`; \
                     its effects and errors are kept."
                );
                if ty.spine().0.is_lang(Lang::Eff) {
                    note.push_str(
                        " The computation is not run: `ignore (await …)` runs it \
                         and discards its result.",
                    );
                }
                return Some((
                    format!("ignore : {shown} -> Unit"),
                    note,
                    (kw.start, kw.end),
                ));
            }
            _ => return None,
        };
        Some((
            format!("{head} : {shown}"),
            note.to_owned(),
            (kw.start, kw.end),
        ))
    }
}
