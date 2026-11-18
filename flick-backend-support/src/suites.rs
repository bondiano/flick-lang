//! The test suites a build runs, one of each test module.

/// What the internal name of a test companion adds to its module's: a
/// companion `src/App/Users.test.fl` declares `module App.Users`, and is
/// checked and built as `App.Users$test`, a name no source can write, so no
/// module imports it.
pub const COMPANION: &str = "$test";

/// The module whose test companion `name`, an internal module name, is.
pub fn companion_of(name: &str) -> Option<&str> {
    name.strip_suffix(COMPANION)
}

/// The name the cases of the suite of `module` are shown under: those of a
/// test companion under its module's.
pub fn suite_name(module: &str) -> &str {
    companion_of(module).unwrap_or(module)
}
