//! The spike of `src/check/regions/live.rs`: the leaks of the generator,
//! the generated programs and their changes, and the cases of
//! `tests/run/model.txt` under the discipline where a region is live where
//! a row has `Live` of it. A change the discipline accepts must run without
//! a use after close or another stuck state: the machine is the judge, not
//! the other checker. `cargo test -p flick-model --test live -- --nocapture`
//! prints what it counted.

mod live_support;

use std::collections::{BTreeSet, HashSet};
use std::env;
use std::fs;
use std::path::Path;
use std::rc::Rc;

use flick_model::check::check_program_with;
use flick_model::decls::Decls;
use flick_model::eval::Outcome;
use flick_model::generate;
use flick_model::run;
use flick_model::syntax::Name;
use flick_model::types::{Caps, Fields, Kind, Scheme, Type, Var};

use live_support::{core, run, verdict};

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

#[test]
fn live_rejects_every_leak() {
    let mut seen = 0;
    for path in 0..13 {
        let Some(p) = generate::leak_used(path) else {
            continue;
        };
        let source = generate::source(&p);
        let live = verdict(&p, true);
        println!("path {path}: {live:?}");
        assert!(live.is_err(), "path {path}: live accepts\n{source}");
        assert!(
            matches!(run(&p), Outcome::Stuck(w) if w.starts_with("use after")),
            "path {path}: the leak does not reach a use after close"
        );
        seen += 1;
    }
    assert_eq!(seen, 11);
}

#[derive(Default, Debug)]
struct Count {
    both: u32,
    only_captures: u32,
    only_live: u32,
    neither: u32,
}

impl Count {
    fn add(&mut self, captures: bool, live: bool) {
        match (captures, live) {
            (true, true) => self.both += 1,
            (true, false) => self.only_captures += 1,
            (false, true) => self.only_live += 1,
            (false, false) => self.neither += 1,
        }
    }
}

/// Generated programs and changes of them: how the two disciplines judge
/// them, and every one the live discipline accepts runs without getting
/// stuck.
#[test]
fn live_accepts_only_what_runs() {
    let rounds: u32 = env::var("LIVE_ROUNDS")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(3000);
    let mut seeds = Seeds(0x9e37_79b9_7f4a_7c15);
    let (mut programs, mut changes) = (Count::default(), Count::default());
    let mut gaps: Vec<String> = Vec::new();
    let mut gained: Vec<String> = Vec::new();
    let mut violations: Vec<String> = Vec::new();
    let mut ran = 0;
    for _ in 0..rounds {
        let c = seeds.choices(512);
        let p = generate::program(&c);
        let m = seeds.choices(8);
        let q = generate::mutate(&p, &m);
        for (prog, count, changed) in [(&p, &mut programs, false), (&q, &mut changes, true)] {
            let (captures, live) = (verdict(prog, false), verdict(prog, true));
            count.add(captures.is_ok(), live.is_ok());
            if captures.is_ok()
                && !changed
                && gaps.len() < 6
                && let Err(e) = &live
            {
                gaps.push(format!("{e}\n{}", generate::source(prog)));
            }
            if live.is_ok()
                && gained.len() < 6
                && let Err(e) = &captures
            {
                gained.push(format!("captures: {e}\n{}", generate::source(prog)));
            }
            if live.is_ok() && generate::runs(prog) {
                ran += 1;
                if let Outcome::Stuck(why) = run(prog) {
                    violations.push(format!("{why}\n{}", generate::source(prog)));
                }
            }
        }
    }
    println!("programs: {programs:?}\nchanges: {changes:?}\nrun under live: {ran}");
    for g in &gaps {
        println!("--- generated, captures accept, live rejects:\n{g}");
    }
    for g in &gained {
        println!("--- live accepts, captures reject:\n{g}");
    }
    assert!(
        violations.is_empty(),
        "{} accepted programs get stuck:\n{}",
        violations.len(),
        violations.join("\n---\n")
    );
    assert_eq!(programs.both, rounds, "generated programs must typecheck");
}

