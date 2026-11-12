//! What checking a module is given besides the module and its imports: the
//! bounds of instance resolution and the settings of the project.

use super::Watched;
use crate::syntax::structure::Thresholds;

/// Bounds of instance resolution for one definition; exceeding them is a
/// diagnostic, never a success.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub steps: u32,
    pub depth: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            steps: 100_000,
            depth: 256,
        }
    }
}

/// What checking a module needs beyond its imports.
#[derive(Clone, Copy)]
pub struct Options<'a> {
    /// Whether a module belongs to the package being checked: only the
    /// package of a class or of the type an instance is for may declare it.
    pub local: &'a dyn Fn(&str) -> bool,
    pub limits: Limits,
    /// `[lint] no-let-assert`: every `let assert` is an error.
    pub no_let_assert: bool,
    pub thresholds: Thresholds,
    /// The names whose places of call the plugins of the project watch:
    /// each one checked gives a [`super::Typing::call_sites`].
    pub watched: &'a Watched,
    /// Whether a stage comes from the classes of the operations of its row, as
    /// on Flick's own runtime: see [`super::effects::Effects::direct`]. Every module of a build
    /// and the built-in modules it reads are checked alike.
    pub stages_by_class: bool,
}
