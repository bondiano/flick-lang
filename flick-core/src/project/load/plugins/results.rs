//! What plugins gave, kept for the calls of a run and the runs after it:
//! in memory between the runs of an editor session, and by the host. A
//! compiler without projects on disk, the browser's, has neither: it asks
//! its plugins again and links no code that writes an answer or reads one.

use std::collections::HashMap;

use flick_plugin::{Arena, Block, DeriveTarget, Place, Published, VERSION};

#[cfg(feature = "native-project")]
use serde::de::DeserializeOwned;
#[cfg(feature = "native-project")]
use serde::{Deserialize, Serialize};

#[cfg(feature = "native-project")]
use crate::plugin::reach::Used;
use crate::plugin::source::BlockSource;
use crate::plugin::{Expander, Failure};
use crate::project::Host;

/// What a call gave, and what its plugin reached outside to give it.
#[cfg(feature = "native-project")]
#[derive(Serialize, Deserialize)]
struct Kept<T> {
    used: Vec<Used>,
    value: T,
}

/// What plugins gave for the calls of a run: those the last run kept in
/// memory first, then those the host saved, and the ones this run made.
pub(super) struct Results<'h> {
    #[cfg_attr(not(feature = "native-project"), allow(dead_code))]
    pub(super) host: &'h dyn Host,
    #[cfg_attr(not(feature = "native-project"), allow(dead_code))]
    pub(super) previous: HashMap<Vec<u8>, Vec<u8>>,
    /// What this run used, for the next.
    pub(super) kept: HashMap<Vec<u8>, Vec<u8>>,
}

#[cfg(feature = "native-project")]
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
}

impl Results<'_> {
    /// What `make` gives, or what the call `key` gave earlier while each
    /// answer its plugin reached is still the same. A call that reported
    /// anything is not kept: a kept one would not report it again.
    #[cfg(feature = "native-project")]
    fn reached<T: Serialize + DeserializeOwned, E>(
        &mut self,
        key: Vec<u8>,
        source: &mut BlockSource,
        make: impl FnOnce(&mut BlockSource) -> Result<T, E>,
    ) -> Result<T, E> {
        // a value that does not read as one is not one
        let kept = self
            .get(&key)
            .and_then(|v| serde_json::from_slice::<Kept<T>>(&v).ok());
        if let Some(kept) = kept.filter(|k| source.still(&k.used)) {
            source.reach.used = kept.used;
            return Ok(kept.value);
        }
        let value = make(source)?;
        let kept = Kept {
            used: source.reach.used.clone(),
            value,
        };
        if source.diagnostics.is_empty()
            && source.reach.problems.is_empty()
            && let Ok(bytes) = serde_json::to_vec(&kept)
        {
            self.put(key, bytes);
        }
        Ok(kept.value)
    }

    /// What `make` gives: nothing is kept.
    #[cfg(not(feature = "native-project"))]
    #[allow(clippy::unused_self)]
    fn reached<T, E>(
        &mut self,
        _key: Vec<u8>,
        source: &mut BlockSource,
        make: impl FnOnce(&mut BlockSource) -> Result<T, E>,
    ) -> Result<T, E> {
        make(source)
    }

    /// What `plugin` collects from `blocks`, their heads and texts in order.
    pub(super) fn collect(
        &mut self,
        plugin: &dyn Expander,
        blocks: &[Block],
        source: &mut BlockSource,
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
        self.reached(key, source, |source| plugin.collect(blocks, source))
    }

    /// What `plugin` publishes of `state`, the state of a group.
    pub(super) fn publish(
        &mut self,
        plugin: &dyn Expander,
        state: &[u8],
    ) -> Result<Vec<Published>, String> {
        let id = [
            b"publish".as_slice(),
            VERSION.as_bytes(),
            plugin.id().as_bytes(),
            state,
        ];
        self.reached(key(id), &mut BlockSource::default(), |_| {
            plugin.publish(state)
        })
    }

    /// The code `plugin` gives for `block`, keyed by the block's text, where
    /// its holes stand and the state collected.
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
        self.reached(key, source, |source| plugin.expand(block, state, source))
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
        // the key of a kept answer: nothing is kept without projects on disk
        #[cfg(feature = "native-project")]
        let summary = serde_json::to_vec(&(target, args, module)).unwrap_or_default();
        #[cfg(not(feature = "native-project"))]
        let summary = Vec::new();
        let key = key([
            b"derive".as_slice(),
            VERSION.as_bytes(),
            plugin.id().as_bytes(),
            class.as_bytes(),
            &summary,
            state,
        ]);
        self.reached(key, source, |source| {
            plugin.derive(target, class, args, module, state, source)
        })
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
