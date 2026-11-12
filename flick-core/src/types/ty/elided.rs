//! The mark of a qualifier the checker read into a signature, not one the
//! signature writes: the hidden row of data with hidden storage written bare
//! (`docs/formal/regions.md` §1.4). It changes no meaning: every two marks
//! are equal, as `Written` is; messages leave a marked qualifier out.

use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Elided(pub bool);

impl Elided {
    pub fn is_no(&self) -> bool {
        !self.0
    }
}

impl PartialEq for Elided {
    fn eq(&self, _: &Elided) -> bool {
        true
    }
}

impl Eq for Elided {}

impl Hash for Elided {
    fn hash<H: Hasher>(&self, _: &mut H) {}
}
