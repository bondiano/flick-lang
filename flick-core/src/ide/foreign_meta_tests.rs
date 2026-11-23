//! IDE completion and hover for core foreign contract words, backend-declared
//! adapter words, and backend-declared type-form attributes.

use std::collections::BTreeMap;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use super::Ide;
use crate::project::config;
use crate::project::{self, Analysis, BackendTags, Host, Project};
use crate::source::{FileId, SourceMap};
use crate::types::{AdapterArgKind, AdapterArgSpec, AdapterWordSpec, TypeFormAttrSpec};

struct BackendHost {
    files: BTreeMap<PathBuf, String>,
    extra_backends: Vec<BackendTags>,
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

    fn backend_tags(&self, _project: &Project) -> Vec<BackendTags> {
        self.extra_backends.clone()
    }
}

struct BackendFixture {
    analysis: Analysis,
    sources: SourceMap,
}

impl BackendFixture {
    fn ide(&self) -> Ide<'_> {
        Ide {
            analysis: &self.analysis,
            sources: &self.sources,
        }
    }

    fn file(&self, module: &str) -> FileId {
        self.analysis.module(module).expect("module").file
    }

    fn offset_after(&self, module: &str, needle: &str) -> u32 {
        let text = &self.sources.get(self.file(module)).text;
        let found = text
            .find(needle)
            .unwrap_or_else(|| panic!("missing `{needle}` in:\n{text}"));
        u32::try_from(found + needle.len()).expect("small")
    }

    fn offset_of(&self, module: &str, needle: &str) -> u32 {
        let text = &self.sources.get(self.file(module)).text;
        let found = text
            .find(needle)
            .unwrap_or_else(|| panic!("missing `{needle}` in:\n{text}"));
        u32::try_from(found).expect("small")
    }
}

fn fixture_for(toml: &str, source: &str, extra_backends: Vec<BackendTags>) -> BackendFixture {
    let mut files = BTreeMap::new();
    files.insert(PathBuf::from("/p/src/App.fl"), source.to_owned());
    let host = BackendHost {
        files,
        extra_backends,
    };
    let mut sources = SourceMap::default();
    let config_file = sources.add("flick.toml", toml);
    let config = config::parse(toml, config_file).expect("valid config");
    let project = Project {
        root: PathBuf::from("/p"),
        config_file,
        workspace: None,
        config,
    };
    let roots = [PathBuf::from("/p/src/App.fl")];
    let analysis = project::analyze(&host, &mut sources, Some(&project), &roots);
    BackendFixture { analysis, sources }
}

fn literal_backend_tags() -> BackendTags {
    BackendTags {
        id: "test/literal".into(),
        short: "literal".into(),
        tags: vec!["literal".into()],
        axes: Vec::new(),
        environments: vec![("js".into(), "node".into())],
        foreign_adapters: vec![
            AdapterWordSpec {
                name: "callback".into(),
                args: vec![
                    AdapterArgSpec {
                        name: "fn".into(),
                        kind: AdapterArgKind::ParamIndex,
                    },
                    AdapterArgSpec {
                        name: "data".into(),
                        kind: AdapterArgKind::Keyword(vec!["data".into()]),
                    },
                    AdapterArgSpec {
                        name: "user_data".into(),
                        kind: AdapterArgKind::ParamIndex,
                    },
                ],
                repeatable: true,
                doc: "Pair callback parameter `fn` with its opaque userdata parameter `user_data`."
                    .into(),
            },
            AdapterWordSpec {
                name: "method".into(),
                args: Vec::new(),
                repeatable: false,
                doc: "Call the foreign binding as a method on its first argument.".into(),
            },
        ],
        type_form_attributes: vec![TypeFormAttrSpec {
            name: "literal".into(),
            doc: "Declare the literal-backend foreign type form.".into(),
        }],
    }
}

