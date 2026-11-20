//! The code a block stands for: a suite is `Test.group name [cases]`, a
//! test `Test.effect_test name (task { body })` and a hook of a suite
//! `Test.before_each (task { body })`, as they are written by hand; a
//! timeout wraps the case in `Test.timeout millis`, a suite that shares a
//! fixture in `Test.serial`, unless it is `parallel`, and a modifier in
//! `Test.skip` or `Test.only`. The module's `tests` lists the case of each
//! block in the order written: a module of one block declares it alone; of
//! several, each block declares its case as `tests$i`, which no source can
//! name, and the first also `tests`, which names them all.
//!
//! A fixture is no case: it is a value of the block, `tests$i$fixture$k =
//! Test.fixture key (task { expression })`, or `Test.shared` with an item
//! `Test.share` in its suite's list. Code that uses fixtures, a task of the
//! suite or the initializer of another fixture, is `match Test.use
//! tests$i$fixture$k with | name -> code`, the binders the user's own names,
//! which the user's code sees, and whose types are known before it; `flick
//! test` gives each task of one test, or of one run of the suite, the same
//! `Lazy`.

use std::mem;

use flick_plugin::{
    Arena, Arm, Block, Builder, Decl, Expr, NodeId, Pat, Span, flick, string_value,
};

use crate::fixtures::names;
use crate::read::{Each, Item, Kind, Mode};

