//! The plugins linked into the compiler: those of the packages that come
//! with it, which every build has, the browser's among them.

use std::sync::Arc;

use flick_plugin::{
    Arena, Block, CallSite, Compiler, Completion, DeriveTarget, Diagnostic, Fact, Highlight, Hover,
    Piece, Plugin, Published,
};
use flick_plugin_cli::Cli;
use flick_plugin_derive::{codec, server_fn};
use flick_plugin_refined::block as refined;
use flick_plugin_sql::Sql;
use flick_plugin_ui::{block as ui_block, calls, component};

use super::{Expander, Native};

pub use crate::syntax::heads::{BARE, DECLARATIONS, HEADS, NAMED};

/// The names whose places of call the plugin of std checks, as the `watch`
/// of a package declares them. Every module is watched for them without a
/// manifest, as every module has std's heads.
pub const WATCH: [&str; 16] = [
    "@cell",
    "@maker",
    "Atom.Read",
    "Prelude.Setup",
    "Ui.view",
    "Ui.Event.exhaust",
    "Ui.Event.latest",
    "Ui.Event.queue",
    "Ui.Event.concurrent",
    "Action.queue",
    "Action.concurrent",
    "*.parse",
    "Atom.serializable",
    "Atom.serializable_family",
    "Atom.serializable_task",
    "Atom.serializable_result",
];

/// The codes std's plugin gives that are the compiler's own, not shown
/// under its package: the checks that moved into it keep the code, the
/// reference page and the `flick explain` text they had. Its other codes,
/// such as `Q010`, stay `std/Q010`.
pub const OWN_CODES: [&str; 12] = [
    "U001", "U002", "U003", "U004", "U005", "U006", "U007", "D006", "E002", "T036", "T037", "T038",
];

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
        "server" | "codec" | "sql" | "component" | "ui" | "refined"
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
            "ui" => ui_block::expand(block, compiler),
            "refined" => refined::expand(block, compiler),
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

    fn check_structure(&self, block: &Block, facts: &flick_plugin::Structure) -> Vec<Diagnostic> {
        if block.head == "component" {
            component::check(block, facts)
        } else {
            Vec::new()
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
            "ui" => ui_block::hover(block, offset),
            "refined" => refined::hover(block, offset),
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
            "ui" => ui_block::format(block, compiler),
            "refined" => refined::format(block, compiler),
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
