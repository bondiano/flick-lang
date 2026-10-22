//! Differential test of compile-time evaluation against Node: random literal
//! expressions are folded in Rust and evaluated as JS, and every folded value
//! must equal the JS one exactly (Number compared by bits, NaN as NaN).
//! Needs `node` on PATH; run by `just e2e`.

use std::fmt::Write as _;
use std::io::Write as _;
use std::process::{Command, Stdio};

use flick_core::consteval::{Const, eval};
use flick_core::source::SourceMap;
use flick_core::syntax::kind::SyntaxKind;
use flick_core::syntax::parse;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[usize::try_from(self.next() % items.len() as u64).expect("fits")]
    }
}

#[derive(Clone, Copy)]
enum Ty {
    Number,
    BigInt,
    String,
    Bool,
}

/// A Flick expression and the JS expression with the same meaning.
fn gen_expr(rng: &mut Rng, ty: Ty, depth: u32) -> (String, String) {
    let leaf = depth == 0 || rng.next().is_multiple_of(3);
    let bin = |rng: &mut Rng, op: &str, js: &str, a: Ty, b: Ty| {
        let (fa, ja) = gen_expr(rng, a, depth - 1);
        let (fb, jb) = gen_expr(rng, b, depth - 1);
        (format!("({fa} {op} {fb})"), format!("(({ja}) {js} ({jb}))"))
    };
    match ty {
        Ty::Number if leaf => {
            let n = rng.pick(&[
                "0",
                "1",
                "2",
                "3",
                "0.1",
                "0.2",
                "1.5",
                "1e308",
                "5e-324",
                "1e-7",
                "0x1F",
                "0b101",
                "1_000",
                "123456789012345680000",
                "9007199254740993",
                "0.5",
            ]);
            (n.to_owned(), n.replace('_', ""))
        }
        Ty::Number => match rng.next() % 4 {
            0 => {
                let (f, j) = gen_expr(rng, Ty::Number, depth - 1);
                (format!("(-{f})"), format!("(-({j}))"))
            }
            1 => {
                let (fc, jc) = gen_expr(rng, Ty::Bool, depth - 1);
                let (fa, ja) = gen_expr(rng, Ty::Number, depth - 1);
                let (fb, jb) = gen_expr(rng, Ty::Number, depth - 1);
                (
                    format!("(if {fc} then {fa} else {fb})"),
                    format!("(({jc}) ? ({ja}) : ({jb}))"),
                )
            }
            _ => {
                let op = rng.pick(&["+", "-", "*", "/", "%", "**"]);
                bin(rng, op, op, Ty::Number, Ty::Number)
            }
        },
        Ty::BigInt if leaf => {
            let n = rng.pick(&["0n", "1n", "2n", "7n", "100n", "0xFFn", "123456789n"]);
            (n.to_owned(), n.to_owned())
        }
        Ty::BigInt => {
            let op = rng.pick(&["+", "-", "*", "/", "%", "**"]);
            bin(rng, op, op, Ty::BigInt, Ty::BigInt)
        }
        Ty::String if leaf => {
            let s = rng.pick(&[
                r#""""#,
                r#""a""#,
                r#""b\n""#,
                r#""\u{1F600}""#,
                r#""\u{FF61}""#,
                r#""é""#,
                r#"r"\x""#,
            ]);
            let value = flick_core::syntax::literal::string_value(s);
            (s.to_owned(), serde_json::to_string(&value).expect("json"))
        }
        Ty::String => bin(rng, "++", "+", Ty::String, Ty::String),
        Ty::Bool if leaf => {
            let b = rng.pick(&["true", "false"]);
            (b.to_owned(), b.to_owned())
        }
        Ty::Bool => gen_bool(rng, depth),
    }
}

