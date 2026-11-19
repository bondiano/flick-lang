//! The code a block stands for: a suite is `Test.group name [cases]`, a
//! test `Test.effect_test name (task { body })` and a hook of a suite
//! `Test.before_each (task { body })`, as they are written by hand; a
//! timeout wraps the case in `Test.timeout millis` and a modifier in
//! `Test.skip` or `Test.only`. The
//! module's `tests` lists the case of each
//! block in the order written: a module of one block declares it alone; of
//! several, each block declares its case as `tests$i`, which no source can
//! name, and the first also `tests`, which names them all.

use flick_plugin::{Arena, Block, Builder, Expr, NodeId, flick};

use crate::read::{Item, Kind, Mode};

/// The declarations of `item`, the block `index` of the `count` blocks of
/// its module.
pub fn arena(block: &Block, item: &Item, index: usize, count: usize) -> Arena {
    let mut b = Builder::new(block.whole());
    let case = case(&mut b, block, item);
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
fn case(b: &mut Builder, block: &Block, item: &Item) -> NodeId {
    let at = item.name;
    let task = |b: &mut Builder, body: &Option<_>| {
        let body = if let Some(span) = body {
            flick!(b @ at, #span)
        } else {
            flick!(b @ at, ())
        };
        b.at(at).expr(Expr::Task(body))
    };
    let mut case = match &item.kind {
        Kind::Test(body) => {
            let task = task(b, body);
            flick!(b @ at, Test.effect_test #at #task)
        }
        Kind::Hook(body) => {
            let task = task(b, body);
            match block.text_of(at) {
                "before_all" => flick!(b @ at, Test.before_all #task),
                "before_each" => flick!(b @ at, Test.before_each #task),
                "after_each" => flick!(b @ at, Test.after_each #task),
                _ => flick!(b @ at, Test.after_all #task),
            }
        }
        Kind::Suite(items) => {
            let cases: Vec<NodeId> = items.iter().map(|i| self::case(b, block, i)).collect();
            flick!(b @ at, Test.group #at [#(..cases)])
        }
    };
    if let Some((_, millis)) = item.timeout {
        let millis = flick!(b @ at, #millis);
        case = flick!(b @ at, Test.timeout #millis #case);
    }
    match item.mode {
        Mode::Run => case,
        Mode::Skip => flick!(b @ at, Test.skip #case),
        Mode::Only => flick!(b @ at, Test.only #case),
    }
}
