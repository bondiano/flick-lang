//! The trait a plugin implements: the exports of its component.

use crate::{
    Arena, Artifact, Block, CallSite, Compiler, Completion, DeriveTarget, Diagnostic, Fact,
    Highlight, Hover, Piece, Published, Span,
};

/// A plugin: what it learns from all its blocks of a group, and the code
/// each block stands for. Both depend only on the blocks and on what the
/// project grants them to read, so the compiler caches each result while
/// what its call read stays the same.
pub trait Plugin {
    /// What the plugin learns from `blocks`, the state each expansion
    /// gets; `compiler` reaches what the project grants, but parses no
    /// hole.
    fn collect(&self, blocks: &[Block], compiler: &mut dyn Compiler) -> Vec<u8>;
    /// What the blocks of other groups and modules may ask of `state`, the
    /// state of a group: each before any block expands.
    fn publish(&self, state: &[u8]) -> Vec<Published> {
        let _ = state;
        Vec::new()
    }
    /// The module the file at `path` stands for, its contents `bytes`, as
    /// declarations at spans of the file; only a plugin whose package says
    /// `load = ["ext"]` gets the files of that extension.
    fn load(&self, path: &str, bytes: &[u8]) -> Result<Arena, Vec<Diagnostic>> {
        let _ = (path, bytes);
        let whole = Span {
            start: 0,
            end: u32::try_from(bytes.len()).unwrap_or(u32::MAX),
        };
        Err(vec![Diagnostic::error("this plugin loads no files", whole)])
    }
    /// The IR of the project's modules as a `Program` in CBOR, after the
    /// checks of effects and before the build optimizes it; the program the
    /// build compiles instead. Only a plugin whose package says `hooks =
    /// ["ir"]` gets it.
    fn transform(&self, program: Vec<u8>) -> Result<Vec<u8>, Vec<Diagnostic>> {
        Ok(program)
    }
    /// The files of `flick build`, each `.js` with its `.map` when the build
    /// writes maps; what it writes instead. Only a plugin whose package says
    /// `hooks = ["emit"]` gets them.
    fn emit(&self, artifacts: Vec<Artifact>) -> Result<Vec<Artifact>, Vec<Diagnostic>> {
        Ok(artifacts)
    }
    /// The code `block` stands for given what `collect` learned, or why
    /// there is none; no diagnostic means the compiler already reported why.
    fn expand(
        &self,
        block: &Block,
        state: &[u8],
        compiler: &mut dyn Compiler,
    ) -> Result<Arena, Vec<Diagnostic>>;
    /// What an editor may insert at `offset` of `block`; `member` says the
    /// user typed `name.` right before `offset`, the `.` not in the block.
    /// `compiler` answers `published` and `literal_of` as while it expands,
    /// and parses no hole.
    fn complete(
        &self,
        block: &Block,
        state: &[u8],
        offset: u32,
        member: bool,
        compiler: &mut dyn Compiler,
    ) -> Vec<Completion> {
        let _ = (block, state, offset, member, compiler);
        Vec::new()
    }
    /// What an editor shows at `offset` of `block`; `facts` are the types
    /// of the `_` of its code, as `check` gets them, once it checked. An
    /// offset past the end of the block is its head, before the block.
    fn hover(&self, block: &Block, state: &[u8], offset: u32, facts: &[Fact]) -> Option<Hover> {
        let _ = (block, state, offset, facts);
        None
    }
    /// How an editor colors the words of `block` that are the plugin's.
    fn highlight(&self, block: &Block, state: &[u8]) -> Vec<Highlight> {
        let _ = (block, state);
        Vec::new()
    }
    /// The instance of `class` for `target`, and what goes with it, as
    /// declarations; `args` is the `{ … }` after the class in `deriving`,
    /// and `module` every type the module of `target` declares.
    fn derive(
        &self,
        target: &DeriveTarget,
        class: &str,
        args: Option<&Block>,
        module: &[DeriveTarget],
        state: &[u8],
        compiler: &mut dyn Compiler,
    ) -> Result<Arena, Vec<Diagnostic>> {
        let _ = (args, module, state, compiler);
        let message = format!("`{class}` has no derive in this plugin");
        Err(vec![Diagnostic::error(message, target.decl.whole())])
    }
    /// The layout of `block`, the user's code in it found through
    /// `compiler`; none keeps the block as written.
    fn format(
        &self,
        block: &Block,
        state: &[u8],
        compiler: &mut dyn Compiler,
    ) -> Option<Vec<Piece>> {
        let _ = (block, state, compiler);
        None
    }
    /// What is wrong with `block` that only the types of its code show:
    /// `facts` are the types of the `_` of its code, at the spans it gave
    /// them. It only adds errors.
    fn check(&self, block: &Block, state: &[u8], facts: &[Fact]) -> Vec<Diagnostic> {
        let _ = (block, state, facts);
        Vec::new()
    }
    /// What is wrong with the places of one module that call the names the
    /// plugin's package declares in `watch`: `calls` are those places with
    /// the types the checker found there, at spans of the module's own
    /// file, and `state` is what `collect` learned from that module's
    /// blocks, empty where it has none. It only adds errors.
    fn check_calls(&self, calls: &[CallSite], state: &[u8]) -> Vec<Diagnostic> {
        let _ = (calls, state);
        Vec::new()
    }
}
