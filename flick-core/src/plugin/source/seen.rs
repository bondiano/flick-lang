//! What the block's module sees, which answers `literal-of` and
//! `published`, and whether a kept call's answers are still the same.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::BlockSource;
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
            Asked::Published(name) => Some(published_answer(
                self.published.get(name).map(Vec::as_slice),
            )),
            _ => None,
        }
    }
}
