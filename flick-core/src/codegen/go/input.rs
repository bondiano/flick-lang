//! What a Go program is compiled from: the IR of its modules and the data of
//! the build the code needs, nothing of the compiler that checked them.

use std::collections::BTreeMap;

use super::Bindings;
use crate::codegen::Known;
use crate::ir::Module;

/// What a program is made of.
pub struct Input<'a> {
    /// The program's own modules, whose sources the build has.
    pub own: &'a [&'a Module],
    /// The built-in modules; their spans are in no source of the build.
    pub builtins: &'a [&'a Module],
    /// The module whose `main` the program runs.
    pub entry: &'a str,
    /// The test modules whose `tests` the program runs instead, when any.
    pub suites: &'a [String],
    /// The source path of each own module, relative to the Go module, for the
    /// `//line` directives that give Go's stack traces the Flick source.
    pub paths: &'a BTreeMap<String, String>,
    /// The offsets where the lines of each source file start, by `FileId`.
    pub lines: &'a [&'a [u32]],
    /// The Go side of a foreign module of an own module, by the module and
    /// its relative specifier: the file's name in the Go module and its text.
    pub foreign: &'a BTreeMap<(String, String), (String, String)>,
    /// The functions of the runtime that JS exports are bound to.
    pub bindings: &'a Bindings,
    /// The program's lang items, the built-in entities the code knows.
    pub lang: &'a Known<'a>,
}

impl<'a> Input<'a> {
    /// The program's own module `name`.
    pub(super) fn own_module(&self, name: &str) -> Option<&'a Module> {
        self.own.iter().copied().find(|m| m.name == name)
    }
}
