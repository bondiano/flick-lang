//! The heads of the blocks of the compiler's own plugins, which the parser
//! reads without an import.

/// The heads of the plugins linked into the compiler and the module that
/// exposes each. Every module has them without an import: `cli "x" { … }`
/// is a block of std's plugin, and no other plugin may give the head.
pub const HEADS: [(&str, &str); 7] = [
    ("cli", "Cli"),
    ("server", "Server"),
    ("codec", "Codec"),
    ("sql", "Sql"),
    ("component", "Ui"),
    ("ui", "Ui"),
    ("refined", "Checks"),
];

/// The word of generated code before the module a body opens: `open$module
/// Ui (body)`. No source writes a name with `$` in it.
pub const OPEN: &str = "open$module";

/// The attribute of generated code before a function a constant is given
/// to, naming the value that reads the constant: `@checked$by Port$check`.
pub const CHECKED_BY: &str = "@checked$by";

/// The heads whose blocks need no body even where no import exposes them,
/// as the compiler read them before: `server fn f x = …` and `server f`.
pub const BARE: [&str; 1] = ["server"];

/// The declarations of the compiler's own plugins even where no import
/// exposes their heads, by the head and the word after it: `server fn f x
/// = …`, `sql table Users "users" { … }`, `sql migration 1 "m" { … }`,
/// `refined type Email = String where Checks.email`.
pub const DECLARATIONS: [(&str, &str); 4] = [
    ("server", "fn"),
    ("sql", "table"),
    ("sql", "migration"),
    ("refined", "type"),
];

/// The heads that declare a name right after them, whatever word it is:
/// `component counter (start : Float) = …`.
pub const NAMED: [&str; 1] = ["component"];
