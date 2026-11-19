//! Tests of the parser, the syntax structure and the suggestions of the
//! diagnostics that also run the checker, the formatter or the lowering,
//! which these crates do not depend on.

use crate::source::SourceMap;
use crate::syntax::{Parse, parse};

mod module_values;
mod modules;
mod plugins;
mod structure;
mod suggest;

fn parse_str(src: &str) -> Parse {
    let mut map = SourceMap::default();
    let file = map.add("t.fl", src);
    parse(src, file)
}

fn parse_ok(src: &str) -> Parse {
    let parsed = parse_str(src);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    parsed
}
