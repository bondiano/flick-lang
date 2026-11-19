//! What the fixtures of a suite may use, read from their tokens: a fixture
//! uses only those declared before it, a shared one only shared ones, and so
//! do the `before_all` and `after_all` hooks, which run for no test; a
//! `parallel` suite shares only `concurrent` fixtures.

use flick_plugin::{Block, Diagnostic, Span, TokenKind};

use crate::read::{DUPLICATE, Fixture, Item, Kind};

/// A fixture named where it is not there yet, or not for that code.
pub const UNAVAILABLE: &str = "T003";

/// A shared fixture of a `parallel` suite that is not `concurrent`.
pub const NOT_CONCURRENT: &str = "T004";

/// A fixture in sight of the code of a suite, by its name.
#[derive(Clone, Copy)]
struct Visible<'a> {
    name: &'a str,
    shared: bool,
}

/// What is wrong with the fixtures under `item`, a suite, into `out`.
pub fn check(block: &Block, item: &Item, out: &mut Vec<Diagnostic>) {
    walk(block, item, &[], out);
}

fn walk(block: &Block, item: &Item, outer: &[Visible], out: &mut Vec<Diagnostic>) {
    let Kind::Suite(items) = &item.kind else {
        return;
    };
    let own: Vec<(&Item, &Fixture)> = (items.iter())
        .filter_map(|i| match &i.kind {
            Kind::Fixture(f) => Some((i, f)),
            _ => None,
        })
        .collect();
    let mut sight: Vec<Visible> = outer.to_vec();
    for (k, (fixture, f)) in own.iter().enumerate() {
        let name = block.text_of(fixture.name);
        if own[..k]
            .iter()
            .any(|(other, _)| block.text_of(other.name) == name)
        {
            let message = format!("another fixture of this suite is named `{name}` too");
            out.push(Diagnostic::error(message, fixture.name).with_code(DUPLICATE));
        }
        if let (Some(shared), None, Some(_)) = (f.shared, f.concurrent, item.parallel) {
            let message = format!(
                "a `parallel` suite runs its tests at once, so its shared fixture `{name}` must say that they may use it at once: `fixture shared concurrent {name} = …`"
            );
            out.push(Diagnostic::error(message, shared).with_code(NOT_CONCURRENT));
        }
        let later: Vec<&str> = own[k..]
            .iter()
            .map(|(i, _)| block.text_of(i.name))
            .collect();
        for at in names(block, f.expr) {
            let used = block.text_of(at);
            // the name is the last fixture of that name in sight, as a `let` it shadows
            match sight.iter().rev().find(|v| v.name == used) {
                Some(seen) if f.shared.is_some() && !seen.shared => {
                    let message = format!(
                        "shared fixture `{name}` uses `{used}`, which each test makes anew: a shared fixture may use only shared ones"
                    );
                    out.push(Diagnostic::error(message, at).with_code(UNAVAILABLE));
                }
                None if later.contains(&used) => {
                    let message = if used == name {
                        format!(
                            "fixture `{name}` uses itself: a fixture may use only the fixtures declared before it"
                        )
                    } else {
                        format!(
                            "fixture `{name}` uses `{used}`, declared after it: a fixture may use only the fixtures declared before it"
                        )
                    };
                    out.push(Diagnostic::error(message, at).with_code(UNAVAILABLE));
                }
                _ => {}
            }
        }
        sight.push(Visible {
            name,
            shared: f.shared.is_some(),
        });
    }
    for inner in items {
        match &inner.kind {
            Kind::Hook(body) => {
                let hook = inner.word.map_or("", |w| block.text_of(w));
                if !hook.ends_with("_all") {
                    continue;
                }
                for at in names(block, *body) {
                    let used = block.text_of(at);
                    if sight
                        .iter()
                        .rev()
                        .find(|v| v.name == used)
                        .is_some_and(|v| !v.shared)
                    {
                        let message = format!(
                            "`{hook}` runs for no test, so it cannot use `{used}`, which each test makes anew: it may use only shared fixtures"
                        );
                        out.push(Diagnostic::error(message, at).with_code(UNAVAILABLE));
                    }
                }
            }
            Kind::Suite(_) => walk(block, inner, &sight, out),
            Kind::Test(_) | Kind::Fixture(_) => {}
        }
    }
}

/// The names in the code at `span`, as tokens: what may be a fixture.
pub fn names(block: &Block, span: Option<Span>) -> Vec<Span> {
    let Some(span) = span else {
        return Vec::new();
    };
    (block.tokens.iter())
        .filter(|t| t.kind == TokenKind::Ident && span.start <= t.span.start && t.span.end <= span.end)
        // a field `x.name` names no fixture
        .filter(|t| t.span.start == 0 || block.text.as_bytes()[t.span.start as usize - 1] != b'.')
        .map(|t| t.span)
        .collect()
}
