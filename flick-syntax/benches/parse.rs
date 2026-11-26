//! Lexing, parsing, and literal-decoding benchmarks for `flick-syntax`.
//!
//! Run with:
//! `cargo bench -p flick-syntax --bench parse`

use std::fmt::Write as _;
use std::fs;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::Instant;

use flick_diagnostics::source::FileId;
use flick_syntax::syntax::kind::SyntaxKind;
use flick_syntax::syntax::lexer::lex;
use flick_syntax::syntax::literal::{bigint_value, integer_value, number_value, string_value};
use flick_syntax::syntax::parse;

const WARMUP_ITERS: usize = 3;
const MEASURE_ITERS: usize = 15;

struct CorpusFile {
    text: String,
    file_id: FileId,
}

struct ExtractedLiterals {
    strings: Vec<String>,
    numbers: Vec<String>,
    bigints: Vec<String>,
    total_bytes: usize,
}

fn collect_fl_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_fl_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "fl") {
            out.push(path);
        }
    }
}

fn load_corpus(root: &Path) -> Vec<CorpusFile> {
    let mut paths = Vec::new();
    for rel in ["packages", "tests/syntax", "tests/check/pass", "tests/run"] {
        collect_fl_files(&root.join(rel), &mut paths);
    }
    let mut files = Vec::with_capacity(paths.len());
    for (idx, path) in paths.into_iter().enumerate() {
        if let Ok(text) = fs::read_to_string(&path) {
            let id = u32::try_from(idx).expect("corpus file count fits in u32");
            files.push(CorpusFile {
                text,
                file_id: FileId::new(id),
            });
        }
    }
    files
}

fn build_literal_heavy_source(repeats: usize) -> String {
    let mut out = String::with_capacity(repeats * 700);
    out.push_str("module Bench.Literals\n\n");
    for i in 0..repeats {
        let _ = writeln!(
            out,
            "// Line comment describing record entry {i} with ASCII prose and punctuation (a, b, c)\n\
             /// Doc comment for item_{i}: parses numbers, strings, raw strings, and multiline text.\n\
             /* outer /* nested block comment {i} */ trailing text */\n\
             let item_{i} = {{\n\
               dec = 1_000_000\n\
               small = {i}\n\
               typed_i32 = 42i32\n\
               typed_u64 = 18_446_744_073_709u64\n\
               hex = 0xDEAD_BEEFu32\n\
               oct = 0o755u16\n\
               bin = 0b1010_0101u8\n\
               flt = 3.141_592_653_589_793\n\
               exp = 1.25e-12f32\n\
               big = 999_999_999_999_999_999_999_999n\n\
               plain = \"hello world from flick literal benchmark {i} without escapes\"\n\
               escaped = \"line1\\nline2\\t\\\"quoted\\\"\\\\slash\\0\\u{{1F600}}\"\n\
               interp = \"value is \\{{small}} and \\{{dec}}\"\n\
               raw0 = r\"C:\\Users\\flick\\path\\to\\file_{i}.fl\"\n\
               raw2 = r##\"contains \"quotes\" and \"# inside {i}\"##\n\
               multi = \"\"\"\n\
                 first indented line {i}\n\
                   second nested line with \\t escape\n\
                 third line\n\
                 \"\"\"\n\
             }}\n",
        );
    }
    out
}

fn extract_literals<'a>(sources: impl IntoIterator<Item = (&'a str, FileId)>) -> ExtractedLiterals {
    let mut strings = Vec::new();
    let mut numbers = Vec::new();
    let mut bigints = Vec::new();
    for (text, file_id) in sources {
        let (tokens, _) = lex(text, file_id);
        for tok in tokens {
            let slice = &text[tok.start as usize..tok.end as usize];
            match tok.kind {
                SyntaxKind::String => strings.push(slice.to_owned()),
                SyntaxKind::Number => numbers.push(slice.to_owned()),
                SyntaxKind::BigInt => bigints.push(slice.to_owned()),
                _ => {}
            }
        }
    }
    let total_bytes = strings
        .iter()
        .chain(&numbers)
        .chain(&bigints)
        .map(String::len)
        .sum();
    ExtractedLiterals {
        strings,
        numbers,
        bigints,
        total_bytes,
    }
}