fn literal_toml() -> String {
    format!(
        "schema-version = 1\n[project]\nkind = \"lib\"\n[build.backend]\nid = \"test/literal\"\nversion = \"1.0.0\"\nsha256 = \"{}\"\ncomponent = \"backend.wasm\"\n",
        "a".repeat(64)
    )
}

fn sorted_labels(ide: &Ide<'_>, file: FileId, offset: u32) -> Vec<String> {
    let mut labels: Vec<String> = ide
        .complete(file, offset, false)
        .into_iter()
        .map(|item| item.label)
        .collect();
    labels.sort();
    labels
}

#[test]
fn completion_offers_only_core_and_js_adapter_words_for_js_backend() {
    let source = concat!(
        "module App exposing (f, g, h)\n",
        "\n",
        "f : Int ->{Async} Int\n",
        "foreign f from \"./m.js\" \"f\"\n",
        "  arity 1\n",
        "  \n",
        "\n",
        "g : Int ->{Async} Int\n",
        "foreign g from \"./m.js\" \"g\"\n",
        "  arity 1\n",
        "  cancel \n",
        "\n",
        "h : Int ->{} Int\n",
        "foreign h from \"./m.js\" \"h\"\n",
        "  arity 1\n",
        "  retention \n",
    );
    let f = fixture_for(
        "schema-version = 1\n[project]\nkind = \"lib\"\n",
        source,
        vec![],
    );
    let ide = f.ide();
    let file = f.file("App");

    let word_pos = f.offset_after("App", "foreign f from \"./m.js\" \"f\"\n  arity 1\n  ");
    assert_eq!(
        sorted_labels(&ide, file, word_pos),
        vec![
            "arity",
            "cancel",
            "catch",
            "nullable",
            "optional",
            "owned",
            "retention",
            "reviewed",
            "this",
            "variadic",
        ]
    );

    let cancel_arg_pos = f.offset_after("App", "  cancel ");
    assert_eq!(sorted_labels(&ide, file, cancel_arg_pos), vec!["signal"]);

    let retention_arg_pos = f.offset_after("App", "  retention ");
    assert_eq!(
        sorted_labels(&ide, file, retention_arg_pos),
        vec!["call", "owner"]
    );
}

#[test]
fn completion_offers_go_adapter_words_and_cancel_choices_for_go_backend() {
    let source = concat!(
        "module App exposing (f, g)\n",
        "\n",
        "f : Int ->{Async} Int\n",
        "foreign f from \"./m.mjs\" \"f\"\n",
        "  arity 1\n",
        "  \n",
        "\n",
        "g : Int ->{Async} Int\n",
        "foreign g from \"./m.mjs\" \"g\"\n",
        "  arity 1\n",
        "  cancel \n",
    );
    let f = fixture_for(
        "schema-version = 1\n[project]\nkind = \"lib\"\n[build]\ntarget = \"go\"\n",
        source,
        vec![],
    );
    let ide = f.ide();
    let file = f.file("App");

    let word_pos = f.offset_after("App", "foreign f from \"./m.mjs\" \"f\"\n  arity 1\n  ");
    assert_eq!(
        sorted_labels(&ide, file, word_pos),
        vec![
            "arity",
            "cancel",
            "catch",
            "nullable",
            "optional",
            "owned",
            "retention",
            "reviewed",
            "this",
            "variadic",
        ]
    );

    let cancel_arg_pos = f.offset_after("App", "  cancel ");
    assert_eq!(
        sorted_labels(&ide, file, cancel_arg_pos),
        vec!["context", "signal"]
    );
}

