use std::collections::BTreeMap;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use super::Ide;
use crate::diagnostics::{Severity, codes};
use crate::ir::{AdapterArg, ItemKind};
use crate::project::{self, BackendTags, Host, Project, config};
use crate::source::SourceMap;
use crate::syntax::foreign_meta::{
    AdapterArgKind, AdapterArgSpec, AdapterWordSpec, ForeignMeta, TypeFormAttrSpec,
};

struct BackendHost {
    files: BTreeMap<PathBuf, String>,
    tags: Vec<BackendTags>,
}

impl BackendHost {
    fn new(files: &[(&str, &str)], tags: Vec<BackendTags>) -> Self {
        Self {
            files: files
                .iter()
                .map(|(p, text)| (PathBuf::from("/p").join(p), (*text).to_owned()))
                .collect(),
            tags,
        }
    }
}

impl Host for BackendHost {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.files
            .get(path)
            .map(|t| t.clone().into_bytes())
            .ok_or_else(|| ErrorKind::NotFound.into())
    }

    fn display(&self, path: &Path) -> String {
        path.strip_prefix("/p")
            .unwrap_or(path)
            .display()
            .to_string()
    }

    fn backend_tags(&self, _: &Project) -> Vec<BackendTags> {
        self.tags.clone()
    }
}

fn literal_backend_tags() -> BackendTags {
    BackendTags {
        id: "test/literal".into(),
        short: "literal".into(),
        tags: vec!["literal".into()],
        axes: vec![],
        environments: vec![("literal".into(), "default".into())],
        foreign_meta: ForeignMeta {
            backend_id: "test/literal".into(),
            adapter_words: vec![AdapterWordSpec {
                name: "slot".into(),
                args: vec![AdapterArgSpec {
                    name: "index".into(),
                    kind: AdapterArgKind::Index,
                    choices: vec![],
                }],
                repeatable: true,
                help: "Bind parameter `index` to a literal target calling slot.".into(),
            }],
            type_form_attributes: vec![TypeFormAttrSpec {
                name: "lit".into(),
                help: "Literal target type form annotation.".into(),
            }],
        },
    }
}

fn analyze_project(
    toml: &str,
    source: &str,
    tags: Vec<BackendTags>,
) -> (SourceMap, project::Analysis) {
    let host = BackendHost::new(&[("flick.toml", toml), ("src/App.fl", source)], tags);
    let mut sources = SourceMap::default();
    let cfg_file = sources.add("flick.toml", toml);
    let project = Project {
        root: PathBuf::from("/p"),
        config: config::parse(toml, cfg_file).expect("valid config"),
        config_file: cfg_file,
        workspace: None,
    };
    let analysis = project::analyze(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/App.fl")],
    );
    (sources, analysis)
}

const JS_TOML: &str = "schema-version = 1\n[project]\nkind = \"lib\"\n[build]\ntarget = \"js\"\n";
const GO_TOML: &str = "schema-version = 1\n[project]\nkind = \"lib\"\n[build]\ntarget = \"go\"\n";
const LITERAL_TOML: &str = "schema-version = 1\n[project]\nkind = \"lib\"\n\
[build.backend]\nid = \"test/literal\"\nversion = \"1.0.0\"\n\
sha256 = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\n\
component = \"backend.wasm\"\n";

#[test]
fn foreign_completion_offers_only_the_package_backends_adapter_words() {
    let source = "module App exposing (f)\n\
f : String ->{} String\n\
foreign f from \"./m.js\" \"f\"\n  arity 1\n  \n";
    let offset = u32::try_from(source.len() - 1).expect("small");

    // JS project: core contract words + JS adapter words (`this`, `cancel`, `variadic`), never `slot`.
    let (js_sources, js_analysis) = analyze_project(JS_TOML, source, vec![]);
    let js_ide = Ide {
        analysis: &js_analysis,
        sources: &js_sources,
    };
    let js_file = js_analysis.module("App").expect("App").file;
    let js_labels: Vec<String> = js_ide
        .complete(js_file, offset, false)
        .into_iter()
        .map(|c| c.label)
        .collect();
    for expected in [
        "arity",
        "catch",
        "retention",
        "nullable",
        "optional",
        "reviewed",
        "owned",
        "this",
        "cancel",
        "variadic",
    ] {
        assert!(
            js_labels.iter().any(|l| l == expected),
            "JS completion missing `{expected}`: {js_labels:?}"
        );
    }
    assert!(
        !js_labels.iter().any(|l| l == "slot"),
        "JS completion must not offer literal backend word `slot`: {js_labels:?}"
    );

    // Custom `test/literal` project: core contract words + `slot`, NEVER `this`/`cancel`/`variadic`.
    let (lit_sources, lit_analysis) =
        analyze_project(LITERAL_TOML, source, vec![literal_backend_tags()]);
    let lit_ide = Ide {
        analysis: &lit_analysis,
        sources: &lit_sources,
    };
    let lit_file = lit_analysis.module("App").expect("App").file;
    let lit_labels: Vec<String> = lit_ide
        .complete(lit_file, offset, false)
        .into_iter()
        .map(|c| c.label)
        .collect();
    for expected in [
        "arity",
        "catch",
        "retention",
        "nullable",
        "optional",
        "reviewed",
        "owned",
        "slot",
    ] {
        assert!(
            lit_labels.iter().any(|l| l == expected),
            "literal completion missing `{expected}`: {lit_labels:?}"
        );
    }
    for forbidden in ["this", "cancel", "variadic"] {
        assert!(
            !lit_labels.iter().any(|l| l == forbidden),
            "literal completion must not offer JS word `{forbidden}`: {lit_labels:?}"
        );
    }
}