fn gen_bool(rng: &mut Rng, depth: u32) -> (String, String) {
    let bin = |rng: &mut Rng, op: &str, js: &str, a: Ty, b: Ty| {
        let (fa, ja) = gen_expr(rng, a, depth - 1);
        let (fb, jb) = gen_expr(rng, b, depth - 1);
        (format!("({fa} {op} {fb})"), format!("(({ja}) {js} ({jb}))"))
    };
    match rng.next() % 5 {
        0 => {
            let (f, j) = gen_expr(rng, Ty::Bool, depth - 1);
            (format!("(not {f})"), format!("(!({j}))"))
        }
        1 => {
            let op = rng.pick(&["&&", "||", "==", "!="]);
            let js = match op {
                "==" => "===",
                "!=" => "!==",
                other => other,
            };
            bin(rng, op, js, Ty::Bool, Ty::Bool)
        }
        n => {
            let operand =
                [Ty::Number, Ty::BigInt, Ty::String][usize::try_from(n - 2).expect("fits")];
            let op = rng.pick(&["==", "!=", "<", "<=", ">", ">="]);
            let (fa, ja) = gen_expr(rng, operand, depth - 1);
            let (fb, jb) = gen_expr(rng, operand, depth - 1);
            let js = match (op, operand) {
                ("==", Ty::Number) => format!("svz({ja}, {jb})"),
                ("!=", Ty::Number) => format!("!svz({ja}, {jb})"),
                ("==", _) => format!("(({ja}) === ({jb}))"),
                ("!=", _) => format!("(({ja}) !== ({jb}))"),
                _ => format!("(({ja}) {op} ({jb}))"),
            };
            (format!("({fa} {op} {fb})"), js)
        }
    }
}

fn show(value: &Const) -> String {
    match value {
        Const::Number(n) if n.is_nan() => "NaN".into(),
        Const::Number(n) => format!("{:016x}", n.to_bits()),
        Const::BigInt(n) => format!("{n}n"),
        Const::String(s) => serde_json::to_string(s).expect("json"),
        Const::Bool(b) => b.to_string(),
    }
}

#[test]
#[ignore = "needs node; run by `just e2e`"]
fn folding_matches_node() {
    let mut rng = Rng(0x00c0_ffee_1234_5678);
    let mut cases = Vec::new();
    for i in 0..4000 {
        let ty = [Ty::Number, Ty::BigInt, Ty::String, Ty::Bool][i % 4];
        let (flick, js) = gen_expr(&mut rng, ty, 4);
        let src = format!("let x = {flick}\n");
        let mut sources = SourceMap::default();
        let file = sources.add("t.fl", src.as_str());
        let parsed = parse(&src, file);
        assert!(
            parsed.diagnostics.is_empty(),
            "{src}: {:?}",
            parsed.diagnostics
        );
        let decl = parsed.tree.nodes().next().expect("let");
        let body = decl
            .nodes()
            .find(|n| n.kind == SyntaxKind::Block)
            .expect("body");
        if let Some(value) = eval(body, &src) {
            cases.push((flick, js, show(&value)));
        }
    }
    assert!(cases.len() > 2000, "only {} of 4000 folded", cases.len());

    let mut script = String::from(
        "const svz = (a, b) => a === b || (a !== a && b !== b);\n\
         const bits = (n) => { const b = new DataView(new ArrayBuffer(8)); b.setFloat64(0, n); \
         return b.getBigUint64(0).toString(16).padStart(16, '0'); };\n\
         const show = (v) => typeof v === 'number' ? (Number.isNaN(v) ? 'NaN' : bits(v)) \
         : typeof v === 'bigint' ? v + 'n' : typeof v === 'string' ? JSON.stringify(v) : String(v);\n",
    );
    for (_, js, _) in &cases {
        let _ = writeln!(script, "console.log(show({js}));");
    }
    let mut node = Command::new("node")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("`node` is on PATH");
    node.stdin
        .take()
        .expect("stdin")
        .write_all(script.as_bytes())
        .expect("write script");
    let out = node.wait_with_output().expect("node exits");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).expect("utf-8");
    let mismatches: Vec<String> = cases
        .iter()
        .zip(stdout.lines())
        .filter(|((_, _, ours), theirs)| ours != theirs)
        .map(|((flick, _, ours), theirs)| format!("{flick}\n  flick: {ours}\n  node:  {theirs}"))
        .collect();
    assert_eq!(stdout.lines().count(), cases.len());
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}
