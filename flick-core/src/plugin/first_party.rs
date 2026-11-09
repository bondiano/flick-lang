//! The plugins linked into the compiler: those of the packages that come
//! with it, which every build has, the browser's among them.

use std::sync::Arc;

use flick_plugin::{
    Arena, Block, CallSite, Compiler, Completion, DeriveTarget, Diagnostic, Fact, Highlight, Hover,
    Piece, Plugin, Published,
};
use flick_plugin_cli::Cli;
use flick_plugin_derive::{codec, server_fn};
use flick_plugin_sql::Sql;
use flick_plugin_ui::{calls, component};

use super::{Expander, Native};

/// The heads of the plugins linked into the compiler and the module that
/// exposes each. Every module has them without an import: `cli "x" { … }`
/// is a block of std's plugin, and no other plugin may give the head.
pub const HEADS: [(&str, &str); 5] = [
    ("cli", "Cli"),
    ("server", "Server"),
    ("codec", "Codec"),
    ("sql", "Sql"),
    ("component", "Ui"),
];

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

/// The names whose places of call the plugin of std checks, as the `watch`
/// of a package declares them. Every module is watched for them without a
/// manifest, as every module has std's heads.
pub const WATCH: [&str; 5] = [
    "Ui.Event.queue",
    "Ui.Event.concurrent",
    "Action.queue",
    "Action.concurrent",
    "*.parse",
];

/// The codes std's plugin gives that are the compiler's own, not shown
/// under its package: the checks that moved into it keep the code, the
/// reference page and the `flick explain` text they had. Its other codes,
/// such as `Q010`, stay `std/Q010`.
pub const OWN_CODES: [&str; 2] = ["U001", "D006"];

/// The package whose plugin is linked into the compiler. An implicit package
/// without a plugin of its own needs no entry here.
pub const PACKAGE: &str = "std";

/// The plugin linked into the compiler for `package`.
pub fn plugin(package: &str) -> Option<Arc<dyn Expander>> {
    (package == PACKAGE).then(|| Arc::new(Native::new(PACKAGE, Std)) as Arc<dyn Expander>)
}

/// The plugin of std: `cli` blocks and `deriving Cli.Value`, `server fn`,
/// `codec`, `sql table`, and the derives of the witnesses of the Prelude.
struct Std;

/// Whether `block` is a `cli` block.
fn cli(block: &Block) -> bool {
    !matches!(
        block.head.as_str(),
        "server" | "codec" | "sql" | "component"
    )
}

impl Plugin for Std {
    // the state is the tables' and migrations' of `sql`: the others keep none
    fn collect(&self, blocks: &[Block], compiler: &mut dyn Compiler) -> Vec<u8> {
        let sql: Vec<Block> = (blocks.iter())
            .filter(|b| b.head == "sql")
            .cloned()
            .collect();
        Sql.collect(&sql, compiler)
    }

    fn publish(&self, state: &[u8]) -> Vec<Published> {
        Sql.publish(state)
    }

    fn expand(
        &self,
        block: &Block,
        state: &[u8],
        compiler: &mut dyn Compiler,
    ) -> Result<Arena, Vec<Diagnostic>> {
        match block.head.as_str() {
            "server" => server_fn::expand(block, compiler),
            "codec" => codec::expand(block, compiler),
            "sql" => Sql.expand(block, state, compiler),
            "component" => component::expand(block, compiler),
            _ => Cli.expand(block, state, compiler),
        }
    }

    fn check(&self, block: &Block, state: &[u8], facts: &[Fact]) -> Vec<Diagnostic> {
        match block.head.as_str() {
            "codec" => codec::check(block, facts),
            "sql" => Sql.check(block, state, facts),
            _ => Vec::new(),
        }
    }

    // each takes the names of [`WATCH`] it owns: the event policies, and
    // the `parse` of a refined type
    fn check_calls(&self, calls: &[CallSite], _state: &[u8]) -> Vec<Diagnostic> {
        let mut found = calls::check(calls);
        found.extend(flick_plugin_refined::check(calls));
        found
    }

    fn complete(
        &self,
        block: &Block,
        state: &[u8],
        offset: u32,
        member: bool,
        compiler: &mut dyn Compiler,
    ) -> Vec<Completion> {
        match block.head.as_str() {
            "sql" => Sql.complete(block, state, offset, member, compiler),
            _ if cli(block) => Cli.complete(block, state, offset, member, compiler),
            _ => Vec::new(),
        }
    }

    fn hover(&self, block: &Block, state: &[u8], offset: u32, facts: &[Fact]) -> Option<Hover> {
        match block.head.as_str() {
            "codec" => codec::hover(block, offset, facts),
            "sql" => Sql.hover(block, state, offset, facts),
            "component" => component::hover(block, offset),
            "server" => None,
            _ => Cli.hover(block, state, offset, facts),
        }
    }

    fn highlight(&self, block: &Block, state: &[u8]) -> Vec<Highlight> {
        if cli(block) {
            Cli.highlight(block, state)
        } else {
            Vec::new()
        }
    }

    fn format(
        &self,
        block: &Block,
        state: &[u8],
        compiler: &mut dyn Compiler,
    ) -> Option<Vec<Piece>> {
        match block.head.as_str() {
            "codec" => codec::format(block, compiler),
            "sql" => Sql.format(block, state, compiler),
            "component" => component::format(block, compiler),
            "server" => None,
            _ => Cli.format(block, state, compiler),
        }
    }

    fn derive(
        &self,
        target: &DeriveTarget,
        class: &str,
        args: Option<&Block>,
        module: &[DeriveTarget],
        state: &[u8],
        compiler: &mut dyn Compiler,
    ) -> Result<Arena, Vec<Diagnostic>> {
        if flick_plugin_derive::CLASSES.contains(&class) {
            return flick_plugin_derive::derive(target, class, args, module);
        }
        Cli.derive(target, class, args, module, state, compiler)
    }
}