/// Generated programs where a value only holds a capability past its
/// `scope` (`generate::holding`): the live discipline accepts each and it
/// runs without a use after close; the discipline of captures and
/// flick-core reject it. The same program with `use h` is rejected by all
/// three, and run, reaches a use after close: the freedom is exactly what
/// the machine allows.
#[test]
fn holding_without_use_on_generated_programs() {
    let rounds: u32 = env::var("LIVE_ROUNDS")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(3000);
    let mut seeds = Seeds(0x2545_f491_4f6c_dd1d);
    let mut failures: Vec<String> = Vec::new();
    let (mut core_checked, mut core_g002, mut ran) = (0, 0, 0);
    for round in 0..rounds {
        let c = seeds.choices(256);
        for used in [false, true] {
            let p = generate::holding(&c, used);
            let source = generate::source(&p);
            let (captures, live) = (verdict(&p, false), verdict(&p, true));
            // the machine has no natives for the foreign values of the
            // holes of TypeScript: those are judged, not run
            let runs = generate::runs(&p);
            let outcome = if runs {
                run(&p)
            } else {
                Outcome::Stuck("not run".into())
            };
            ran += usize::from(runs);
            let stuck_after_close =
                matches!(&outcome, Outcome::Stuck(w) if w.starts_with("use after"));
            let ok = if used {
                live.is_err() && captures.is_err() && (!runs || stuck_after_close)
            } else {
                live.is_ok()
                    && matches!(&captures, Err(e) if e.contains("FLICK-G002"))
                    && (!runs || !matches!(outcome, Outcome::Stuck(_)))
            };
            // flick-core is slower: every tenth program
            if round % 10 == 0 {
                core_checked += 1;
                let errors = core(&p);
                core_g002 += usize::from(errors.iter().any(|e| e.starts_with("FLICK-G002")));
                if errors.is_empty() {
                    failures.push(format!("flick-core accepts (used {used}):\n{source}"));
                }
            }
            if !ok && failures.len() < 8 {
                failures.push(format!(
                    "used {used}: captures {captures:?}; live {live:?}; run {outcome:?}\n{source}"
                ));
            }
        }
    }
    println!(
        "holding: {rounds} shapes, held and used; run {ran}; flick-core checked {core_checked}, G002 {core_g002}"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n---\n"));
    // a quarter of the programs at least reach the machine
    assert!(ran * 2 > rounds as usize, "too few programs run: {ran}");
}

/// The cases of `tests/run/model.txt`: which the live discipline rejects.
#[test]
fn live_on_the_corpus() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run");
    let list = fs::read_to_string(root.join("model.txt")).expect("model.txt");
    let (mut total, mut rejected) = (0, Vec::new());
    for case in list
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter(|l| !l.is_empty())
    {
        let Ok(modules) = run::compile(&root.join(format!("{case}.fl"))) else {
            continue;
        };
        let program = run::program(&modules);
        if !run::check(&modules, &program).is_empty() {
            continue;
        }
        total += 1;
        let errors = run::check_with(&modules, &program, true);
        if !errors.is_empty() {
            rejected.push(format!("{case}: {}", errors.join("; ")));
        }
    }
    println!(
        "corpus: {total} checked, {} rejected under live",
        rejected.len()
    );
    for r in &rejected {
        println!("{r}");
    }
    assert!(total > 0, "the corpus must not be empty");
    assert!(
        rejected.is_empty(),
        "live rejects corpus cases: {rejected:?}"
    );
}

fn strip(t: &Type, regions: &BTreeSet<Name>) -> Type {
    let go = |t: &Type| Rc::new(strip(t, regions));
    let brand = |c: &Caps| {
        c.atoms
            .iter()
            .any(|v| matches!(v, Var::Named(n) if regions.contains(n)))
    };
    match t {
        Type::Captured(x, c) if !brand(c) => strip(x, regions),
        Type::Bounded(x, c) if !brand(c) => strip(x, regions),
        Type::Captured(x, c) => Type::Captured(go(x), c.clone()),
        Type::Bounded(x, c) => Type::Bounded(go(x), c.clone()),
        Type::App(a, b) => Type::App(go(a), go(b)),
        Type::Fun(a, s, b) => Type::Fun(go(a), s.clone(), go(b)),
        Type::Eff(r, e, a) => Type::Eff(r.clone(), e.clone(), go(a)),
        Type::Ref(a) => Type::Ref(go(a)),
        Type::Tuple(ts) => Type::Tuple(ts.iter().map(|t| strip(t, regions)).collect()),
        Type::Record(f) => Type::Record(Fields {
            fields: f
                .fields
                .iter()
                .map(|(l, t)| (l.clone(), strip(t, regions)))
                .collect(),
            tail: f.tail.clone(),
        }),
        Type::Forall(s) => Type::Forall(Rc::new(strip_scheme(s, regions))),
        other => other.clone(),
    }
}

