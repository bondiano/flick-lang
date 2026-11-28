//! IDE code-action preservation and namespaced-attribute isolation tests.

use crate::source::SourceMap;
use crate::types::Thresholds;

#[cfg(feature = "ide")]
#[test]
fn ide_code_actions_preserve_plugin_declared_attributes() {
    use super::{MetaHost, project_with, roots};
    use crate::ide::Ide;
    use crate::project::load::analyze;

    // 1. `let ... else` -> `match` action validates full module with active plugin attributes.
    let let_else_src = r#"module App.Main exposing (User(..), unwrap_or)

@meta.prefix "usr"
type User = User {
  @rename "user_id"
  userName : String
  @default -1
  retries : Int
}

unwrap_or : Option Int -> Int
let unwrap_or opt =
  let Some x = opt else 0
  x + 1
"#;
    let (host1, _) = MetaHost::with_meta(let_else_src);
    let mut sources1 = SourceMap::default();
    let project1 = project_with(&mut sources1, &["meta"]);
    let analysis1 = analyze(&host1, &mut sources1, Some(&project1), &roots());
    assert!(
        analysis1.diagnostics.is_empty(),
        "{:#?}",
        analysis1.diagnostics
    );
    let file1 = analysis1.module("App.Main").expect("App.Main").file;
    let ide1 = Ide {
        analysis: &analysis1,
        sources: &sources1,
    };
    let let_else_off = u32::try_from(
        let_else_src
            .find("let Some x = opt else 0")
            .expect("let else offset"),
    )
    .expect("u32");
    let let_else_actions = ide1.actions(file1, let_else_off, let_else_off);
    assert!(
        let_else_actions.iter().any(|a| a.title.contains("`match`")),
        "expected `let ... else` -> `match` action in module with plugin attributes, got: {let_else_actions:#?}"
    );

    // 2. GADT `todo` signature inference validates full module with active plugin attributes.
    let sig_src = r#"module App.Main exposing (User(..), Expr(..), eval)

@meta.prefix "usr"
type User = User {
  @rename "user_id"
  userName : String
  @default -1
  retries : Int
}

type Expr a =
  @alias "i"
  Lit Int : Expr Int

eval : todo
let eval e =
  match e with
    | Lit n -> n
"#;
    let (host2, _) = MetaHost::with_meta(sig_src);
    let mut sources2 = SourceMap::default();
    let project2 = project_with(&mut sources2, &["meta"]);
    let analysis2 = analyze(&host2, &mut sources2, Some(&project2), &roots());
    let file2 = analysis2.module("App.Main").expect("App.Main").file;
    let ide2 = Ide {
        analysis: &analysis2,
        sources: &sources2,
    };
    let todo_off = u32::try_from(sig_src.find("todo").expect("todo offset")).expect("u32");
    let sig_actions = ide2.actions(file2, todo_off, todo_off);
    assert!(
        sig_actions.iter().any(|a| a.title.contains("Expr a -> a")),
        "expected GADT signature inference action in module with plugin attributes, got: {sig_actions:#?}"
    );
}

const NS_CHECK_SRC: &str = r#"@when "js"
module App.NsCheck exposing (..)

@when.tag
type WhenThing = WhenThing

@label.short
type Tag = Tag

tag_val : Tag
let tag_val = Tag

@cell.id
type Boxed a = Boxed a

mk : Int -> Int
@maker.id
let mk x = x

@owner.id
@sync.reader
service CustomSvc =
  ping : Unit ->{} Int

@stores.id
type Handle

@implicit.tag
instance Show Tag =
  let show _ = "Tag"

@derive.skip
class Printable a =
  fmt : a -> String

@ts.none
type JsThing = JsThing

@go.import "pkg" "GoThing"
type GoThing = GoThing

exported_fn : Int -> Int
@export.csv "exported_fn"
let exported_fn x = x

@lang.int
type NotInt = NotInt
"#;

