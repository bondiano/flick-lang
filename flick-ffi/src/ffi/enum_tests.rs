use std::collections::BTreeMap;

use super::import::{Generated, Status, generate};
use super::{Policy, Surface};

/// An export `name` of one parameter of the type `ty`, as JSON.
fn taking(name: &str, ty: &str) -> String {
    format!(
        r#"{{"name": "{name}", "local": "{name}", "kind": "function", "fingerprint": "sha256:{name}",
  "signatures": [{{"params": [{{"name": "x", "optional": false, "rest": false, "type": {ty}}}],
  "this": false, "typeParams": [], "result": {{"k": "void"}}}}]}}"#
    )
}

/// A union of the literals `texts`, named by `alias` when it has one.
fn union(alias: Option<&str>, texts: &[&str]) -> String {
    let types: Vec<String> = (texts.iter())
        .map(|t| format!(r#"{{"k": "literal", "text": {}}}"#, serde_json::json!(t)))
        .collect();
    let alias = alias.map_or(String::new(), |a| format!(r#", "alias": "{a}""#));
    format!(
        r#"{{"k": "union", "types": [{}]{alias}}}"#,
        types.join(", ")
    )
}

fn imported(exports: &[String], policy: &Policy) -> Generated {
    let json = format!(
        r#"{{"package": "p", "version": "1.0.0", "declarations": "p/index.d.ts", "exports": [{}]}}"#,
        exports.join(",\n")
    );
    let surface = Surface::parse(&json).expect("surface parses");
    generate(&surface, policy, None, "P.Generated")
}

fn status_of(generated: &Generated, export: &str) -> (Status, String) {
    let e = (generated.entries.iter())
        .find(|e| e.export == export)
        .expect("entry");
    (e.status, e.detail.clone())
}

#[test]
fn a_named_union_of_literals_is_an_enum() {
    let method = union(
        Some("Method"),
        &[r#""GET""#, r#""POST""#, r#""content-type""#],
    );
    let generated = imported(
        &[taking("send", &method), taking("again", &method)],
        &Policy::default(),
    );
    assert_eq!(
        status_of(&generated, "send"),
        (Status::Supported, "Method ->{} Unit".into())
    );
    assert_eq!(status_of(&generated, "again").0, Status::Supported);
    let declaration =
        "untagged type Method = Get = \"GET\" | Post = \"POST\" | ContentType = \"content-type\"\n";
    assert_eq!(
        generated.text.matches(declaration).count(),
        1,
        "{}",
        generated.text
    );
}

#[test]
fn integers_and_clashing_names_need_the_policy() {
    let code = union(Some("Code"), &["200", "-1"]);
    let case = union(Some("Case"), &[r#""get""#, r#""GET""#]);
    let verb = union(Some("Verb"), &[r#""Get""#, r#""put""#]);
    let exports = [
        taking("status", &code),
        taking("cased", &case),
        taking("verb", &verb),
    ];
    let generated = imported(&exports, &Policy::default());
    for (export, reason) in [
        (
            "status",
            "the literal `200` of `Code` has no constructor name: give it one in `[enums.Code]`",
        ),
        (
            "cased",
            "two literals of `Case` are the constructor `Get`: name them in `[enums.Case]`",
        ),
    ] {
        assert_eq!(
            status_of(&generated, export),
            (Status::NeedsPolicy, reason.into())
        );
    }
    // with `Case` blocked, `Verb` takes `Get`
    assert_eq!(status_of(&generated, "verb").0, Status::Supported);
    let names = |pairs: &[(&str, &str)]| {
        (pairs.iter())
            .map(|(l, c)| ((*l).to_owned(), (*c).to_owned()))
            .collect()
    };
    let policy = Policy {
        enums: BTreeMap::from([
            ("Code".into(), names(&[("200", "Ok"), ("-1", "Failed")])),
            ("Case".into(), names(&[("get", "Lower")])),
        ]),
        ..Policy::default()
    };
    let generated = imported(&exports, &policy);
    assert_eq!(status_of(&generated, "status").0, Status::Supported);
    assert!(
        generated
            .text
            .contains("untagged type Code = Ok = 200 | Failed = -1\n"),
        "{}",
        generated.text
    );
    // `Case` comes first and takes `Get`; `Verb` is blocked instead
    assert_eq!(status_of(&generated, "cased").0, Status::Supported);
    assert_eq!(
        status_of(&generated, "verb"),
        (
            Status::NeedsPolicy,
            "the constructor `Get` of `Verb` is taken by `Case`: name it in `[enums.Verb]`".into()
        )
    );
}

#[test]
fn other_unions_of_literals_are_refused() {
    let generated = imported(
        &[
            taking("mixed", &union(Some("Mixed"), &[r#""a""#, "1"])),
            taking("flags", &union(Some("Flags"), &["true", r#""auto""#])),
            taking("fraction", &union(Some("Half"), &["0.5", "1"])),
            taking("anonymous", &union(None, &[r#""a""#, r#""b""#])),
            taking("escaped", &union(Some("Quote"), &[r#""\"""#, r#""b""#])),
        ],
        &Policy::default(),
    );
    for (export, reason) in [
        ("mixed", "its literals are not all strings or all integers"),
        ("flags", "its literals are not all strings or all integers"),
        (
            "fraction",
            "its literals are not all strings or all integers",
        ),
        (
            "anonymous",
            "has no name: a `type` alias of literals is an enum",
        ),
        ("escaped", "has an escape"),
    ] {
        let (status, detail) = status_of(&generated, export);
        assert_eq!(status, Status::Unsupported, "{export}");
        assert!(detail.contains(reason), "{export}: {detail}");
    }
    assert!(!generated.text.contains("untagged"), "{}", generated.text);
}

#[cfg(feature = "native-project")]
#[test]
fn the_policy_names_constructors() {
    let policy = Policy::parse("[enums.Code]\n200 = \"Ok\"\n").expect("policy parses");
    assert_eq!(policy.enums["Code"]["200"], "Ok");
    assert_eq!(
        Policy::parse("[enums.Code]\n200 = \"ok\"\n").expect_err("refused"),
        "`ok` of `[enums.Code]` is no constructor name: it starts with a capital letter"
    );
}
