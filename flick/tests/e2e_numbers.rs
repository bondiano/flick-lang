//! Number semantics: random arithmetic over NaN, infinities, signed zeros
//! and fractions gives, compiled from Flick, exactly the value the same
//! expression has in JS.

use std::fs;
use std::path::Path;
use std::process::Command;

const ATOMS: [(&str, &str); 9] = [
    ("0", "0"),
    ("(-0)", "(-0)"),
    ("1", "1"),
    ("2.5", "2.5"),
    ("3", "3"),
    ("1e308", "1e308"),
    ("0.1", "0.1"),
    ("(0 / 0)", "(0 / 0)"),
    ("(1 / 0)", "(1 / 0)"),
];
const OPS: [&str; 6] = ["+", "-", "*", "/", "%", "**"];

struct Lcg(u64);

impl Lcg {
    fn next(&mut self, n: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        usize::try_from(self.0 >> 33).expect("fits") % n
    }
}

/// The same expression for Flick and for JS, fully parenthesized.
fn expression(rng: &mut Lcg, depth: u32) -> (String, String) {
    if depth == 0 || rng.next(3) == 0 {
        let (f, j) = ATOMS[rng.next(ATOMS.len())];
        return (f.to_owned(), j.to_owned());
    }
    if rng.next(6) == 0 {
        let (f, j) = expression(rng, depth - 1);
        return (format!("(-{f})"), format!("(-{j})"));
    }
    let op = OPS[rng.next(OPS.len())];
    let (lf, lj) = expression(rng, depth - 1);
    let (rf, rj) = expression(rng, depth - 1);
    (format!("({lf} {op} {rf})"), format!("({lj} {op} {rj})"))
}

fn run(command: &mut Command) -> String {
    let output = command.output().expect("runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
#[ignore = "needs Node and `pnpm install`; run `just e2e`"]
fn numbers_match_js() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("numbers");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("dir");
    let mut rng = Lcg(0x5eed);
    let (mut flick, mut js) = (Vec::new(), Vec::new());
    for _ in 0..400 {
        let (f, j) = expression(&mut rng, 4);
        flick.push(format!("  Platform.stdout_line (show {f})"));
        js.push(format!("show({j});"));
    }
    let source = dir.join("Main.fl");
    fs::write(
        &source,
        format!(
            "module App.Main exposing (main)\n\nmain : Unit ->{{none; AppError}} Unit\nlet main () =\n{}\n",
            flick.join("\n")
        ),
    )
    .expect("write");
    let reference = dir.join("reference.mjs");
    fs::write(
        &reference,
        format!(
            "const show = (n) => console.log(Object.is(n, -0) ? \"-0\" : String(n));\n{}\n",
            js.join("\n")
        ),
    )
    .expect("write");
    let out = dir.join("out");
    run(Command::new(env!("CARGO_BIN_EXE_flick"))
        .arg("build")
        .arg(&source)
        .arg("--out-dir")
        .arg(&out));
    let compiled = run(Command::new("node").arg(out.join("main.js")));
    let expected = run(Command::new("node").arg(&reference));
    for (i, (a, b)) in compiled.lines().zip(expected.lines()).enumerate() {
        assert_eq!(a, b, "expression {i}: {}", flick[i]);
    }
    assert_eq!(compiled.lines().count(), 400);
}
