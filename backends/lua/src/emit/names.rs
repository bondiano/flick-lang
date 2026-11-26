//! Lua names for the bindings of a top-level definition. IR locals are
//! unique within their definition, so each gets one Lua name for all of it:
//! two bindings never share a name, and a closure captures the binding it
//! means. No name is a Lua keyword or one the generated code reads itself.

use std::collections::{BTreeMap, BTreeSet};

use super::ast::Name;

/// The reserved words of Lua 5.1, and `goto`, which `LuaJIT` reserves.
const KEYWORDS: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if", "in",
    "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

/// The host globals generated code calls; every profile's API snapshot
/// defines them, as a test checks.
pub(crate) const GLOBALS: &[&str] = &["pairs", "setmetatable", "type"];

/// The table of the module being lowered, the private namespace that
/// holds every module's table, and the local each chunk keeps the Lua
/// kernel's table `ns.k` in.
pub(crate) const MODULE: &str = "M";
pub(crate) const NAMESPACE: &str = "ns";
pub(crate) const KERNEL: &str = "K";

/// The name of what is never read: a parameter a pattern ignores.
const UNUSED: &str = "_";

/// Whether `text` is a Lua identifier and not a keyword.
pub fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !KEYWORDS.contains(&text)
}

/// The names of one top-level definition.
pub struct Namer {
    used: BTreeSet<String>,
    /// The name of each IR local, once it has one.
    locals: BTreeMap<String, Name>,
}

impl Namer {
    pub fn new() -> Namer {
        let used = (GLOBALS.iter().chain(&[MODULE, NAMESPACE, KERNEL, UNUSED]))
            .map(|&n| n.to_owned())
            .collect();
        Namer {
            used,
            locals: BTreeMap::new(),
        }
    }

    /// A name no other binding has, spelled after `hint`: its letters,
    /// digits and underscores, `_` for anything else, and a suffix when
    /// that is taken.
    pub fn fresh(&mut self, hint: &str) -> Name {
        let mut base: String = hint
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        if !base.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
            base.insert(0, '_');
        }
        let mut text = base.clone();
        let mut n = 1;
        while KEYWORDS.contains(&text.as_str()) || self.used.contains(&text) {
            n += 1;
            text = format!("{base}_{n}");
        }
        self.used.insert(text.clone());
        Name::new(text)
    }

    /// Keeps `name` from every binding: a global the code reads.
    pub fn reserve(&mut self, name: &str) {
        self.used.insert(name.to_owned());
    }

    /// The name of the IR local `local`, the same at every use.
    pub fn local(&mut self, local: &str) -> Name {
        if let Some(name) = self.locals.get(local) {
            return name.clone();
        }
        let name = self.fresh(local);
        self.locals.insert(local.to_owned(), name.clone());
        name
    }

    /// The name of a binding nothing reads, which any number of them share.
    pub fn unused() -> Name {
        Name::new(UNUSED.to_owned())
    }
}
