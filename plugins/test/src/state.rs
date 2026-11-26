//! What the plugin collects from a group: for each top-level block of each
//! module in the order written, the hashes of its module, of its text and
//! of its name. A block learns from it where it stands among its module's
//! blocks and whether another of them has its name.

use flick_plugin::{Block, Place};

use crate::read::{name_of, structure};

/// The bytes of a block in the state: three hashes.
const ENTRY: usize = 24;

/// Each block the state names. A block whose tokens show it wrong is left
/// out: it declares nothing, so the module's `tests` does not name it.
pub fn collect(blocks: &[Block]) -> Vec<u8> {
    let mut out = Vec::new();
    for block in blocks.iter().filter(|b| top(b) && structure(b).is_ok()) {
        let name = name_of(block).unwrap_or_default();
        for word in [
            hash(block.module.as_bytes()),
            key(block),
            hash(name.as_bytes()),
        ] {
            out.extend(word.to_le_bytes());
        }
    }
    out
}

/// Whether `block` is a suite or a test of the module, not of an expression.
fn top(block: &Block) -> bool {
    crate::owns(&block.head) && block.place == Place::Declaration
}

/// Where `block` stands among the `count` blocks of its module, by `state`,
/// and whether another of them has its name; blocks of one name stand
/// nowhere, as each is wrong. A block the state lacks, as the formatter
/// collects one block alone, is the only one.
pub fn place(state: &[u8], block: &Block) -> (usize, usize, bool) {
    let module = hash(block.module.as_bytes());
    let word = |entry: &[u8], i: usize| {
        let bytes: [u8; 8] = entry[i * 8..i * 8 + 8].try_into().unwrap_or_default();
        u64::from_le_bytes(bytes)
    };
    let own: Vec<(u64, u64)> = (state.chunks_exact(ENTRY))
        .filter(|entry| word(entry, 0) == module)
        .map(|entry| (word(entry, 1), word(entry, 2)))
        .collect();
    let named = |name: u64| own.iter().filter(|(_, n)| *n == name).count() > 1;
    let mine = key(block);
    let mut index = None;
    let mut count = 0;
    for (k, n) in &own {
        if *k == mine && named(*n) {
            return (0, 1, true);
        }
        if *k == mine && index.is_none() {
            index = Some(count);
        }
        count += usize::from(!named(*n));
    }
    index.map_or((0, 1, false), |i| (i, count, false))
}

/// The block by its text and where its body starts.
fn key(block: &Block) -> u64 {
    hash(block.text.as_bytes()) ^ u64::from(block.body).rotate_left(32)
}

/// FNV-1a: stable across runs and hosts, as the state is kept in caches.
fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}