fn decode_all(lits: &ExtractedLiterals) -> u64 {
    let mut total = 0_u64;
    for s in &lits.strings {
        total = total.wrapping_add(string_value(black_box(s)).len() as u64);
    }
    for n in &lits.numbers {
        if let Some(v) = integer_value(black_box(n)) {
            #[expect(clippy::cast_possible_truncation, reason = "checksum folding")]
            let low = v.unsigned_abs() as u64;
            total = total.wrapping_add(low);
        } else if let Some(v) = number_value(black_box(n)) {
            total = total.wrapping_add(v.to_bits());
        }
    }
    for b in &lits.bigints {
        if let Some(v) = bigint_value(black_box(b)) {
            #[expect(clippy::cast_possible_truncation, reason = "checksum folding")]
            let low = v.unsigned_abs() as u64;
            total = total.wrapping_add(low);
        }
    }
    total
}

fn bench<F: FnMut() -> u64>(name: &str, bytes_per_iter: usize, mut f: F) {
    for _ in 0..WARMUP_ITERS {
        let _ = black_box(f());
    }
    let mut samples_ns = Vec::with_capacity(MEASURE_ITERS);
    let mut checksum = 0_u64;
    for _ in 0..MEASURE_ITERS {
        let started = Instant::now();
        checksum = checksum.wrapping_add(black_box(f()));
        samples_ns.push(started.elapsed().as_nanos());
    }
    samples_ns.sort_unstable();
    let best_nanos = samples_ns[0];
    let mid_nanos = samples_ns[samples_ns.len() / 2];
    #[expect(clippy::cast_precision_loss, reason = "benchmark reporting precision")]
    let median_ms = (mid_nanos as f64) / 1_000_000.0;
    #[expect(clippy::cast_precision_loss, reason = "benchmark reporting precision")]
    let min_ms = (best_nanos as f64) / 1_000_000.0;
    #[expect(clippy::cast_precision_loss, reason = "benchmark reporting precision")]
    let mib_s =
        ((bytes_per_iter as f64) / (1024.0 * 1024.0)) / ((mid_nanos as f64) / 1_000_000_000.0);
    println!(
        "{name:<28} median {median_ms:8.3} ms | min {min_ms:8.3} ms | {mib_s:7.2} MiB/s | checksum {checksum}"
    );
}

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .expect("flick-syntax sits inside workspace root");
    let corpus = load_corpus(workspace_root);
    let corpus_bytes: usize = corpus.iter().map(|f| f.text.len()).sum();
    #[expect(clippy::cast_precision_loss, reason = "reporting")]
    let corpus_mib = corpus_bytes as f64 / (1024.0 * 1024.0);
    println!(
        "Corpus: {} files, {corpus_bytes} bytes ({corpus_mib:.2} MiB)",
        corpus.len(),
    );

    bench("corpus_lex", corpus_bytes, || {
        let mut total = 0_u64;
        for f in &corpus {
            let (tokens, diags) = lex(black_box(&f.text), f.file_id);
            total = total
                .wrapping_add(tokens.len() as u64)
                .wrapping_add(diags.len() as u64);
        }
        total
    });

    bench("corpus_parse", corpus_bytes, || {
        let mut total = 0_u64;
        for f in &corpus {
            let parsed = parse(black_box(&f.text), f.file_id);
            total = total
                .wrapping_add(parsed.tree.children.len() as u64)
                .wrapping_add(parsed.diagnostics.len() as u64);
        }
        total
    });

    let corpus_lits = extract_literals(corpus.iter().map(|f| (f.text.as_str(), f.file_id)));
    println!(
        "Corpus literals: {} strings, {} numbers, {} bigints ({} bytes)",
        corpus_lits.strings.len(),
        corpus_lits.numbers.len(),
        corpus_lits.bigints.len(),
        corpus_lits.total_bytes
    );
    bench("corpus_literals_decode", corpus_lits.total_bytes, || {
        decode_all(&corpus_lits)
    });

    let synth = build_literal_heavy_source(600);
    let synth_bytes = synth.len();
    #[expect(clippy::cast_precision_loss, reason = "reporting")]
    let synth_kib = synth_bytes as f64 / 1024.0;
    println!("Synthetic literal-heavy: {synth_bytes} bytes ({synth_kib:.2} KiB)");

    bench("synth_literals_lex", synth_bytes, || {
        let (tokens, diags) = lex(black_box(&synth), FileId::new(0));
        (tokens.len() as u64).wrapping_add(diags.len() as u64)
    });

    bench("synth_literals_parse", synth_bytes, || {
        let parsed = parse(black_box(&synth), FileId::new(0));
        (parsed.tree.children.len() as u64).wrapping_add(parsed.diagnostics.len() as u64)
    });

    let synth_lits = extract_literals([(synth.as_str(), FileId::new(0))]);
    bench("synth_literals_decode", synth_lits.total_bytes, || {
        decode_all(&synth_lits)
    });
}