#[test]
fn namespaced_plugin_attributes_do_not_trigger_core_or_backend_behavior() {
    use crate::plugin::derive::derived_classes;
    use crate::resolve::{Module, NO_GENERATED, module_syntax, resolve};
    use crate::syntax::parse;
    use crate::types::{
        ForeignBackendSpec, Limits, Options, PluginAttribute, Watched, builtins_of, check_typed,
    };
    use flick_plugin::{AttributeSpec, AttributeTarget};

    let names = [
        "label.short",
        "cell.id",
        "maker.id",
        "owner.id",
        "stores.id",
        "implicit.tag",
        "derive.skip",
        "ts.none",
        "go.import",
        "export.csv",
        "when.tag",
        "sync.reader",
        "lang.int",
    ];
    let plugin_attrs: Vec<PluginAttribute> = names
        .iter()
        .map(|name| PluginAttribute {
            package: "ns_test".to_owned(),
            spec: AttributeSpec {
                name: (*name).to_owned(),
                targets: vec![AttributeTarget::Declaration],
                doc: String::new(),
            },
        })
        .collect();

    let mut sources = SourceMap::default();
    let file = sources.add("App/NsCheck.fl", NS_CHECK_SRC);
    let parsed = parse(NS_CHECK_SRC, file);
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    assert!(derived_classes(&parsed.tree, NS_CHECK_SRC).is_empty());

    let syntax = module_syntax(&parsed.tree, NS_CHECK_SRC, file);
    assert_eq!(
        syntax
            .header
            .as_ref()
            .and_then(|h| h.when.as_ref().map(|(c, _)| c.as_str())),
        Some("js")
    );
    let module = Module {
        name: "App.NsCheck",
        file,
        text: NS_CHECK_SRC,
        tree: &parsed.tree,
        syntax: &syntax,
        generated: NO_GENERATED,
    };

    let builtins = builtins_of(false);
    let (mut js_iface, resolve_diags) = resolve(&module, &|_| None);
    assert!(resolve_diags.is_empty(), "{resolve_diags:#?}");
    let watched = Watched::default();
    let js_opts = Options {
        local: &|_| true,
        limits: Limits::default(),
        no_let_assert: false,
        thresholds: Thresholds::default(),
        watched: &watched,
        stages_by_class: false,
        foreign_backend: ForeignBackendSpec::default_js(),
        validate_foreign_metadata: true,
        native_overrides: None,
        plugin_attributes: &plugin_attrs,
    };
    let (js_diags, js_typing) = check_typed(&module, &|_| None, builtins, js_opts, &mut js_iface);
    assert!(js_diags.is_empty(), "JS check diagnostics: {js_diags:#?}");
    assert!(js_typing.labelled.is_empty(), "{:?}", js_typing.labelled);
    assert!(js_typing.cells.is_empty(), "{:?}", js_typing.cells);
    assert!(js_typing.makers.is_empty(), "{:?}", js_typing.makers);
    assert!(js_typing.owner_services.is_empty());
    assert!(js_typing.host_exports.is_empty());
    for core_tag in ["ts", "label", "cell", "maker", "owner", "stores"] {
        assert!(!js_iface.tags.contains_key(core_tag), "{:?}", js_iface.tags);
    }
    assert!(js_iface.instances.iter().all(|i| !i.implicit));

    let (mut go_iface, _) = resolve(&module, &|_| None);
    let go_spec = ForeignBackendSpec::go();
    let go_opts = Options {
        foreign_backend: &go_spec,
        ..js_opts
    };
    let (go_diags, go_typing) = check_typed(&module, &|_| None, builtins, go_opts, &mut go_iface);
    assert!(go_diags.is_empty(), "Go check diagnostics: {go_diags:#?}");
    assert!(!go_iface.tags.contains_key("go"), "{:?}", go_iface.tags);
    assert!(
        go_typing
            .ir_type_decls
            .iter()
            .all(|td| td.foreign_forms.is_empty())
    );
}
