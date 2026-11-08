//! Plugins: blocks `head { … }` whose code a plugin gives back. The code
//! comes as an arena of core syntax ([`flick_plugin::Arena`]); it is
//! rendered as text after the module's own, parsed and spliced in place of
//! the block, the user's code inside keeping its own place in the file.

pub mod derive;
pub mod expansion;
pub mod first_party;
pub mod reach;
pub mod render;
pub mod source;
pub mod splice;
pub mod testing;

use std::panic::{self, AssertUnwindSafe};
use std::sync::Arc;

use flick_plugin::{
    Arena, Block, Completion, DeriveTarget, Diagnostic, Fact, Highlight, Hover, Piece, Plugin,
    Published,
};

use source::{BlockSource, Seen};

use crate::diagnostics::{self, codes};
use crate::source::Span;
use crate::syntax::cst::{Node, Token};
use crate::syntax::kind::SyntaxKind as K;

/// A plugin as the compiler runs it: linked into the compiler, or loaded.
pub trait Expander: Send + Sync {
    /// What tells this plugin and its version apart, for caches.
    fn id(&self) -> &str;
    /// What the plugin learns from all its blocks of a group; `source`
    /// reaches what the project grants.
    fn collect(&self, blocks: &[Block], source: &mut BlockSource) -> Result<Vec<u8>, String>;
    /// What the blocks of other groups and modules may ask of `state`.
    fn publish(&self, state: &[u8]) -> Result<Vec<Published>, String>;
    /// The code `block` stands for; `source` parses the holes it asks for.
    fn expand(
        &self,
        block: &Block,
        state: &[u8],
        source: &mut BlockSource,
    ) -> Result<Arena, Failure>;
    /// What an editor may insert at `offset` of `block`, whether the user
    /// typed `name.` before `offset`; `source` answers what the block's
    /// module sees. An error when the plugin fails.
    fn complete(
        &self,
        block: &Block,
        state: &[u8],
        offset: u32,
        member: bool,
        source: &mut BlockSource,
    ) -> Result<Vec<Completion>, String> {
        let _ = (block, state, offset, member, source);
        Ok(Vec::new())
    }
    /// What an editor shows at `offset` of `block`, given `facts`, the
    /// types of the `_` of its code.
    fn hover(
        &self,
        block: &Block,
        state: &[u8],
        offset: u32,
        facts: &[Fact],
    ) -> Result<Option<Hover>, String> {
        let _ = (block, state, offset, facts);
        Ok(None)
    }
    /// How an editor colors the words of `block` that are the plugin's.
    fn highlight(&self, block: &Block, state: &[u8]) -> Result<Vec<Highlight>, String> {
        let _ = (block, state);
        Ok(Vec::new())
    }
    /// The layout of `block`, whose holes `source` parses; none keeps it as
    /// written, and an error says the plugin failed.
    fn format(
        &self,
        block: &Block,
        state: &[u8],
        source: &mut BlockSource,
    ) -> Result<Option<Vec<Piece>>, String> {
        let _ = (block, state, source);
        Ok(None)
    }
    /// The instance of `class` for `target` as declarations; `source`, the
    /// target's declaration, parses the holes the plugin asks for.
    fn derive(
        &self,
        target: &DeriveTarget,
        class: &str,
        args: Option<&Block>,
        module: &[DeriveTarget],
        state: &[u8],
        source: &mut BlockSource,
    ) -> Result<Arena, Failure> {
        let _ = (args, module, state, source);
        let message = format!("`{class}` has no derive in this plugin");
        Err(Failure::Refused(vec![Diagnostic::error(
            message,
            target.decl.whole(),
        )]))
    }
    /// What only the types of `block`'s code show is wrong with it, from
    /// `facts`, the types of the `_` of its code; why the plugin failed.
    fn check(
        &self,
        block: &Block,
        state: &[u8],
        facts: &[Fact],
    ) -> Result<Vec<Diagnostic>, String> {
        let _ = (block, state, facts);
        Ok(Vec::new())
    }
}

/// A plugin block as editors ask its plugin about it: completions and
/// hovers, even while the block does not expand.
#[derive(Clone)]
pub struct EditorBlock {
    /// Where its frame starts in the module text; offsets of the block
    /// count from here.
    pub start: u32,
    pub block: Block,
    /// What its plugin collected from its group.
    pub state: Arc<[u8]>,
    pub plugin: Arc<dyn Expander>,
    /// What the block's module sees, which answers its plugin's questions.
    seen: Seen,
}

impl EditorBlock {
    /// The block of `source`, for editors.
    pub fn new(source: &BlockSource, state: &[u8], plugin: &Arc<dyn Expander>) -> Self {
        EditorBlock {
            start: source.start(),
            block: source.block(),
            state: Arc::from(state),
            plugin: Arc::clone(plugin),
            seen: source.seen(),
        }
    }

    /// What answers the plugin's questions in an editor: what the block's
    /// module sees, no hole parsed.
    pub fn source(&self) -> BlockSource {
        self.seen.source()
    }

    /// Whether `offset` of the module text is in the block, its end included.
    pub fn holds(&self, offset: u32) -> bool {
        let len = u32::try_from(self.block.text.len()).unwrap_or(u32::MAX);
        self.start <= offset && offset - self.start <= len
    }
}

/// Why a plugin gave no code for a block.
#[derive(Debug)]
pub enum Failure {
    /// What the plugin found wrong, at spans of the body; none when the
    /// compiler already reported why.
    Refused(Vec<Diagnostic>),
    /// The plugin itself failed: it trapped or ran out of fuel or memory.
    Broken(String),
}

/// A plugin linked into the compiler.
pub struct Native<P> {
    id: String,
    plugin: P,
}

