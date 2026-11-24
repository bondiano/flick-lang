use super::*;

fn open(lsp: &mut Lsp, file: &str, text: &str) -> Vec<Value> {
    lsp.notify(
        "textDocument/didOpen",
        &json!({ "textDocument": { "uri": file, "languageId": "flick", "version": 1, "text": text } }),
    );
    lsp.diagnostics(file)
}

fn hover(lsp: &mut Lsp, file: &str, source: &str, offset: usize, expected: &[&str]) -> Value {
    let response = lsp.request(
        "textDocument/hover",
        &json!({ "textDocument": { "uri": file }, "position": position(source, offset) }),
    );
    let result = &response["result"];
    assert_eq!(result["contents"]["kind"], "markdown", "{response}");
    let markdown = result["contents"]["value"].as_str().expect("hover text");
    for part in expected {
        assert!(
            markdown.contains(part),
            "missing {part:?} at {offset}: {markdown}"
        );
    }
    result["range"].clone()
}

fn offset(source: &str, phrase: &str, token: &str) -> usize {
    source.find(phrase).expect("hover site") + phrase.find(token).expect("token in site")
}

fn range(source: &str, offset: usize, token: &str) -> Value {
    json!({ "start": position(source, offset), "end": position(source, offset + token.len()) })
}

#[test]
fn proof_hover_explains_declarations_goals_and_rule_arguments() {
    let source = "module App.Main exposing (..)

/// Equality can be reversed.
proof reversed (x : Bool) (y : Bool) (h : equal x y) where equal y x = symm h
proof both (x : Bool) (y : Bool) (z : Bool) (p : equal x y) (q : equal y z) where equal x z = trans y p q
proof eta (f : a -> b) where equal (fun x -> f x) f = funext (fun y -> refl)
proof lifted (x : a) (y : a) (h : equal x y) where equal (Some y) (Some x) =
  let back = symm h
  cong (fun z -> Some z) back
proof rebuilt (v : Option a) where equal (match v with | None -> None | Some x -> Some x) v =
  match v with
  | None -> refl
  | Some y -> refl
proof same where same_type a a = refl
proof yes where holds true = refl
";
    let dir = project(
        "lsp_proof_rule_hover",
        &[("flick.toml", LIB), ("src/App/Main.fl", source)],
    );
    let main = uri(&dir.join("src/App/Main.fl"));
    let mut lsp = Lsp::start(&dir);
    let diagnostics = open(&mut lsp, &main, source);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let header = "proof reversed (x : Bool) (y : Bool) (h : equal x y) where equal y x";
    for name in ["proof", "reversed"] {
        let at = offset(source, header, name);
        assert_eq!(
            hover(
                &mut lsp,
                &main,
                source,
                at,
                &[header, "Equality can be reversed.", "**checked**"]
            ),
            range(source, at, name)
        );
    }

    let cases: &[(&str, &str, &[&str])] = &[
        ("where equal y x", "equal", &["Value equality", "`y = x`"]),
        ("equal y x =", "y", &["y : Bool", "`y = x`"]),
        ("x = symm h", "x", &["x : Bool", "`y = x`"]),
        ("reversed (x : Bool)", "x", &["x : Bool"]),
        ("h : equal x y", "h", &["**h**", "`x = y`"]),
        ("symm h", "symm", &["Symmetry", "`y = x`"]),
        ("symm h", "h", &["**h**", "reversed equality", "`x = y`"]),
        ("trans y p q", "trans", &["Transitivity", "`x = z`"]),
        ("trans y p q", "y", &["middle term", "y : Bool"]),
        ("trans y p q", "p", &["first proof", "`x = y`"]),
        ("trans y p q", "q", &["second proof", "`y = z`"]),
        (
            "funext (fun y -> refl)",
            "funext",
            &["Function extensionality", "**Goal**"],
        ),
        (
            "fun y -> refl",
            "y",
            &["y : a", "fresh argument", "**Goal**"],
        ),
        (
            "let back = symm",
            "let",
            &["introduces a local fact", "back", "`y = x`"],
        ),
        (
            "cong (fun z -> Some z) back",
            "cong",
            &["Congruence", "**Goal**"],
        ),
        (
            "cong (fun z -> Some z) back",
            "fun",
            &["function applied to both sides"],
        ),
        (
            "Some z) back",
            "back",
            &["**back**", "`y = x`", "before applying the context"],
        ),
        ("match v with\n", "match", &["each constructor", "**Goal**"]),
        (
            "v with\n  | None",
            "v",
            &["v : Option a", "each constructor"],
        ),
        ("None -> refl", "None", &["**Branch**", "`v = None`"]),
        ("Some y -> refl", "Some", &["**Branch**", "`v = Some y`"]),
        ("Some y -> refl", "y", &["y : a", "`v = Some y`"]),
        ("Some y -> refl", "refl", &["Reflexivity", "`v = Some y`"]),
        (
            "same_type a a",
            "same_type",
            &["Type equality", "`same_type a a`"],
        ),
        (
            "holds true",
            "holds",
            &["Boolean proposition", "`true = true`"],
        ),
    ];
    for &(site, token, expected) in cases {
        hover(
            &mut lsp,
            &main,
            source,
            offset(source, site, token),
            expected,
        );
    }
    lsp.close();
}

