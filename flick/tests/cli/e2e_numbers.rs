//! Float semantics: random arithmetic over NaN, infinities, signed zeros
//! and fractions gives, compiled from Flick, exactly the value the same
//! expression has in JS.

use std::fs;
#[cfg(unix)]
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;

use flick_core::build::go::executable;
use flick_core::build::runner_path;
use flick_core::project::config::MAIN_ENTRY;

// written as fractions in Flick, so every expression is a `Float`
const ATOMS: [(&str, &str); 9] = [
    ("0.0", "0"),
    ("(-0.0)", "(-0)"),
    ("1.0", "1"),
    ("2.5", "2.5"),
    ("3.0", "3"),
    ("1e308", "1e308"),
    ("0.1", "0.1"),
    ("(0.0 / 0.0)", "(0 / 0)"),
    ("(1.0 / 0.0)", "(1 / 0)"),
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
            "module App.Main exposing (main)\n\nmain : Unit ->{{Host.Console; AppError}} Unit\nlet main () =\n{}\n",
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
    let expected = run(Command::new("node").arg(&reference));
    // release folds the literal expressions when it compiles: bit for bit the same
    for profile in ["debug", "release"] {
        let out = dir.join(profile);
        run(Command::new(env!("CARGO_BIN_EXE_flick"))
            .arg("build")
            .arg(&source)
            .arg("--out-dir")
            .arg(&out)
            .arg("--profile")
            .arg(profile));
        let compiled = run(Command::new("node").arg(out.join(runner_path(false))));
        for (i, (a, b)) in compiled.lines().zip(expected.lines()).enumerate() {
            assert_eq!(a, b, "expression {i} ({profile}): {}", flick[i]);
        }
        assert_eq!(compiled.lines().count(), 400);
    }
}

const INT_PARSE_SOURCE: &str = r#"module App.Main exposing (main)

render : Option Int -> String
let render parsed =
  match parsed with
  | Some n ->
    let zero_ok = if n == 0 then show (Float.same_value 0.0 (Int.to_float n)) else "n/a"
    "Some " ++ Int.to_string n ++ " zero=" ++ zero_ok
  | None -> "None"

main : Unit ->{Host.Console, Host.Arguments; AppError} Unit
let main () =
  Platform.stdout_line ("lit:feff=" ++ render (Int.parse "\u{FEFF}36\u{00A0}"))
  Platform.stdout_line ("lit:nel=" ++ render (Int.parse "\u{0085}36"))
  Platform.stdout_line ("lit:neg_zero=" ++ render (Int.parse "\u{FEFF}-0\u{00A0}"))
  Platform.stdout_line ("lit:min=" ++ render (Int.parse "-2147483648"))
  Platform.stdout_line ("lit:under=" ++ render (Int.parse "-2147483649"))
  Platform.stdout_line ("lit:max=" ++ render (Int.parse "2147483647"))
  Platform.stdout_line ("lit:over=" ++ render (Int.parse "2147483648"))
  List.each_effect (fun arg -> Platform.stdout_line ("arg:" ++ render (Int.parse arg))) (Platform.args ())
"#;

