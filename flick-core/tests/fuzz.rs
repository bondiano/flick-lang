//! Mutation fuzzing of the parser and formatter over the syntax corpus: the
//! parser must not panic or hang, the tree must stay lossless, every
//! diagnostic span must lie inside the input, and inputs without errors must
//! format idempotently and type-check without panicking. `FLICK_FUZZ_ITERATIONS` sets the number of mutated
//! inputs (default 20 000; the nightly CI job runs 1 000 000 in release).
//! The checker runs on the stack the compiler gives it. Each input is written
//! to `target/tmp/fuzz-input.fl` before it is parsed: after an abort, such as a
//! stack overflow, which names no iteration, the file holds the input.

use std::io::{Seek, SeekFrom, Write};
use std::num::NonZero;
use std::path::Path;
use std::{env, fs, panic, thread};

use flick_core::source::SourceMap;
use flick_core::syntax::parse;
use flick_core::types;
use flick_test_support::files as fixture_files;

/// Fragments spliced into inputs: tokens and layout that stress recovery.
const FRAGMENTS: [&str; 46] = [
    " a_rather_long_argument_name_for_wrapping another_quite_long_argument_name ",
    " |> List.map some_function_with_a_long_name |> List.filter another_long_predicate ",
    " -> SomeLongTypeName ->{ServiceOne, ServiceTwo; ErrorOne} ",
    "(",
    ")",
    "[",
    "]",
    "{",
    "}",
    ",",
    ";",
    ":",
    ".",
    "..",
    "=",
    "->",
    "->{",
    "=>",
    "|",
    "|>",
    "<|",
    ">>",
    "<<",
    "\n",
    "\n  ",
    "\n    ",
    "\n\t",
    " ",
    "let ",
    "match x with\n| ",
    "fun x -> ",
    "if ",
    " then ",
    " else ",
    "task { ",
    "sql p { from ",
    "\"",
    "\"\"\"\n",
    "r#\"",
    "/*",
    "*/",
    "//",
    "0x",
    "1_",
    "é",
    "<div>",
];

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % (n.max(1) as u64)).expect("fits")
    }
}

fn char_boundary(text: &str, mut at: usize) -> usize {
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

fn mutate(rng: &mut Rng, text: &str) -> String {
    let mut out = text.to_owned();
    for _ in 0..=rng.below(4) {
        let at = char_boundary(&out, rng.below(out.len() + 1));
        match rng.below(4) {
            0 => out.insert_str(at, FRAGMENTS[rng.below(FRAGMENTS.len())]),
            1 => {
                let end = char_boundary(&out, (at + rng.below(16)).min(out.len()));
                out.replace_range(at..end, "");
            }
            2 => {
                let end = char_boundary(&out, (at + rng.below(64)).min(out.len()));
                let copy = out[at..end].to_owned();
                let to = char_boundary(&out, rng.below(out.len() + 1));
                out.insert_str(to, &copy);
            }
            _ => {
                let end = char_boundary(&out, (at + rng.below(8)).min(out.len()));
                let fragment = FRAGMENTS[rng.below(FRAGMENTS.len())];
                out.replace_range(at..end, fragment);
            }
        }
    }
    out
}

/// The stack the compiler checks a module on (`project/load/checking/graph.rs`):
/// the checker recurses as deep as an input nests, and as a class's instances
/// chain up to the solver's budget.
const CHECK_STACK: usize = 8 << 20;

#[test]
fn front_end_survives_mutated_corpus() {
    let iterations: usize = env::var("FLICK_FUZZ_ITERATIONS")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(20_000);
    let seeds: Vec<String> = fixture_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests"),
        "fl",
    )
    .iter()
    .map(|path| fs::read_to_string(path).expect("UTF-8 fixture"))
    .collect();
    assert!(!seeds.is_empty());
    // Initialize shared built-ins once before fanning out across worker threads.
    let _ = types::builtins();
    let workers = thread::available_parallelism()
        .map_or(1, NonZero::get)
        .min(iterations.max(1));
    thread::scope(|scope| {
        let seeds = &seeds;
        let handles: Vec<_> = (0..workers)
            .map(|worker| {
                thread::Builder::new()
                    .stack_size(CHECK_STACK)
                    .spawn_scoped(scope, move || {
                        survive_mutated_corpus(seeds, iterations, worker, workers);
                    })
                    .expect("a thread")
            })
            .collect();
        for handle in handles {
            handle.join().expect("the corpus survives");
        }
    });
}

fn survive_mutated_corpus(seeds: &[String], iterations: usize, worker: usize, workers: usize) {
    let base_seed = 0x5eed_f11c_0000_0001_u64;
    let seed = base_seed.wrapping_add((worker as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15));
    let mut rng = Rng(seed);
    let file_name = if workers == 1 {
        "fuzz-input.fl".to_owned()
    } else {
        format!("fuzz-input-{worker}.fl")
    };
    // one handle rewritten in place: reopening the file per input costs more
    let mut current = fs::File::create(Path::new(env!("CARGO_TARGET_TMPDIR")).join(file_name))
        .expect("a file for the current input");
    for i in (worker..iterations).step_by(workers) {
        let input = mutate(&mut rng, &seeds[i % seeds.len()]);
        current.set_len(0).expect("the current input is written");
        current
            .seek(SeekFrom::Start(0))
            .expect("the current input is written");
        current
            .write_all(input.as_bytes())
            .expect("the current input is written");
        let mut sources = SourceMap::default();
        let file = sources.add("fuzz.fl", input.as_str());
        let parsed = panic::catch_unwind(|| parse(&input, file))
            .unwrap_or_else(|_| panic!("parser panicked on iteration {i}:\n{input}"));
        assert_eq!(
            parsed.tree.text(&input),
            input,
            "not lossless on iteration {i}"
        );
        for d in &parsed.diagnostics {
            let span = d.span.expect("syntax diagnostics have spans");
            assert!(
                span.end as usize <= input.len(),
                "span out of bounds on {i}:\n{input}"
            );
        }
        // the checker runs on what parses cleanly
        if parsed.diagnostics.is_empty() {
            panic::catch_unwind(|| flick_core::check(&sources, file))
                .unwrap_or_else(|_| panic!("checker panicked on iteration {i}:\n{input}"));
        }
    }
}