#[test]
fn proof_hover_and_navigation_find_the_same_aliased_lemma() {
    let base = "module App.Base exposing (..)

/// Reflexivity at any type.
proof identical (x : a) where equal x x = refl
proof inverted (x : Bool) (y : Bool) (reason : equal x y) where equal y x = symm reason
";
    let source = "module App.Main exposing (..)

import App.Base as B

proof here (x : Bool) where equal x x = B.identical x
proof again (b : Bool) where equal b b = B.identical b
proof local (b : Bool) where equal b b = again b
proof supplied (p : Bool) (q : Bool) (known : equal p q) where equal q p = B.inverted p q known
";
    let dir = project(
        "lsp_proof_lemma_navigation",
        &[
            ("flick.toml", LIB),
            ("proofs/App/Base.fl", base),
            ("proofs/App/Main.fl", source),
        ],
    );
    let base_file = uri(&dir.join("proofs/App/Base.fl"));
    let main = uri(&dir.join("proofs/App/Main.fl"));
    let mut lsp = Lsp::start(&dir);
    let diagnostics = open(&mut lsp, &main, source);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let lemma = offset(source, "B.identical x", "identical");
    let lemma_range = range(source, lemma, "identical");
    assert_eq!(
        hover(
            &mut lsp,
            &main,
            source,
            lemma,
            &[
                "proof identical (x : a) where equal x x",
                "Reflexivity at any type.",
                "Arguments here · `x`",
                "`x = x`",
            ]
        ),
        lemma_range
    );
    hover(
        &mut lsp,
        &main,
        source,
        offset(source, "= again b", "again"),
        &[
            "proof again (b : Bool) where equal b b",
            "Arguments here · `b`",
            "`b = b`",
        ],
    );
    hover(
        &mut lsp,
        &main,
        source,
        offset(source, "B.inverted p q known", "inverted"),
        &[
            "proof inverted (x : Bool) (y : Bool) (reason : equal x y)",
            "`p`, `q`, `known`",
            "`p = q`",
            "`q = p`",
        ],
    );
    let base_decl = json!({
        "uri": base_file,
        "range": range(base, offset(base, "proof identical", "identical"), "identical"),
    });
    let params = json!({ "textDocument": { "uri": main }, "position": position(source, lemma) });
    let definition = lsp.request("textDocument/definition", &params);
    assert_eq!(definition["result"], base_decl, "{definition}");
    let mut params = params;
    params["context"] = json!({ "includeDeclaration": true });
    let references = lsp.request("textDocument/references", &params);
    let locations = references["result"]
        .as_array()
        .expect("reference locations");
    let second = range(
        source,
        offset(source, "B.identical b", "identical"),
        "identical",
    );
    assert_eq!(locations.len(), 3, "{references}");
    for expected in [
        base_decl,
        json!({ "uri": main, "range": lemma_range }),
        json!({ "uri": main, "range": second }),
    ] {
        assert!(locations.contains(&expected), "{references}");
    }
    lsp.close();
}