#[test]
fn foreign_arg_completion_offers_choices_and_lets_catch_fall_through() {
    let cancel_src = "module App exposing (f)\n\
f : String ->{Async} String\n\
foreign f from \"./m.js\" \"f\"\n  cancel \n";
    let (sources, analysis) = analyze_project(JS_TOML, cancel_src, vec![]);
    let ide = Ide {
        analysis: &analysis,
        sources: &sources,
    };
    let file = analysis.module("App").expect("App").file;
    let labels: Vec<String> = ide
        .complete(
            file,
            u32::try_from(cancel_src.len() - 1).expect("small"),
            false,
        )
        .into_iter()
        .map(|c| c.label)
        .collect();
    assert_eq!(labels, vec!["signal"]);

    let retention_src = "module App exposing (f)\n\
f : String ->{} String\n\
foreign f from \"./m.js\" \"f\"\n  retention \n";
    let (sources, analysis) = analyze_project(JS_TOML, retention_src, vec![]);
    let ide = Ide {
        analysis: &analysis,
        sources: &sources,
    };
    let file = analysis.module("App").expect("App").file;
    let labels: Vec<String> = ide
        .complete(
            file,
            u32::try_from(retention_src.len() - 1).expect("small"),
            false,
        )
        .into_iter()
        .map(|c| c.label)
        .collect();
    assert_eq!(labels, vec!["call", "owner"]);

    // `catch ` falls through to normal value completion (offers `my_mapper` in scope).
    let catch_src = "module App exposing (f, my_mapper)\n\
type error MyErr = MyErr String\n\
my_mapper : JsValue -> MyErr\n\
let my_mapper _ = MyErr \"x\"\n\
f : String ->{; MyErr} String\n\
foreign f from \"./m.js\" \"f\"\n  catch \n";
    let (sources, analysis) = analyze_project(JS_TOML, catch_src, vec![]);
    let ide = Ide {
        analysis: &analysis,
        sources: &sources,
    };
    let file = analysis.module("App").expect("App").file;
    let labels: Vec<String> = ide
        .complete(
            file,
            u32::try_from(catch_src.len() - 1).expect("small"),
            false,
        )
        .into_iter()
        .map(|c| c.label)
        .collect();
    assert!(
        labels.iter().any(|l| l == "my_mapper"),
        "`catch ` should offer functions in scope: {labels:?}"
    );
}

#[test]
fn attribute_completion_and_hover_follow_active_backend_type_form_attributes() {
    let attr_src = "module App exposing (Handle)\n\n@\ntype Handle\n";
    let at_offset = u32::try_from(attr_src.find('@').expect("@") + 1).expect("small");

    // JS offers `ts`, not `lit`.
    let (sources, analysis) = analyze_project(JS_TOML, attr_src, vec![]);
    let ide = Ide {
        analysis: &analysis,
        sources: &sources,
    };
    let file = analysis.module("App").expect("App").file;
    let js_attrs: Vec<String> = ide
        .complete(file, at_offset, false)
        .into_iter()
        .map(|c| c.label)
        .collect();
    assert!(js_attrs.iter().any(|l| l == "ts"), "{js_attrs:?}");
    assert!(!js_attrs.iter().any(|l| l == "lit"), "{js_attrs:?}");

    // Go offers neither `ts` nor `lit`.
    let (sources, analysis) = analyze_project(GO_TOML, attr_src, vec![]);
    let ide = Ide {
        analysis: &analysis,
        sources: &sources,
    };
    let file = analysis.module("App").expect("App").file;
    let go_attrs: Vec<String> = ide
        .complete(file, at_offset, false)
        .into_iter()
        .map(|c| c.label)
        .collect();
    assert!(!go_attrs.iter().any(|l| l == "ts"), "{go_attrs:?}");
    assert!(!go_attrs.iter().any(|l| l == "lit"), "{go_attrs:?}");
    assert!(go_attrs.iter().any(|l| l == "when"), "{go_attrs:?}");

    // Custom `test/literal` offers `lit`, not `ts`.
    let (sources, analysis) = analyze_project(LITERAL_TOML, attr_src, vec![literal_backend_tags()]);
    let ide = Ide {
        analysis: &analysis,
        sources: &sources,
    };
    let file = analysis.module("App").expect("App").file;
    let lit_attrs: Vec<String> = ide
        .complete(file, at_offset, false)
        .into_iter()
        .map(|c| c.label)
        .collect();
    assert!(lit_attrs.iter().any(|l| l == "lit"), "{lit_attrs:?}");
    assert!(!lit_attrs.iter().any(|l| l == "ts"), "{lit_attrs:?}");
}

