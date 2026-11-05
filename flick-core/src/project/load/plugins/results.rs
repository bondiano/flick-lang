//! What plugins gave, kept for the calls of a run and the runs after it:
//! in memory between the runs of an editor session, and by the host.

use std::collections::HashMap;

use flick_plugin::{Arena, Block, DeriveTarget, Place, VERSION};

use crate::plugin::source::BlockSource;
use crate::plugin::{Expander, Failure};
use crate::project::Host;

/// What plugins gave for the calls of a run: those the last run kept in
/// memory first, then those the host saved, and the ones this run made.
pub(super) struct Results<'h> {
    pub(super) host: &'h dyn Host,
    pub(super) previous: HashMap<Vec<u8>, Vec<u8>>,
    /// What this run used, for the next.
    pub(super) kept: HashMap<Vec<u8>, Vec<u8>>,
}

impl Results<'_> {
    fn get(&mut self, key: &[u8]) -> Option<Vec<u8>> {
        let value = (self.previous.remove(key))
            .or_else(|| self.kept.get(key).cloned())
            .or_else(|| self.host.cached(key))?;
        self.kept.insert(key.to_vec(), value.clone());
        Some(value)
    }

    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) {
        self.host.save(&key, &value);
        self.kept.insert(key, value);
    }

    /// What `plugin` collects from `blocks`, their heads and texts in order.
    pub(super) fn collect(
        &mut self,
        plugin: &dyn Expander,
        blocks: &[Block],
    ) -> Result<Vec<u8>, String> {
        let shown = blocks
            .iter()
            .flat_map(|b| [b.head.as_bytes(), b.text.as_bytes()]);
        let id = [
            b"collect".as_slice(),
            VERSION.as_bytes(),
            plugin.id().as_bytes(),
        ];
        let key = key(id.into_iter().chain(shown));
        if let Some(state) = self.get(&key) {
            return Ok(state);
        }
        let state = plugin.collect(blocks)?;
        self.put(key, state.clone());
        Ok(state)
    }

    /// The code `plugin` gives for `block`, keyed by the block's text, where
    /// its holes stand and the state collected. An expansion whose holes did
    /// not all parse is not kept: a cached one would not report them.
    pub(super) fn expand(
        &mut self,
        plugin: &dyn Expander,
        block: &Block,
        state: &[u8],
        source: &mut BlockSource,
    ) -> Result<Arena, Failure> {
        let (column, indent) = source.layout();
        let (column, indent) = (column.to_le_bytes(), indent.to_le_bytes());
        let key = key([
            b"expand".as_slice(),
            VERSION.as_bytes(),
            plugin.id().as_bytes(),
            block.head.as_bytes(),
            block.text.as_bytes(),
            block.docs.as_bytes(),
            block.module.as_bytes(),
            (block.signature.as_ref()).map_or(&[][..], |s| s.text.as_bytes()),
            &[u8::from(block.place == Place::Declaration)],
            &column,
            &indent,
            state,
        ]);
        // a value that does not read as an arena is not one: made again
        if let Some(arena) = self.get(&key).and_then(|v| serde_json::from_slice(&v).ok()) {
            return Ok(arena);
        }
        let arena = plugin.expand(block, state, source)?;
        if source.diagnostics.is_empty()
            && let Ok(value) = serde_json::to_vec(&arena)
        {
            self.put(key, value);
        }
        Ok(arena)
    }
}

impl Results<'_> {
    /// The instance `plugin` derives of `class` for `target`, keyed as an
    /// expansion is, by everything the call reads.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn derive(
        &mut self,
        plugin: &dyn Expander,
        target: &DeriveTarget,
        class: &str,
        args: Option<&Block>,
        module: &[DeriveTarget],
        state: &[u8],
        source: &mut BlockSource,
    ) -> Result<Arena, Failure> {
        let summary = serde_json::to_vec(&(target, args, module)).unwrap_or_default();
        let key = key([
            b"derive".as_slice(),
            VERSION.as_bytes(),
            plugin.id().as_bytes(),
            class.as_bytes(),
            &summary,
            state,
        ]);
        if let Some(arena) = self.get(&key).and_then(|v| serde_json::from_slice(&v).ok()) {
            return Ok(arena);
        }
        let arena = plugin.derive(target, class, args, module, state, source)?;
        if source.diagnostics.is_empty()
            && let Ok(value) = serde_json::to_vec(&arena)
        {
            self.put(key, value);
        }
        Ok(arena)
    }
}

/// The parts, each after its length, as one key.
fn key<'a>(parts: impl IntoIterator<Item = &'a [u8]>) -> Vec<u8> {
    let mut out = Vec::new();
    for part in parts {
        let len = u64::try_from(part.len()).expect("a part fits in u64");
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(part);
    }
    out
}
