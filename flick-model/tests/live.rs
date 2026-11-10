//! Regions by use (`src/check/regions/live.rs`): the leaks of the
//! generator, the generated programs and their changes, and the values of
//! the built-in modules, where a region is live where a row has `Live` of
//! it. A change the model accepts must run without a use after close or
//! another stuck state: the machine is the judge. `cargo test -p flick-model --test live -- --nocapture`
//! prints what it counted.

mod live_support;

use std::collections::{BTreeSet, HashSet};
use std::env;
use std::rc::Rc;

use flick_model::check::check_program;
use flick_model::decls::Decls;
use flick_model::eval::Outcome;
use flick_model::generate;
use flick_model::run;
use flick_model::syntax::Name;
use flick_model::types::{Caps, Fields, Kind, Scheme, Type, Var};

use live_support::{run, verdict};

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
        let live = verdict(&p);
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

/// Generated programs and changes of them: every generated program
/// checks, and every one the model accepts runs without getting stuck.
#[test]
fn live_accepts_only_what_runs() {
    let rounds: u32 = env::var("LIVE_ROUNDS")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(3000);
    let mut seeds = Seeds(0x9e37_79b9_7f4a_7c15);
    let (mut programs, mut changes) = (0, 0);
    let mut rejected: Vec<String> = Vec::new();
    let mut violations: Vec<String> = Vec::new();
    let mut ran = 0;
    for _ in 0..rounds {
        let c = seeds.choices(512);
        let p = generate::program(&c);
        let m = seeds.choices(8);
        let q = generate::mutate(&p, &m);
        for (prog, changed) in [(&p, false), (&q, true)] {
            let live = verdict(prog);
            match (&live, changed) {
                (Ok(()), false) => programs += 1,
                (Ok(()), true) => changes += 1,
                (Err(e), false) if rejected.len() < 6 => {
                    rejected.push(format!("{e}\n{}", generate::source(prog)));
                }
                (Err(_), _) => {}
            }
            if live.is_ok() && generate::runs(prog) {
                ran += 1;
                if let Outcome::Stuck(why) = run(prog) {
                    violations.push(format!("{why}\n{}", generate::source(prog)));
                }
            }
        }
    }
    println!("accepted: {programs} programs, {changes} changes of {rounds}; run {ran}");
    assert!(
        violations.is_empty(),
        "{} accepted programs get stuck:\n{}",
        violations.len(),
        violations.join("\n---\n")
    );
    assert_eq!(
        programs,
        rounds,
        "generated programs must typecheck:\n{}",
        rejected.join("\n---\n")
    );
}

/// Generated programs where a value only holds a capability past its
/// `scope` (`generate::holding`): the model accepts each and it runs
/// without a use after close. The same program with `use h` is rejected,
/// and run, reaches a use after close: the freedom is exactly what the
/// machine allows. `live_core.rs` checks flick-core on the same programs.
#[test]
fn holding_without_use_on_generated_programs() {
    let rounds: u32 = env::var("LIVE_ROUNDS")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(3000);
    let mut seeds = Seeds(0x2545_f491_4f6c_dd1d);
    let mut failures: Vec<String> = Vec::new();
    let mut ran = 0;
    for _ in 0..rounds {
        let c = seeds.choices(256);
        for used in [false, true] {
            let p = generate::holding(&c, used);
            let source = generate::source(&p);
            let live = verdict(&p);
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
                live.is_err() && (!runs || stuck_after_close)
            } else {
                live.is_ok() && (!runs || !matches!(outcome, Outcome::Stuck(_)))
            };
            if !ok && failures.len() < 8 {
                failures.push(format!(
                    "used {used}: model {live:?}; run {outcome:?}\n{source}"
                ));
            }
        }
    }
    println!("holding: {rounds} shapes, held and used; run {ran}");
    assert!(failures.is_empty(), "{}", failures.join("\n---\n"));
    // a quarter of the programs at least reach the machine
    assert!(ran * 2 > rounds as usize, "too few programs run: {ran}");
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

/// The values of the built-in modules (std and core) the model rejects:
/// natives it has no translation or type for, and the values below, which
/// it cannot type yet. Taking out every qualifier that names no region
/// rejects nothing more: elision makes no use of them.
#[test]
fn live_on_std() {
    let program = run::program(&[]);
    let decls = run::decls(&[]);
    let modules: HashSet<Name> = program.globals.keys().map(|p| p.module.clone()).collect();
    let errs = |d: &Decls| -> BTreeSet<String> {
        check_program(&program, d, &modules)
            .into_iter()
            .map(|(p, _)| p.to_string())
            .collect()
    };
    let rejected = errs(&decls);
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
    let bare = errs(&stripped);
    let native =
        |e: &str| e.starts_with("(Translation)") || e.ends_with("has no type in the model");
    let typed: BTreeSet<String> = check_program(&program, &decls, &modules)
        .into_iter()
        .filter(|(_, e)| !native(&e.to_string()))
        .map(|(p, e)| {
            println!("rejects {p}: {e}");
            p.to_string()
        })
        .collect();
    println!(
        "std: {} values; rejected {}, {} of them typed; without qualifiers {}",
        program.globals.len(),
        rejected.len(),
        typed.len(),
        bare.len()
    );
    let known: BTreeSet<String> = UNTYPED.iter().map(|p| (*p).to_owned()).collect();
    assert_eq!(typed, known, "the std values the model cannot type changed");
    assert!(
        bare.is_subset(&rejected),
        "elision introduces std rejections"
    );
}

/// The values of std and core the model cannot type (`live_on_std`).
const UNTYPED: &[&str] = &[
    "Atom.local",
    "Atom.serializable_result",
    "Http.Client.current",
    "Http.Client.execute",
    "Http.Client.layer",
    "Http.Server.layer_test_client",
    "Http.Server.test_client",
    "Http.StaticServer.route",
    "HttpApi.Derive.problem_case",
    "HttpApi.http_error",
    "Jwt.expiry_json",
    "Jwt.header_json",
    "Jwt.sign",
    "Record.instance Show (ReadonlyRecord a)",
    "Resource.acquire_release",
    "Scope.add_finalizer",
    "Ui.element",
    "Ui.on_mount",
    "Ui.watch",
];