#[test]
fn proof_hover_covers_laws_bundles_and_unsaved_utf16_positions() {
    let source = "module App.Main exposing (..)

class Witness (a : Type) =
  /// The class law.
  law witness (x : a) where equal x x

proof wrong where equal true true = refl
proof opt where lawful Monad Option = {
  Applicative = {
    Functor = { identity = wrong }
  }
}
proof utf16 (x : Bool) (y : Bool) where equal /* 🥝 */ x x = refl

let outside = true
";
    let dir = project(
        "lsp_proof_bundle_hover",
        &[("flick.toml", LIB), ("src/App/Main.fl", source)],
    );
    let main = uri(&dir.join("src/App/Main.fl"));
    let mut lsp = Lsp::start(&dir);
    let diagnostics = open(&mut lsp, &main, source);
    assert!(
        diagnostics.iter().any(|d| d["code"] == "FLICK-T083"),
        "{diagnostics:?}"
    );

    let header = "law witness (x : a) where equal x x";
    for token in ["law", "witness"] {
        let at = offset(source, header, token);
        assert_eq!(
            hover(
                &mut lsp,
                &main,
                source,
                at,
                &[header, "The class law.", "Law of `Witness`", "`x = x`"]
            ),
            range(source, at, token)
        );
    }
    let bundle: &[(&str, &str, &[&str])] = &[
        (
            "lawful Monad Option",
            "lawful",
            &["selected instance and its superclasses", "Monad Option"],
        ),
        (
            "lawful Monad Option",
            "Monad",
            &["dictionary selected", "Monad Option"],
        ),
        (
            "lawful Monad Option",
            "Option",
            &["dictionary selected", "Monad Option"],
        ),
        (
            "Applicative =",
            "Applicative",
            &["superclass path", "Applicative.Functor.identity"],
        ),
        (
            "Functor =",
            "Functor",
            &["superclass path", "Applicative.Functor.identity"],
        ),
        (
            "identity = wrong",
            "identity",
            &[
                "instantiated law",
                "Applicative.Functor.identity",
                "**Goal**",
                "map",
            ],
        ),
        (
            "identity = wrong",
            "wrong",
            &["Applicative.Functor.identity", "**Goal**", "map"],
        ),
    ];
    for &(site, token, expected) in bundle {
        hover(
            &mut lsp,
            &main,
            source,
            offset(source, site, token),
            expected,
        );
    }

    let key = offset(source, "identity = wrong", "identity");
    let params = json!({
        "textDocument": {"uri": main}, "position": position(source, key),
        "context": {"includeDeclaration": true},
    });
    let definition = lsp.request("textDocument/definition", &params);
    let law = &definition["result"];
    let path = law["uri"]
        .as_str()
        .and_then(|uri| uri.strip_prefix("file://"))
        .unwrap_or_else(|| panic!("{definition}"));
    let found = fs::read_to_string(path).expect("the actual law source");
    let line = usize::try_from(law["range"]["start"]["line"].as_u64().expect("law line"))
        .expect("small line");
    assert!(
        found
            .lines()
            .nth(line)
            .is_some_and(|line| line.contains("map identity v")),
        "{law}"
    );
    let references = lsp.request("textDocument/references", &params);
    assert!(
        references["result"]
            .as_array()
            .expect("law references")
            .contains(&json!({"uri": main, "range": range(source, key, "identity")})),
        "{references}"
    );

    let symbols = lsp.request(
        "textDocument/documentSymbol",
        &json!({ "textDocument": { "uri": main } }),
    );
    let outline = symbols["result"].as_array().expect("outline");
    assert!(outline.iter().any(|s| s["name"] == "opt"), "{symbols}");
    let class = outline
        .iter()
        .find(|s| s["name"] == "Witness")
        .expect("class");
    assert!(
        class["children"]
            .as_array()
            .expect("class members")
            .iter()
            .any(|s| s["name"] == "witness"),
        "{class}"
    );
    for name in ["witness", "utf16"] {
        let symbols = lsp.request("workspace/symbol", &json!({ "query": name }));
        assert!(
            symbols["result"]
                .as_array()
                .expect("workspace symbols")
                .iter()
                .any(|s| { s["name"] == name && s["location"]["uri"] == main }),
            "{symbols}"
        );
    }
    hover(
        &mut lsp,
        &main,
        source,
        offset(source, "let outside", "outside"),
        &["Bool"],
    );

    let edited = source.replace("/* 🥝 */ x x = refl", "/* 🥝 */ x y =");
    lsp.notify(
        "textDocument/didChange",
        &json!({ "textDocument": { "uri": main, "version": 2 }, "contentChanges": [{ "text": edited }] }),
    );
    let _ = lsp.diagnostics(&main);
    let at = offset(&edited, "proof utf16", "utf16");
    hover(&mut lsp, &main, &edited, at, &["**open**", "`x = y`"]);
    let at = offset(&edited, "/* 🥝 */ x y", "y");
    assert_eq!(
        hover(&mut lsp, &main, &edited, at, &["y : Bool", "`x = y`"]),
        range(&edited, at, "y")
    );
    lsp.close();
}
