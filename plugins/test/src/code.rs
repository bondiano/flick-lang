//! The code a block stands for: a suite is `Test.group name [cases]`, a
//! test `Test.case name (fun _ -> body)`, and a modifier wraps the case in
//! `Test.skip` or `Test.only`. The module's `tests` lists the case of each
//! block in the order written: a module of one block declares it alone; of
//! several, each block declares its case as `tests$i`, which no source can
//! name, and the first also `tests`, which names them all.

use flick_plugin::{Arena, Block, Builder, Expr, NodeId, flick};

use crate::read::{Item, Kind, Mode};

/// The declarations of `item`, the block `index` of the `count` blocks of
/// its module.
pub fn arena(block: &Block, item: &Item, index: usize, count: usize) -> Arena {
    let mut b = Builder::new(block.whole());
    let case = case(&mut b, item);
    let root = if count <= 1 {
        flick!(b, let tests : List Test.Test = [#case])
    } else {
        let own = flick!(b, let #(value(index)) = #case);
        if index == 0 {
            let cases: Vec<NodeId> = (0..count).map(|i| b.expr(Expr::Name(value(i)))).collect();
            let all = flick!(b, let tests : List Test.Test = [#(..cases)]);
            b.group(vec![own, all])
        } else {
            own
        }
    };
    b.finish(root)
}

/// The value the case of block `index` is, which no source can name.
fn value(index: usize) -> String {
    format!("tests${index}")
}

/// The `Test` of `item`, each call at the name of its case.
fn case(b: &mut Builder, item: &Item) -> NodeId {
    let at = item.name;
    let case = match &item.kind {
        Kind::Test(body) => {
            let body = if let Some(span) = body {
                flick!(b @ at, #span)
            } else {
                flick!(b @ at, ())
            };
            flick!(b @ at, Test.case #at (fun _ -> #body))
        }
        Kind::Suite(items) => {
            let cases: Vec<NodeId> = items.iter().map(|i| self::case(b, i)).collect();
            flick!(b @ at, Test.group #at [#(..cases)])
        }
    };
    match item.mode {
        Mode::Run => case,
        Mode::Skip => flick!(b @ at, Test.skip #case),
        Mode::Only => flick!(b @ at, Test.only #case),
    }
}
