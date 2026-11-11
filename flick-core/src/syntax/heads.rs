//! The heads of the blocks of the compiler's own plugins, which the parser
//! reads without an import.

/// The heads of the plugins linked into the compiler and the module that
/// exposes each. Every module has them without an import: `cli "x" { … }`
/// is a block of std's plugin, and no other plugin may give the head.
pub const HEADS: [(&str, &str); 6] = [
    ("cli", "Cli"),
    ("server", "Server"),
    ("codec", "Codec"),
    ("sql", "Sql"),
    ("component", "Ui"),
    ("ui", "Ui"),
];

/// The word of generated code before the module a body opens: `open$module
/// Ui (body)`. No source writes a name with `$` in it.
pub const OPEN: &str = "open$module";

/// The heads whose blocks need no body even where no import exposes them,
/// as the compiler read them before: `server fn f x = …` and `server f`.
pub const BARE: [&str; 1] = ["server"];

/// The declarations of the compiler's own plugins even where no import
/// exposes their heads, by the head and the word after it: `server fn f x
/// = …`, `sql table Users "users" { … }`, `sql migration 1 "m" { … }`.
pub const DECLARATIONS: [(&str, &str); 3] =
    [("server", "fn"), ("sql", "table"), ("sql", "migration")];

/// The heads that declare a name right after them, whatever word it is:
/// `component counter (start : Float) = …`.
pub const NAMED: [&str; 1] = ["component"];