const INT_PARSE_CASES: &[(&str, &str)] = &[
    ("\u{FEFF}36", "Some 36 zero=n/a"),
    ("36\u{FEFF}", "Some 36 zero=n/a"),
    ("\u{FEFF}36\u{00A0}", "Some 36 zero=n/a"),
    ("\u{00A0}36\u{FEFF}", "Some 36 zero=n/a"),
    (
        " \t\n\u{000B}\u{000C}\r 36 \r\u{000C}\u{000B}\n\t ",
        "Some 36 zero=n/a",
    ),
    (
        "\u{1680}\u{2000}\u{200A}\u{2028}\u{2029}\u{202F}\u{205F}\u{3000}36\u{3000}",
        "Some 36 zero=n/a",
    ),
    ("\u{0085}36", "None"),
    ("36\u{0085}", "None"),
    ("\u{0085}36\u{0085}", "None"),
    ("\u{180E}36", "None"),
    ("\u{200B}36", "None"),
    ("3 6", "None"),
    ("3\t6", "None"),
    ("3\u{00A0}6", "None"),
    ("3\u{FEFF}6", "None"),
    ("+ 36", "None"),
    ("- 36", "None"),
    ("", "None"),
    ("   ", "None"),
    ("+", "None"),
    ("-", "None"),
    ("+36", "Some 36 zero=n/a"),
    ("-36", "Some -36 zero=n/a"),
    ("-0", "Some 0 zero=true"),
    ("+0", "Some 0 zero=true"),
    ("\u{FEFF}-0\u{00A0}", "Some 0 zero=true"),
    ("-2147483648", "Some -2147483648 zero=n/a"),
    ("-2147483649", "None"),
    ("2147483647", "Some 2147483647 zero=n/a"),
    ("+2147483647", "Some 2147483647 zero=n/a"),
    ("2147483648", "None"),
    ("+2147483648", "None"),
];

fn prepare_int_parse_dir() -> PathBuf {
    let tmp = Path::new(env!("CARGO_TARGET_TMPDIR"));
    fs::create_dir_all(tmp).expect("tmpdir");
    let dir = fs::canonicalize(tmp)
        .expect("canonical tmpdir")
        .join("int_parse_whitespace");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("dir");
    #[cfg(unix)]
    if !dir
        .ancestors()
        .any(|p| p.join("node_modules/@flick-lang/runtime").exists())
    {
        let repo_nm = Path::new(env!("CARGO_MANIFEST_DIR")).join("../node_modules");
        symlink(
            fs::canonicalize(&repo_nm).expect("repo node_modules"),
            dir.join("node_modules"),
        )
        .expect("link node_modules");
    }
    dir
}

#[test]
#[ignore = "needs Node, Go and `pnpm install`; run `just e2e`"]
fn int_parse_whitespace_matches_contract_on_js_and_go_in_debug_and_release() {
    let dir = prepare_int_parse_dir();
    let source = dir.join("Main.fl");
    fs::write(&source, INT_PARSE_SOURCE).expect("write");

    let mut expected = vec![
        "lit:feff=Some 36 zero=n/a".to_owned(),
        "lit:nel=None".to_owned(),
        "lit:neg_zero=Some 0 zero=true".to_owned(),
        "lit:min=Some -2147483648 zero=n/a".to_owned(),
        "lit:under=None".to_owned(),
        "lit:max=Some 2147483647 zero=n/a".to_owned(),
        "lit:over=None".to_owned(),
    ];
    for &(_, want) in INT_PARSE_CASES {
        expected.push(format!("arg:{want}"));
    }
    let expected_stdout = format!("{}\n", expected.join("\n"));

    let targets: &[(&str, &str, &[&str])] = &[
        ("js-effect", "js", &["--runtime", "effect"]),
        ("js-rt", "js", &["--runtime", "rt"]),
        ("go", "go", &["--allow-tool", "go"]),
    ];
    for &(label, target, extra_flags) in targets {
        for profile in ["debug", "release"] {
            let out = dir.join(format!("{label}-{profile}"));
            run(Command::new(env!("CARGO_BIN_EXE_flick"))
                .arg("build")
                .arg(&source)
                .arg("--out-dir")
                .arg(&out)
                .arg("--target")
                .arg(target)
                .arg("--profile")
                .arg(profile)
                .args(extra_flags));
            let mut cmd = if target == "go" {
                Command::new(out.join(executable(MAIN_ENTRY)))
            } else {
                let mut node = Command::new("node");
                node.arg(out.join(runner_path(false)));
                node
            };
            for &(arg, _) in INT_PARSE_CASES {
                cmd.arg(arg);
            }
            let actual = run(&mut cmd);
            assert_eq!(actual, expected_stdout, "target={label} profile={profile}");
        }
    }
}