fn strip_scheme(s: &Scheme, outer: &BTreeSet<Name>) -> Scheme {
    let mut regions = outer.clone();
    regions.extend(
        s.vars
            .iter()
            .filter(|(_, k)| *k == Kind::Region)
            .map(|(n, _)| n.clone()),
    );
    Scheme {
        vars: s.vars.clone(),
        body: strip(&s.body, &regions),
        outlives: s.outlives.clone(),
    }
}

fn qualified(t: &Type) -> bool {
    match t {
        Type::Captured(..) | Type::Bounded(..) => true,
        Type::App(a, b) | Type::Fun(a, _, b) => qualified(a) || qualified(b),
        Type::Eff(_, _, a) | Type::Ref(a) => qualified(a),
        Type::Tuple(ts) => ts.iter().any(qualified),
        Type::Record(f) => f.fields.values().any(qualified),
        Type::Forall(s) => qualified(&s.body),
        _ => false,
    }
}

/// The values of the built-in modules (std and core): checked under the
/// discipline of captures, under the live one, and under the live one with
/// every qualifier that names no region taken out of every scheme.
#[test]
fn live_on_std() {
    let program = run::program(&[]);
    let decls = run::decls(&[]);
    let modules: HashSet<Name> = program.globals.keys().map(|p| p.module.clone()).collect();
    let errs = |d: &Decls, live: bool| -> BTreeSet<String> {
        check_program_with(&program, d, &modules, live)
            .into_iter()
            .map(|(p, _)| p.to_string())
            .collect()
    };
    let checked = program.globals.len();
    let captures = errs(&decls, false);
    let live = errs(&decls, true);
    let before = decls.values.values().filter(|s| qualified(&s.body)).count();
    let mut stripped = run::decls(&[]);
    let none = BTreeSet::new();
    for s in stripped
        .values
        .values_mut()
        .chain(stripped.instances.values_mut())
        .chain(stripped.constructors.values_mut())
    {
        *s = strip_scheme(s, &none);
    }
    for ops in stripped.services.values_mut() {
        for (_, s) in ops.iter_mut() {
            *s = strip_scheme(s, &none);
        }
    }
    let after = stripped
        .values
        .values()
        .filter(|s| qualified(&s.body))
        .count();
    let live_bare = errs(&stripped, true);
    let captures_bare = errs(&stripped, false);
    println!(
        "std: {checked} values; schemes with a qualifier {before} -> {after} once those without a region are out"
    );
    println!(
        "rejected: captures {}, live {}, live without qualifiers {}, captures without qualifiers {}",
        captures.len(),
        live.len(),
        live_bare.len(),
        captures_bare.len()
    );
    for p in captures.difference(&live) {
        println!("live newly accepts {p}");
    }
    for p in captures.difference(&live_bare) {
        println!("live without qualifiers newly accepts {p}");
    }
    let fails = |d: &Decls, live: bool| check_program_with(&program, d, &modules, live);
    let mut shown = 0;
    for (p, e) in fails(&stripped, true) {
        if !captures.contains(&p.to_string()) && shown < 25 {
            println!("live without qualifiers rejects {p}: {e}");
            shown += 1;
        }
    }
    let mut shown = 0;
    for (p, e) in fails(&decls, true) {
        if !captures.contains(&p.to_string()) && shown < 15 {
            println!("live rejects {p}: {e}");
            shown += 1;
        }
    }
    assert!(live.is_subset(&captures), "live introduces std rejections");
    assert!(
        live_bare.is_subset(&captures),
        "elision introduces std rejections"
    );
}
