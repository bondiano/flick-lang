//! The packages every program sees without depending on them, in dependency
//! order, each `packages/<name>` with its own `src/modules.txt`. Adding one is
//! adding its name here and its directory: the compiler's build script reads
//! this file too.

/// The implicit packages. The first is the one `FLICK_STD_PATH` names; the
/// others are its siblings there.
pub const IMPLICIT_PACKAGES: &[&str] = &["std"];

/// Whether the package `name` is implicit.
pub fn is_implicit_package(name: &str) -> bool {
    IMPLICIT_PACKAGES.contains(&name)
}
