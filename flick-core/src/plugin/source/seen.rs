//! What the block's module sees, which answers `literal-of` and
//! `published`, and whether a kept call's answers are still the same.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::{BlockSource, InScope};
use crate::plugin::reach::{Asked, Used, literal_answer, published_answer};

impl BlockSource {
    /// What the plugin published of the names the block's module sees, as
    /// it writes them.
    pub fn see_published(&mut self, published: Arc<BTreeMap<String, Vec<u8>>>) {
        self.published = published;
    }

    /// Whether each answer a kept call of this block got is still the
    /// same: what the module sees answers `literal-of` and `published`, the
    /// gate the rest.
    pub fn still(&self, used: &[Used]) -> bool {
        let (own, outside): (Vec<&Used>, Vec<&Used>) =
            used.iter().partition(|u| u.asked.by_module());
        let outside: Vec<Used> = outside.into_iter().cloned().collect();
        own.iter()
            .all(|u| self.answer(&u.asked) == Some(u.answer.clone()))
            && self.reach.still(&outside)
    }

    /// The answer, as it is kept, to what the module sees.
    fn answer(&self, asked: &Asked) -> Option<String> {
        match asked {
            Asked::LiteralOf(name) => {
                Some(literal_answer(self.enums.get(name).map(String::as_str)))
            }
            Asked::PublishedNames => {
                let names: Vec<&str> = self.published.keys().map(String::as_str).collect();
                Some(names.join("\n"))
            }
            Asked::Published(name) => Some(published_answer(
                self.published.get(name).map(Vec::as_slice),
            )),
            _ => None,
        }
    }

    /// What the module has in scope, for its plugin's completion in an
    /// editor.
    pub fn see_scope(&mut self, in_scope: Arc<InScope>) {
        self.in_scope = Some(in_scope);
    }

    /// What this block's module sees, for its plugin's questions of later.
    pub fn seen(&self) -> Seen {
        Seen {
            enums: Arc::clone(&self.enums),
            published: Arc::clone(&self.published),
        }
    }
}

/// What a module sees that answers its plugin's questions: the enums and
/// what the plugin published, as it writes them.
#[derive(Clone, Default)]
pub struct Seen {
    enums: Arc<BTreeMap<String, String>>,
    published: Arc<BTreeMap<String, Vec<u8>>>,
}

impl Seen {
    /// A source of no block that sees it.
    pub fn source(&self) -> BlockSource {
        BlockSource {
            enums: Arc::clone(&self.enums),
            published: Arc::clone(&self.published),
            ..BlockSource::default()
        }
    }
}
