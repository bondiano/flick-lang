use flick_backend::command::{Descriptor, Parameter, Style, ValueKind};
use flick_core::diagnostics::codes;

use super::{Contributed, Source, arguments, resolve};

fn descriptor(name: &str) -> Descriptor {
    Descriptor {
        name: name.into(),
        summary: String::new(),
        positionals: vec![Parameter {
            name: "who".into(),
            kind: ValueKind::Text,
            multiple: false,
            required: false,
            help: String::new(),
        }],
        options: vec![Parameter {
            name: "loud".into(),
            kind: ValueKind::Flag,
            multiple: false,
            required: false,
            help: String::new(),
        }],
        inputs: vec![],
        reads: vec![],
        writes: vec![],
        style: Style::Plan,
    }
}

fn found(entries: &[(&str, &[&str])]) -> Vec<(Source, Contributed)> {
    (entries.iter())
        .map(|(short, names)| {
            let source = Source {
                official: false,
                short: (*short).into(),
                id: format!("test/{short}"),
                digest: String::new(),
                plugin: None,
            };
            let contributed = Contributed {
                short: (*short).into(),
                commands: names.iter().map(|n| descriptor(n)).collect(),
            };
            (source, contributed)
        })
        .collect()
}

fn words(text: &str) -> Vec<String> {
    text.split_whitespace().map(str::to_owned).collect()
}

#[test]
fn a_command_runs_under_its_flat_and_its_qualified_name() {
    let found = found(&[("literal", &["hello", "ffi import"]), ("pairs", &["greet"])]);
    let (source, d, rest) = resolve(&words("hello world"), &found, &Vec::new).expect("flat");
    assert_eq!(
        (source.short.as_str(), d.name.as_str(), rest),
        ("literal", "hello", words("world"))
    );
    let (source, d, rest) =
        resolve(&words("literal:ffi import x"), &found, &Vec::new).expect("qualified");
    assert_eq!(
        (source.short.as_str(), d.name.as_str(), rest),
        ("literal", "ffi import", words("x"))
    );
    let (_, d, _) = resolve(&words("ffi import"), &found, &Vec::new).expect("words of a name");
    assert_eq!(d.name, "ffi import");
}

#[test]
fn a_collision_names_both_and_an_unknown_name_fails() {
    let found = found(&[("literal", &["greet"]), ("pairs", &["greet"])]);
    let collision = resolve(&words("greet x"), &found, &Vec::new).expect_err("a collision");
    assert_eq!(collision.code, codes::P053);
    let fix = format!("{:?}", collision.fixes);
    assert!(
        fix.contains("literal:greet") && fix.contains("pairs:greet"),
        "{fix}"
    );
    let (source, _, _) =
        resolve(&words("pairs:greet x"), &found, &Vec::new).expect("qualified wins");
    assert_eq!(source.short, "pairs");
    assert_eq!(
        resolve(&words("nothing"), &found, &Vec::new)
            .expect_err("unknown")
            .code,
        codes::P052
    );
    assert_eq!(
        resolve(&words("other:greet"), &found, &Vec::new)
            .expect_err("no such contributor")
            .code,
        codes::P052
    );
}

#[test]
fn arguments_follow_the_descriptor() {
    let parsed =
        arguments(&descriptor("hello"), "literal", &words("world --loud")).expect("parses");
    let named: Vec<(&str, &[String])> = parsed
        .iter()
        .map(|a| (a.name.as_str(), a.values.as_slice()))
        .collect();
    assert_eq!(
        named,
        [("who", &words("world")[..]), ("loud", &words("true")[..])]
    );
    assert!(arguments(&descriptor("hello"), "literal", &words("--quiet")).is_err());
}