#[test]
fn hover_on_foreign_metadata_and_type_form_attributes_reads_active_backend() {
    let lit_src = concat!(
        "module App exposing (Handle, f)\n",
        "@lit \"i32\"\n",
        "type Handle\n",
        "f : Handle ->{} Handle\n",
        "foreign f from \"./lit.js\" \"f\"\n",
        "  arity 1\n",
        "  owned 0\n",
        "  slot 0\n",
    );
    let (sources, analysis) = analyze_project(LITERAL_TOML, lit_src, vec![literal_backend_tags()]);
    assert!(
        !analysis
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error),
        "{:?}",
        analysis.diagnostics
    );
    let ide = Ide {
        analysis: &analysis,
        sources: &sources,
    };
    let file = analysis.module("App").expect("App").file;

    let slot_at = u32::try_from(lit_src.find("slot 0").expect("slot")).expect("small");
    let (slot_doc, slot_range) = ide.hover(file, slot_at + 1).expect("slot hover");
    assert!(
        slot_doc.contains("literal target calling slot"),
        "{slot_doc}"
    );
    assert_eq!(slot_range, (slot_at, slot_at + 4));

    let owned_at = u32::try_from(lit_src.find("owned 0").expect("owned")).expect("small");
    let (owned_doc, owned_range) = ide.hover(file, owned_at + 1).expect("owned hover");
    assert!(owned_doc.contains("ownership"), "{owned_doc}");
    assert_eq!(owned_range, (owned_at, owned_at + 5));

    let lit_at = u32::try_from(lit_src.find("@lit").expect("@lit")).expect("small");
    let (lit_doc, _) = ide.hover(file, lit_at + 2).expect("@lit hover");
    assert!(
        lit_doc.contains("Literal target type form annotation"),
        "{lit_doc}"
    );

    // `@ts` in a Go project is rejected with S014 and has no hover doc.
    let go_src = "module App exposing (Handle)\n@ts \"number\"\ntype Handle\n";
    let (go_sources, go_analysis) = analyze_project(GO_TOML, go_src, vec![]);
    assert!(
        go_analysis
            .diagnostics
            .iter()
            .any(|d| d.code == codes::S014),
        "{:?}",
        go_analysis.diagnostics
    );
    let go_ide = Ide {
        analysis: &go_analysis,
        sources: &go_sources,
    };
    let go_file = go_analysis.module("App").expect("App").file;
    let ts_at = u32::try_from(go_src.find("@ts").expect("@ts")).expect("small");
    assert!(go_ide.hover(go_file, ts_at + 1).is_none());
}

#[test]
fn compiled_ir_carries_core_owned_and_backend_adapter_words_with_spans() {
    let source = concat!(
        "module App exposing (f)\n",
        "f : String -> String ->{} Unit\n",
        "foreign f from \"./lit.js\" \"f\"\n",
        "  arity 2\n",
        "  owned 0\n",
        "  slot 0\n",
        "  slot 1\n",
    );
    let host = BackendHost::new(
        &[("flick.toml", LITERAL_TOML), ("src/App.fl", source)],
        vec![literal_backend_tags()],
    );
    let mut sources = SourceMap::default();
    let cfg_file = sources.add("flick.toml", LITERAL_TOML);
    let project = Project {
        root: PathBuf::from("/p"),
        config: config::parse(LITERAL_TOML, cfg_file).expect("config"),
        config_file: cfg_file,
        workspace: None,
    };
    let (diags, compilation) = project::compile(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/App.fl")],
    );
    assert!(
        !diags.iter().any(|d| d.severity == Severity::Error),
        "{diags:?}"
    );
    let module = compilation
        .modules
        .iter()
        .find(|m| m.name == "App")
        .expect("App");
    let foreign = module
        .ir
        .items
        .iter()
        .find_map(|item| match &item.kind {
            ItemKind::Foreign(f) => Some(f),
            _ => None,
        })
        .expect("foreign item");
    assert_eq!(foreign.adapter.owned, vec![0]);
    assert_eq!(foreign.adapter.words.len(), 2);
    assert_eq!(foreign.adapter.words[0].name, "slot");
    assert_eq!(foreign.adapter.words[0].args, vec![AdapterArg::Index(0)]);
    assert_eq!(&source[foreign.adapter.words[0].span.range()], "slot 0");
    assert_eq!(&source[foreign.adapter.words[1].span.range()], "slot 1");
}