#[test]
fn completion_offers_only_active_custom_backend_adapter_words() {
    let source = concat!(
        "module App exposing (reg, reg2)\n",
        "\n",
        "reg : (Int -> Unit) -> Int ->{} Unit\n",
        "foreign reg from \"./native.c\" \"reg\"\n",
        "  arity 2\n",
        "  \n",
        "\n",
        "reg2 : (Int -> Unit) -> Int ->{} Unit\n",
        "foreign reg2 from \"./native.c\" \"reg2\"\n",
        "  arity 2\n",
        "  callback 0 \n",
    );
    let f = fixture_for(&literal_toml(), source, vec![literal_backend_tags()]);
    let ide = f.ide();
    let file = f.file("App");

    let word_pos = f.offset_after(
        "App",
        "foreign reg from \"./native.c\" \"reg\"\n  arity 2\n  ",
    );
    let labels = sorted_labels(&ide, file, word_pos);
    assert_eq!(
        labels,
        vec![
            "arity",
            "callback",
            "catch",
            "method",
            "nullable",
            "optional",
            "owned",
            "retention",
            "reviewed",
        ]
    );
    for forbidden in ["this", "cancel", "variadic", "reg", "show"] {
        assert!(
            !labels.iter().any(|l| l == forbidden),
            "unexpected `{forbidden}` in custom backend completion: {labels:?}"
        );
    }

    let kw_pos = f.offset_after("App", "  callback 0 ");
    assert_eq!(sorted_labels(&ide, file, kw_pos), vec!["data"]);
}

#[test]
fn hover_reflects_active_backend_adapter_words_and_type_form_attributes() {
    let js_source = "\
module App exposing (Handle, invoke)

@ts \"MyHandle\"
type Handle

invoke : Handle -> Int ->{} Unit
foreign invoke from \"./m.js\" \"invoke\"
  arity 2
  this
";
    let js = fixture_for(
        "schema-version = 1\n[project]\nkind = \"lib\"\n",
        js_source,
        vec![],
    );
    let js_ide = js.ide();
    let js_file = js.file("App");
    let arity_hover = js_ide
        .hover(js_file, js.offset_of("App", "arity") + 1)
        .expect("core hover")
        .0;
    assert!(arity_hover.contains("arity <n>"), "{arity_hover}");
    let this_hover = js_ide
        .hover(js_file, js.offset_of("App", "this") + 1)
        .expect("js adapter hover")
        .0;
    assert!(this_hover.contains("flick/js"), "{this_hover}");
    let ts_hover = js_ide
        .hover(js_file, js.offset_of("App", "@ts") + 1)
        .expect("@ts hover")
        .0;
    assert!(ts_hover.contains("TypeScript"), "{ts_hover}");

    let literal_source = "\
module App exposing (Handle, reg, bad)

@literal \"MyHandle\"
type Handle

@ts \"Other\"
type Other

reg : (Int -> Unit) -> Handle ->{} Unit
foreign reg from \"./native.c\" \"reg\"
  arity 2
  callback 0 data 1
  method

bad : Handle ->{} Unit
foreign bad from \"./native.c\" \"bad\"
  arity 1
  this
";
    let lit = fixture_for(
        &literal_toml(),
        literal_source,
        vec![literal_backend_tags()],
    );
    let lit_ide = lit.ide();
    let lit_file = lit.file("App");
    let cb_hover = lit_ide
        .hover(lit_file, lit.offset_of("App", "callback") + 1)
        .expect("callback hover")
        .0;
    assert!(
        cb_hover.contains("callback <fn> data <user_data>") && cb_hover.contains("test/literal"),
        "{cb_hover}"
    );
    let method_hover = lit_ide
        .hover(lit_file, lit.offset_of("App", "method") + 1)
        .expect("method hover")
        .0;
    assert!(method_hover.contains("test/literal"), "{method_hover}");
    let lit_attr_hover = lit_ide
        .hover(lit_file, lit.offset_of("App", "@literal") + 1)
        .expect("@literal hover")
        .0;
    assert!(lit_attr_hover.contains("test/literal"), "{lit_attr_hover}");
    assert!(
        lit_ide
            .hover(lit_file, lit.offset_of("App", "@ts") + 1)
            .is_none(),
        "@ts must not hover under test/literal"
    );
    assert!(
        lit_ide
            .hover(lit_file, lit.offset_of("App", "  this") + 3)
            .is_none(),
        "`this` must not hover under test/literal"
    );
}
