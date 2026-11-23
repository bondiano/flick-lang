use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::resolve;
use crate::source::FileId;
use crate::syntax;
use crate::syntax::foreign_meta::{AdapterArgKind, AdapterArgSpec, AdapterWordSpec, ForeignMeta};
use crate::types::{self, Options, StaticContext, Watched};
use flick_ir::AdapterArg;

fn text_meta(choices: &[&str]) -> ForeignMeta {
    ForeignMeta {
        backend_id: "test/text".into(),
        adapter_words: vec![AdapterWordSpec {
            name: "symbol".into(),
            args: vec![AdapterArgSpec {
                name: "sym".into(),
                kind: AdapterArgKind::Text,
                choices: choices.iter().map(|s| (*s).to_owned()).collect(),
            }],
            repeatable: true,
            help: "Target symbol name.".into(),
        }],
        type_form_attributes: vec![],
    }
}

fn check_with_meta(source: &str, meta: &ForeignMeta) -> (Vec<Diagnostic>, types::Typing) {
    let file = FileId::default();
    let parsed = syntax::parse(source, file);
    let mod_syntax = resolve::module_syntax(&parsed.tree, source, file);
    let module = resolve::Module {
        name: "Probe",
        file,
        text: source,
        tree: &parsed.tree,
        syntax: &mod_syntax,
        generated: resolve::NO_GENERATED,
    };
    let builtins = types::builtins();
    let lookup = |name: &str| builtins.iter().find(|i| i.module == name).cloned();
    let (mut iface, mut diags) = resolve::resolve(&module, &lookup);
    diags.extend(parsed.diagnostics);
    let local = |name: &str| name == "Probe";
    let (checked, typing) = types::check_typed_with_static_context(
        &module,
        &lookup,
        builtins,
        Options {
            local: &local,
            limits: types::Limits::default(),
            no_let_assert: false,
            thresholds: types::Thresholds::default(),
            watched: Watched::none(),
            stages_by_class: false,
        },
        &mut iface,
        StaticContext {
            dictionaries: &[],
            witnesses: &[],
            #[cfg(not(target_arch = "wasm32"))]
            ir_lookup: None,
            skip_ir_types: true,
            foreign_meta: Some(meta),
        },
    );
    diags.extend(checked);
    (diags, typing)
}

#[test]
fn text_adapter_args_accept_plain_raw_and_multiline_strings_and_enforce_choices() {
    let source = concat!(
        "module Probe exposing (f)\n",
        "f : Unit ->{} Unit\n",
        "foreign f from \"./m.js\" \"f\"\n",
        "  symbol \"plain\\nline\"\n",
        "  symbol r\"C:\\temp\"\n",
        "  symbol r#\"say \"hi\"\"#\n",
        "  symbol \"\"\"\n    multi\n    \"\"\"\n",
    );
    let (diags, typing) = check_with_meta(source, &text_meta(&[]));
    assert!(
        !diags.iter().any(|d| d.severity == Severity::Error),
        "{diags:?}"
    );
    let info = typing.foreigns.values().next().expect("foreign info");
    let args: Vec<&AdapterArg> = info.adapter.words.iter().map(|w| &w.args[0]).collect();
    assert_eq!(
        args,
        vec![
            &AdapterArg::Text("plain\nline".into()),
            &AdapterArg::Text("C:\\temp".into()),
            &AdapterArg::Text("say \"hi\"".into()),
            &AdapterArg::Text("multi".into()),
        ]
    );

    // Declared choices accept matching decoded values and reject others with FLICK-F001.
    let ok_choice = concat!(
        "module Probe exposing (f)\n",
        "f : Unit ->{} Unit\n",
        "foreign f from \"./m.js\" \"f\"\n",
        "  symbol \"c\"\n",
    );
    let (diags, typing) = check_with_meta(ok_choice, &text_meta(&["c", "stdcall"]));
    assert!(
        !diags.iter().any(|d| d.severity == Severity::Error),
        "{diags:?}"
    );
    let info = typing.foreigns.values().next().expect("foreign info");
    assert_eq!(
        info.adapter.words[0].args,
        vec![AdapterArg::Text("c".into())]
    );

    let bad_choice = ok_choice.replace("\"c\"", "\"fast\"");
    let (diags, typing) = check_with_meta(&bad_choice, &text_meta(&["c", "stdcall"]));
    assert!(
        diags.iter().any(|d| d.code == codes::F001),
        "disallowed Text choice must emit FLICK-F001: {diags:?}"
    );
    let info = typing.foreigns.values().next().expect("foreign info");
    assert!(info.adapter.words.is_empty());
}

#[test]
fn text_adapter_args_reject_postfix_and_interpolated_expressions() {
    let meta = text_meta(&[]);
    let cases = [
        ("field_access", r#""foo".bar"#),
        ("call_expr", r#""foo"()"#),
        ("interpolated", r#""hi \{x}""#),
        ("parenthesized", r#"("foo")"#),
        ("identifier", "foo"),
        ("number", "123"),
        ("bool", "true"),
    ];
    for (label, arg) in cases {
        let source = format!(
            "module Probe exposing (f)\n\
             x : String\n\
             let x = \"world\"\n\
             f : Unit ->{{}} Unit\n\
             foreign f from \"./m.js\" \"f\"\n\
             \x20\x20symbol {arg}\n"
        );
        let (diags, typing) = check_with_meta(&source, &meta);
        assert!(
            diags.iter().any(|d| d.code == codes::F001),
            "{label}: expected FLICK-F001 for `symbol {arg}`, got {diags:?}"
        );
        let info = typing.foreigns.values().next().expect("foreign info");
        assert!(
            info.adapter.words.is_empty(),
            "{label}: malformed Text arg `{arg}` must not be recorded in adapter.words: {:?}",
            info.adapter.words
        );
    }
}