impl<P: Plugin> Native<P> {
    /// `plugin`, told apart by `id`: its package, since the compiler's own
    /// version covers its code.
    pub fn new(id: impl Into<String>, plugin: P) -> Self {
        Native {
            id: id.into(),
            plugin,
        }
    }
}

impl<P: Plugin + Send + Sync> Expander for Native<P> {
    fn id(&self) -> &str {
        &self.id
    }

    fn collect(&self, blocks: &[Block], source: &mut BlockSource) -> Result<Vec<u8>, String> {
        guarded(|| self.plugin.collect(blocks, source))
    }

    fn publish(&self, state: &[u8]) -> Result<Vec<Published>, String> {
        guarded(|| self.plugin.publish(state))
    }

    fn expand(
        &self,
        block: &Block,
        state: &[u8],
        source: &mut BlockSource,
    ) -> Result<Arena, Failure> {
        guarded(|| self.plugin.expand(block, state, source))
            .map_err(Failure::Broken)?
            .map_err(Failure::Refused)
    }

    fn complete(
        &self,
        block: &Block,
        state: &[u8],
        offset: u32,
        member: bool,
        source: &mut BlockSource,
    ) -> Result<Vec<Completion>, String> {
        guarded(|| self.plugin.complete(block, state, offset, member, source))
    }

    fn hover(
        &self,
        block: &Block,
        state: &[u8],
        offset: u32,
        facts: &[Fact],
    ) -> Result<Option<Hover>, String> {
        guarded(|| self.plugin.hover(block, state, offset, facts))
    }

    fn highlight(&self, block: &Block, state: &[u8]) -> Result<Vec<Highlight>, String> {
        guarded(|| self.plugin.highlight(block, state))
    }

    fn format(
        &self,
        block: &Block,
        state: &[u8],
        source: &mut BlockSource,
    ) -> Result<Option<Vec<Piece>>, String> {
        guarded(|| self.plugin.format(block, state, source))
    }

    fn derive(
        &self,
        target: &DeriveTarget,
        class: &str,
        args: Option<&Block>,
        module: &[DeriveTarget],
        state: &[u8],
        source: &mut BlockSource,
    ) -> Result<Arena, Failure> {
        guarded(|| (self.plugin).derive(target, class, args, module, state, source))
            .map_err(Failure::Broken)?
            .map_err(Failure::Refused)
    }

    fn check(
        &self,
        block: &Block,
        state: &[u8],
        facts: &[Fact],
    ) -> Result<Vec<Diagnostic>, String> {
        guarded(|| self.plugin.check(block, state, facts))
    }
}

/// What `call` gives, or why it panicked: a native plugin runs in the
/// compiler's process, which its panic must not take down.
fn guarded<T>(call: impl FnOnce() -> T) -> Result<T, String> {
    panic::catch_unwind(AssertUnwindSafe(call)).map_err(|payload| {
        let why = (payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "no message".to_owned());
        format!("panicked: {why}")
    })
}

/// What the plugin `package` found wrong at a span of a block whose frame
/// is `frame` in its module, under the plugin's own code or `P036` without
/// one: the whole frame, or any span of a declaration of another module
/// (`foreign`), is `whole`, what the block stands for.
pub(crate) fn placed(
    d: &Diagnostic,
    package: &str,
    frame: Span,
    whole: Span,
    foreign: bool,
) -> diagnostics::Diagnostic {
    let all = (d.span.start, d.span.end) == (0, frame.end - frame.start);
    let span = if all || foreign {
        whole
    } else {
        Span::new(
            frame.file,
            frame.start + d.span.start,
            frame.start + d.span.end,
        )
    };
    let Some(code) = (d.code.as_deref()).and_then(|code| codes::plugin(package, code)) else {
        return diagnostics::Diagnostic::error(
            codes::P036,
            format!("plugin `{package}`: {}", d.message),
            span,
        );
    };
    let severity = match d.severity {
        flick_plugin::Severity::Error => diagnostics::Severity::Error,
        flick_plugin::Severity::Warning => diagnostics::Severity::Warning,
        flick_plugin::Severity::Note => diagnostics::Severity::Note,
    };
    let mut placed =
        diagnostics::Diagnostic::new(code, severity, d.message.clone()).with_span(span);
    // a fix edits the block, so one of a declaration elsewhere has nowhere to go
    for fix in d.fixes.iter().filter(|_| !foreign) {
        let edits = || {
            let edits = (fix.edits.iter())
                .map(|e| diagnostics::Edit {
                    span: Span::new(
                        frame.file,
                        frame.start + e.span.start,
                        frame.start + e.span.end,
                    ),
                    replacement: e.replacement.clone(),
                })
                .collect();
            (fix.message.clone(), edits)
        };
        placed = if fix.safe {
            placed.with_safe_fix(edits)
        } else {
            placed.with_fix(edits)
        };
    }
    placed
}

/// The head of the block `node`, its first token: a name.
pub fn head_token(node: &Node) -> Option<Token> {
    let head = node.child_tokens().find(|t| !t.kind.is_trivia())?;
    (head.kind == K::Ident).then_some(head)
}

/// How many blocks a block may be nested in, in the user's code of others.
pub const ROUNDS: usize = 8;

/// The plugin blocks under `node`, expressions and declarations, not those
/// inside them.
pub fn blocks_under<'n>(node: &'n Node, out: &mut Vec<&'n Node>) {
    for child in node.nodes() {
        if matches!(child.kind, K::PluginExpr | K::PluginDecl) {
            out.push(child);
        } else {
            blocks_under(child, out);
        }
    }
}

#[cfg(test)]
mod tests;
