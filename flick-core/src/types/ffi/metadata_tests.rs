use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::resolve;
use crate::source::FileId;
use crate::syntax;
use crate::types::ffi::{AdapterArgKind, AdapterArgSpec, AdapterWordSpec, ForeignBackendSpec};
use crate::types::{self, Options, StaticContext, Watched};
use flick_ir::AdapterArg;

fn text_backend() -> ForeignBackendSpec {
    ForeignBackendSpec {
        backend_id: "test/text".into(),
        adapters: vec![AdapterWordSpec {
            name: "symbol".into(),
            args: vec![AdapterArgSpec {
                name: "sym".into(),
                kind: AdapterArgKind::Text,
            }],
            repeatable: true,
            doc: "Target symbol name.".into(),
        }],
        type_form_attributes: Vec::new(),
        value_abi: None,
    }
}

fn check_with_backend(
    source: &str,
    backend: &ForeignBackendSpec,
) -> (Vec<Diagnostic>, types::Typing) {
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
            foreign_backend: backend,
            validate_foreign_metadata: true,
            native_overrides: None,
            plugin_attributes: &[],
        },
        &mut iface,
        StaticContext {
            dictionaries: &[],
            witnesses: &[],
            #[cfg(not(target_arch = "wasm32"))]
            ir_lookup: None,
            skip_ir_types: true,
        },
    );
    diags.extend(checked);
    (diags, typing)
}

#[test]
fn text_adapter_args_accept_plain_raw_and_multiline_strings() {
    let source = concat!(
        "module Probe exposing (f)\n",
        "f : Unit ->{} Unit\n",
        "foreign f from \"./m.js\" \"f\"\n",
        "  symbol \"plain\\nline\"\n",
        "  symbol r\"C:\\temp\"\n",
        "  symbol r#\"say \"hi\"\"#\n",
        "  symbol \"\"\"\n    multi\n    \"\"\"\n",
    );
    let (diags, typing) = check_with_backend(source, &text_backend());
    assert!(
        !diags.iter().any(|d| d.severity == Severity::Error),
        "{diags:?}"
    );
    let info = typing.foreigns.values().next().expect("foreign info");
    let args: Vec<&AdapterArg> = info.adapter.words.iter().map(|w| &w.args[0]).collect();
    assert_eq!(
        args,
        vec![
            &AdapterArg::String("plain\nline".into()),
            &AdapterArg::String("C:\\temp".into()),
            &AdapterArg::String("say \"hi\"".into()),
            &AdapterArg::String("multi".into()),
        ]
    );
}

#[test]
fn text_adapter_args_reject_postfix_and_interpolated_expressions() {
    let backend = text_backend();
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
        let (diags, typing) = check_with_backend(&source, &backend);
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
