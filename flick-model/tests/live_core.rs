//! flick-core against the model on regions by use
//! (`src/check/regions/live.rs`): the leaks of the generator,
//! the programs that only hold a region past its `scope`, generated
//! programs, and cases by hand. The machine judges what is accepted: a
//! leak must be rejected because it reaches a use after close.
//! `cargo test -p flick-model --test live_core -- --nocapture` prints the
//! counts; `LIVE_CORE_ROUNDS` sets how many programs are generated.

mod live_support;

use std::env;
use std::rc::Rc;

use flick_model::eval::Outcome;
use flick_model::generate;
use flick_model::syntax::{Const, Term};

use live_support::{core, g, open_in, probe, run, thunk_program, v, verdict};

/// xorshift: choices without a dependency.
struct Seeds(u64);

impl Seeds {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        u32::try_from(self.0 >> 32).expect("32 bits")
    }

    fn choices(&mut self, most: u32) -> Vec<u32> {
        let n = self.next() % most;
        (0..n).map(|_| self.next()).collect()
    }
}

fn rounds() -> u32 {
    env::var("LIVE_CORE_ROUNDS")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(300)
}

/// Every leak of the generator reaches a use after close, so flick-core
/// rejects each, as the model does.
#[test]
fn live_core_rejects_every_leak() {
    let mut seen = 0;
    let mut accepted = Vec::new();
    for path in 0..13 {
        let Some(p) = generate::leak_used(path) else {
            continue;
        };
        seen += 1;
        assert!(verdict(&p).is_err(), "path {path}: the model accepts");
        let errors = core(&p);
        println!("path {path}: {errors:?}");
        if errors.is_empty() {
            accepted.push(format!("path {path}:\n{}", generate::source(&p)));
        }
    }
    assert_eq!(seen, 11);
    assert!(
        accepted.is_empty(),
        "flick-core accepts leaks:\n{}",
        accepted.join("\n")
    );
}

/// `generate::holding`: what only holds a region past its `scope` is
/// accepted by flick-core and the model; with a use of it, rejected by both.
#[test]
fn live_core_on_holding() {
    let mut seeds = Seeds(0x2545_f491_4f6c_dd1d);
    let mut failures = Vec::new();
    let (mut held, mut used_rejected) = (0, 0);
    for _ in 0..rounds() {
        let c = seeds.choices(256);
        for used in [false, true] {
            let p = generate::holding(&c, used);
            let (model, errors) = (verdict(&p), core(&p));
            let agree = if used {
                used_rejected += usize::from(!errors.is_empty());
                model.is_err() && !errors.is_empty()
            } else {
                held += usize::from(errors.is_empty());
                model.is_ok() && errors.is_empty()
            };
            if !agree && failures.len() < 8 {
                failures.push(format!(
                    "used {used}: model {model:?}; flick-core {errors:?}\n{}",
                    generate::source(&p)
                ));
            }
        }
    }
    println!(
        "holding: {} shapes; flick-core accepts {held} held, rejects {used_rejected} used",
        rounds()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n---\n"));
}

/// Generated programs, and changes of them: flick-core under the live
/// discipline gives the model's verdict, and a change it accepts runs
/// without a use after close.
#[test]
fn live_core_agrees_on_generated() {
    let mut seeds = Seeds(0x9e37_79b9_7f4a_7c15);
    let mut failures = Vec::new();
    let (mut programs, mut accepted, mut rejected) = (0, 0, 0);
    for _ in 0..rounds() {
        let choices = seeds.choices(512);
        let original = generate::program(&choices);
        let change = seeds.choices(8);
        let changed = generate::mutate(&original, &change);
        for prog in [&original, &changed] {
            programs += 1;
            let (model, errors) = (verdict(prog), core(prog));
            let core_ok = errors.is_empty() || generate::by_design(&errors);
            if core_ok {
                accepted += 1;
            } else {
                rejected += 1;
            }
            let stuck = core_ok
                && errors.is_empty()
                && generate::runs(prog)
                && matches!(run(prog), Outcome::Stuck(w) if w.starts_with("use after"));
            if (model.is_ok() != core_ok || stuck) && failures.len() < 8 {
                failures.push(format!(
                    "model {model:?}; flick-core {errors:?}; stuck {stuck}\n{}",
                    generate::source(prog)
                ));
            }
        }
    }
    println!(
        "generated: {programs} programs and changes; flick-core accepts {accepted}, rejects {rejected}"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n---\n"));
}

/// A task that uses a region, stored in a cell made outside its `scope`
/// and awaited after it: a use after close. The cell's content and the cell
/// each bound what it may hold, and the task is within both, so the region
/// would leave its `scope` in the cell's type; flick-core bounds it by the
/// capacity of the cell, which lacks the region.
#[test]
fn live_core_rejects_a_task_in_an_outer_cell() {
    let task = |e: Term| Term::Task(Rc::new(e));
    let in_cell = |stored: Term| {
        let body = Term::Let(
            "cell".into(),
            None,
            Rc::new(Term::app(Term::konst(Const::RefMake), task(Term::unit()))),
            Rc::new(Term::Let(
                "_".into(),
                None,
                Rc::new(open_in("s", stored)),
                Rc::new(Term::Await(Rc::new(Term::app(
                    Term::konst(Const::RefGet),
                    v("cell"),
                )))),
            )),
        );
        thunk_program(body)
    };
    let set = |x: Term| Term::apps(Term::konst(Const::RefSet), [v("cell"), x]);
    let using = || task(Term::app(g("use"), v("h")));
    let direct = in_cell(set(using()));
    let named = in_cell(Term::Let(
        "t".into(),
        None,
        Rc::new(using()),
        Rc::new(set(v("t"))),
    ));
    for (name, p) in [
        ("task in an outer cell", direct),
        ("named task in an outer cell", named),
    ] {
        let (live, outcome) = probe(name, &p);
        assert!(matches!(outcome, Outcome::Stuck(w) if w.starts_with("use after")));
        assert!(live.is_err(), "{name}: the model accepts a use after close");
        let errors = core(&p);
        assert!(
            errors.iter().any(|e| e.starts_with("FLICK-G002")),
            "{name}: {errors:?}"
        );
        assert!(
            !errors.iter().any(|e| e.starts_with("FLICK-T003")),
            "{name}: {errors:?}"
        );
    }
}
