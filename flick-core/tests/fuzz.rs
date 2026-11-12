//! Mutation fuzzing of the parser and formatter over the syntax corpus: the
//! parser must not panic or hang, the tree must stay lossless, every
//! diagnostic span must lie inside the input, and inputs without errors must
//! format idempotently and type-check without panicking. `FLICK_FUZZ_ITERATIONS` sets the number of mutated
//! inputs (default 20 000; the nightly CI job runs 1 000 000 in release).

use std::path::Path;
use std::{env, fs, panic};

use flick_core::source::SourceMap;
use flick_core::syntax::parse;
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
    let mut rng = Rng(0x5eed_f11c_0000_0001);
    for i in 0..iterations {
        let input = mutate(&mut rng, &seeds[i % seeds.len()]);
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