/// The declarations of `item`, the block `index` of the `count` blocks of
/// its module.
pub fn arena(block: &Block, item: &Item, index: usize, count: usize) -> Arena {
    let mut b = Builder::new(block.whole());
    let mut decls = Vec::new();
    let sight = Sight {
        block: index,
        ..Sight::default()
    };
    let cases = list(&mut b, block, &[item], &sight, &mut decls, Vec::new());
    if count <= 1 {
        decls.push(flick!(b, let tests : List Test.Test = #cases));
    } else {
        decls.push(flick!(b, let #(value(index)) = #cases));
        if index == 0 {
            let cases: Vec<NodeId> = (0..count).map(|i| b.expr(Expr::Name(value(i)))).collect();
            decls.push(flick!(b, let tests : List Test.Test = List.concat [#(..cases)]));
        }
    }
    let root = if decls.len() == 1 {
        decls[0]
    } else {
        b.group(decls)
    };
    b.finish(root)
}

/// The list of the cases of `items` after `lead`: `[a, b]`, or, with the
/// cases of a `test.each` among them, `List.concat [[a], cases, [b]]`.
fn list(
    b: &mut Builder,
    block: &Block,
    items: &[&Item],
    sight: &Sight,
    decls: &mut Vec<NodeId>,
    lead: Vec<NodeId>,
) -> NodeId {
    let at = items.first().map_or(block.whole(), |i| i.name);
    let mut parts: Vec<NodeId> = Vec::new();
    let mut ones = lead;
    for item in items {
        if let Kind::Each(rows) = &item.kind {
            if !ones.is_empty() {
                let run = mem::take(&mut ones);
                parts.push(flick!(b @ at, [#(..run)]));
            }
            parts.push(each(b, block, item, rows, sight));
        } else {
            ones.push(case(b, block, item, sight, decls));
        }
    }
    if parts.is_empty() {
        return flick!(b @ at, [#(..ones)]);
    }
    if !ones.is_empty() {
        parts.push(flick!(b @ at, [#(..ones)]));
    }
    if let [only] = parts.as_slice() {
        return *only;
    }
    flick!(b @ at, List.concat [#(..parts)])
}

/// The cases of `item`, a `test.each`: `List.map_indexed (fun index$0 row$0 ->
/// case) rows`, each case named by `Test.case_name` and matching its row
/// with the user's pattern around the body.
fn each(b: &mut Builder, block: &Block, item: &Item, rows: &Each, sight: &Sight) -> NodeId {
    let at = item.name;
    let body = if let Some(span) = rows.body {
        flick!(b @ at, #span)
    } else {
        flick!(b @ at, ())
    };
    let row = b.at(at).expr(Expr::Name("row$0".into()));
    let pattern = b.at(rows.pattern).pat(Pat::Hole(rows.pattern));
    let matched = b.at(at).expr(Expr::Match {
        scrutinee: row,
        arms: vec![Arm {
            pattern,
            guard: None,
            body,
        }],
    });
    let code = with_fixtures(b, block, rows.body, matched, sight, at);
    let task = b.at(at).expr(Expr::Task(code));
    // the parts of a row are shown in the name only when a `{k}` of the name asks for them
    let template = string_value(block.text_of(at));
    let asked = (0..rows.parts).any(|k| template.contains(&format!("{{{k}}}")));
    let shown = if !asked {
        flick!(b @ at, [])
    } else if rows.parts == 1 {
        let row = b.at(at).expr(Expr::Name("row$0".into()));
        flick!(b @ at, [show #row])
    } else {
        let names: Vec<String> = (0..rows.parts).map(|k| format!("part${k}")).collect();
        let shows: Vec<NodeId> = (names.iter())
            .map(|n| {
                let part = b.at(at).expr(Expr::Name(n.clone()));
                flick!(b @ at, show #part)
            })
            .collect();
        let list = flick!(b @ at, [#(..shows)]);
        let pats: Vec<NodeId> = names
            .iter()
            .map(|n| b.at(at).pat(Pat::Var(n.clone())))
            .collect();
        let pattern = b.at(at).pat(Pat::Tuple(pats));
        let row = b.at(at).expr(Expr::Name("row$0".into()));
        b.at(at).expr(Expr::Match {
            scrutinee: row,
            arms: vec![Arm {
                pattern,
                guard: None,
                body: list,
            }],
        })
    };
    let index = b.at(at).expr(Expr::Name("index$0".into()));
    let name = flick!(b @ at, Test.case_name #at #index #shown);
    let mut case = flick!(b @ at, Test.effect_test #name #task);
    if let Some((_, millis)) = item.timeout {
        let millis = flick!(b @ at, #millis);
        case = flick!(b @ at, Test.timeout #millis #case);
    }
    let params = vec![
        b.at(at).pat(Pat::Var("index$0".into())),
        b.at(at).pat(Pat::Var("row$0".into())),
    ];
    let lambda = b.at(at).expr(Expr::Lambda { params, body: case });
    let rows = flick!(b @ at, #(rows.rows));
    flick!(b @ at, List.map_indexed #lambda #rows)
}

/// The value the case of block `index` is, which no source can name.
fn value(index: usize) -> String {
    format!("tests${index}")
}

/// A fixture in sight of the code of a suite: its name, where the user
/// wrote it, and the value of the block it is.
#[derive(Clone)]
struct Seen {
    name: String,
    at: Span,
    value: String,
}

/// The fixtures in sight, in the order declared, outermost first, the path
/// of the suite of the code, and which block of the module it is in.
#[derive(Clone, Default)]
struct Sight {
    fixtures: Vec<Seen>,
    path: String,
    block: usize,
}

/// The `Test` of `item`, each call at the name of its case; the functions
/// of its fixtures go into `decls`.
fn case(
    b: &mut Builder,
    block: &Block,
    item: &Item,
    sight: &Sight,
    decls: &mut Vec<NodeId>,
) -> NodeId {
    let at = item.name;
    let task = |b: &mut Builder, written: &Option<Span>, sight: &Sight| {
        let body = if let Some(span) = written {
            flick!(b @ at, #span)
        } else {
            flick!(b @ at, ())
        };
        let body = with_fixtures(b, block, *written, body, sight, at);
        b.at(at).expr(Expr::Task(body))
    };
    let mut case = match &item.kind {
        Kind::Test(body) => {
            let task = task(b, body, sight);
            flick!(b @ at, Test.effect_test #at #task)
        }
        Kind::Hook(body) => {
            let task = task(b, body, sight);
            match block.text_of(at) {
                "before_all" => flick!(b @ at, Test.before_all #task),
                "before_each" => flick!(b @ at, Test.before_each #task),
                "after_each" => flick!(b @ at, Test.after_each #task),
                _ => flick!(b @ at, Test.after_all #task),
            }
        }
        Kind::Suite(items) => {
            let (inner, shares) = within(b, block, item, items, sight, decls);
            let lead: Vec<NodeId> = (shares.iter())
                .map(|value| {
                    let value = b.at(at).expr(Expr::Name(value.clone()));
                    flick!(b @ at, Test.share #value)
                })
                .collect();
            let cases: Vec<&Item> = items
                .iter()
                .filter(|i| !matches!(i.kind, Kind::Fixture(_)))
                .collect();
            let cases = list(b, block, &cases, &inner, decls, lead);
            let group = flick!(b @ at, Test.group #at #cases);
            // a suite whose tests share a fixture runs them one at a time
            if !shares.is_empty() && item.parallel.is_none() {
                flick!(b @ at, Test.serial #group)
            } else {
                group
            }
        }
        Kind::Fixture(_) | Kind::Each(_) => flick!(b @ at, ()),
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

/// The sight of the code inside `item`, a suite in sight of `outer`: the
/// fixtures around and its own, whose values go into `decls`; and the
/// values of its shared ones.
fn within(
    b: &mut Builder,
    block: &Block,
    item: &Item,
    items: &[Item],
    outer: &Sight,
    decls: &mut Vec<NodeId>,
) -> (Sight, Vec<String>) {
    let name = string_value(block.text_of(item.name));
    let path = if outer.path.is_empty() {
        name
    } else {
        format!("{} > {name}", outer.path)
    };
    let mut sight = Sight {
        fixtures: outer.fixtures.clone(),
        path,
        block: outer.block,
    };
    let mut shares = Vec::new();
    for i in items {
        let Kind::Fixture(f) = &i.kind else { continue };
        let name = block.text_of(i.name).to_owned();
        let at = f.expr.unwrap_or(i.name);
        let expr = if let Some(span) = f.expr {
            flick!(b @ at, #span)
        } else {
            flick!(b @ at, ())
        };
        // the initializer sees the fixtures before it that it uses
        let body = with_fixtures(b, block, f.expr, expr, &sight, at);
        let init = b.at(at).expr(Expr::Task(body));
        let key = b
            .at(at)
            .expr(Expr::Text(format!("{} > {name}", sight.path)));
        let made = if f.shared.is_some() {
            flick!(b @ at, Test.shared #key #init)
        } else {
            flick!(b @ at, Test.fixture #key #init)
        };
        let value = format!("tests${}$fixture${}", sight.block, decls.len());
        decls.push(b.at(at).decl(Decl::Let {
            name: value.clone(),
            params: Vec::new(),
            ty: None,
            body: made,
            rec: false,
        }));
        if f.shared.is_some() {
            shares.push(value.clone());
        }
        sight.fixtures.push(Seen {
            name,
            at: i.name,
            value,
        });
    }
    (sight, shares)
}

/// `code`, the user's code at `written`, with each fixture in sight that it
/// names: `match (Test.use value, …) with | (name, …) -> code`, in the order
/// declared; a name declared twice is the later one.
fn with_fixtures(
    b: &mut Builder,
    block: &Block,
    written: Option<Span>,
    code: NodeId,
    sight: &Sight,
    at: Span,
) -> NodeId {
    let used: Vec<String> = (names(block, written).into_iter())
        .map(|s| block.text_of(s).to_owned())
        .collect();
    let (mut params, mut arguments) = (Vec::new(), Vec::new());
    for (k, f) in sight.fixtures.iter().enumerate() {
        let hidden = sight.fixtures[k + 1..]
            .iter()
            .any(|later| later.name == f.name);
        if hidden || !used.contains(&f.name) {
            continue;
        }
        // the binder is the user's name, which the user's code sees
        params.push(b.at(f.at).pat(Pat::Hole(f.at)));
        let value = b.at(at).expr(Expr::Name(f.value.clone()));
        arguments.push(flick!(b @ at, Test.use #value));
    }
    let (pattern, scrutinee) = match (params.len(), params.first(), arguments.first()) {
        (0, ..) => return code,
        (1, Some(&p), Some(&a)) => (p, a),
        _ => (
            b.at(at).pat(Pat::Tuple(params)),
            b.at(at).expr(Expr::Tuple(arguments)),
        ),
    };
    let arm = Arm {
        pattern,
        guard: None,
        body: code,
    };
    b.at(at).expr(Expr::Match {
        scrutinee,
        arms: vec![arm],
    })
}
