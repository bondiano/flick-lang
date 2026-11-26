//! Target metadata capture, invalid/conflicting external metadata validation,
//! and direct `target_in` variant/GADT/enum-literal tests.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use flick_plugin::{
    Arena, Block, Builder, Decl, DeriveTarget, Diagnostic as PluginDiagnostic, HeadSpec,
    LiteralValue, Plugin, PluginMetadata, Published, Ty, Visibility, WrittenKind, flick,
};

use super::super::{LIBRARY, show};
use super::{META_LIB, MetaHost, project_with, roots};
use crate::diagnostics::codes;
use crate::plugin::source::BlockSource;
use crate::plugin::{Expander, Failure};
use crate::project::load::{check, compile};
use crate::source::{FileId, SourceMap};

#[test]
#[allow(clippy::too_many_lines)]
fn derive_target_captures_docs_through_attrs_visibility_spans_and_error_decl_foreign() {
    struct Capture {
        seen: Mutex<Vec<DeriveTarget>>,
    }
    impl Plugin for Capture {
        fn collect(&self, _: &[Block], _: &mut dyn flick_plugin::Compiler) -> Vec<u8> {
            Vec::new()
        }
        fn expand(
            &self,
            b: &Block,
            _: &[u8],
            _: &mut dyn flick_plugin::Compiler,
        ) -> Result<Arena, Vec<PluginDiagnostic>> {
            Err(vec![PluginDiagnostic::error("unused", b.whole())])
        }
        fn derive(
            &self,
            target: &DeriveTarget,
            _: &str,
            _: Option<&Block>,
            _: &[DeriveTarget],
            _: &[u8],
            _: &mut dyn flick_plugin::Compiler,
        ) -> Result<Arena, Vec<PluginDiagnostic>> {
            self.seen.lock().expect("lock").push(target.clone());
            let mut b = Builder::new(target.decl.whole());
            let head = b.ty(Ty::Name(target.name.clone()));
            let cls = b.ty(Ty::Name("Meta.Codec".into()));
            let app = b.ty(Ty::Apply {
                function: cls,
                arguments: vec![head],
            });
            let enc = flick!(b, let encode _ = "");
            let dec = flick!(b, let decode _ = Err "x");
            let inst = b.decl(Decl::Instance {
                context: Vec::new(),
                head: app,
                members: vec![enc, dec],
            });
            Ok(b.finish(inst))
        }
    }

    struct CapExpander(Arc<Capture>);
    impl Expander for CapExpander {
        fn id(&self) -> &'static str {
            "cap"
        }
        fn collect(&self, _: &[Block], _: &mut BlockSource) -> Result<Vec<u8>, String> {
            Ok(Vec::new())
        }
        fn publish(&self, _: &[u8]) -> Result<Vec<Published>, String> {
            Ok(Vec::new())
        }
        fn expand(&self, b: &Block, _: &[u8], _: &mut BlockSource) -> Result<Arena, Failure> {
            Err(Failure::Refused(vec![PluginDiagnostic::error(
                "unused",
                b.whole(),
            )]))
        }
        fn derive(
            &self,
            target: &DeriveTarget,
            class: &str,
            args: Option<&Block>,
            module: &[DeriveTarget],
            state: &[u8],
            src: &mut BlockSource,
        ) -> Result<Arena, Failure> {
            self.0
                .derive(target, class, args, module, state, src)
                .map_err(Failure::Refused)
        }
    }

    let cap = Arc::new(Capture {
        seen: Mutex::new(Vec::new()),
    });

    let foreign_mod = r"module App.Errors exposing (NotFound(..))

/// Missing resource error
type error NotFound = NotFound String
";
    let main = r"module App.Main exposing (..)

import Meta
import App.Errors

/// Doc above attr
@label item
type Item =
  /// First variant
  First {
    /// Second alphabetically, first in source
    zebra : String;
    /// First alphabetically, second in source
    alpha : Int
  } deriving Meta.Codec

deriving instance Meta.Codec (App.Errors.NotFound)
";
    let mut plugins: BTreeMap<String, Arc<dyn Expander>> = BTreeMap::new();
    plugins.insert("meta".into(), Arc::new(CapExpander(Arc::clone(&cap))));
    let host = MetaHost {
        files: RefCell::new(BTreeMap::from([
            (PathBuf::from("/p/src/App/Main.fl"), main.to_owned()),
            (
                PathBuf::from("/p/src/App/Errors.fl"),
                foreign_mod.to_owned(),
            ),
            (PathBuf::from("/meta/flick.toml"), LIBRARY.to_owned()),
            (PathBuf::from("/meta/src/Meta.fl"), META_LIB.to_owned()),
        ])),
        plugins,
        store: RefCell::default(),
    };
    let mut sources = SourceMap::default();
    let project = project_with(&mut sources, &["meta"]);
    let (diags, _) = compile(
        &host,
        &mut sources,
        Some(&project),
        &[
            PathBuf::from("/p/src/App/Main.fl"),
            PathBuf::from("/p/src/App/Errors.fl"),
        ],
    );
    assert!(
        diags.is_empty(),
        "{:#?}",
        diags.iter().map(|d| show(&sources, d)).collect::<Vec<_>>()
    );
    let seen = cap.seen.lock().expect("lock");
    assert_eq!(seen.len(), 2);

    let item = &seen[0];
    assert_eq!(item.name, "Item");
    assert_eq!(item.decl.module, "App.Main");
    assert_eq!(item.decl.docs, "Doc above attr");
    assert_eq!(item.visibility, Visibility::PublicConstructors);
    assert_eq!(item.attributes.len(), 1);
    assert_eq!(item.attributes[0].name, "label");
    assert_eq!(item.attributes[0].args, "item");
    assert_eq!(item.decl.text_of(item.name_span), "Item");
    assert_eq!(item.deriving, ["Meta.Codec"]);
    assert_eq!(item.decl.text_of(item.deriving_spans[0]), "Meta.Codec");
    assert_eq!(item.cases.len(), 1);
    assert_eq!(item.cases[0].docs, "First variant");
    assert!(item.cases[0].attributes.is_empty());
    let WrittenKind::Record { fields, .. } = &item.cases[0].fields[0].kind else {
        panic!("expected record");
    };
    // Canonical UTF-16 order: alpha (source_index 1), zebra (source_index 0).
    assert_eq!(fields[0].name, "alpha");
    assert_eq!(fields[0].source_index, 1);
    assert_eq!(fields[0].docs, "First alphabetically, second in source");
    assert_eq!(fields[1].name, "zebra");
    assert_eq!(fields[1].source_index, 0);
    assert_eq!(fields[1].docs, "Second alphabetically, first in source");

    // Foreign ErrorDecl target!
    let err_target = &seen[1];
    assert_eq!(err_target.name, "App.Errors.NotFound");
    assert!(!err_target.local);
    assert_eq!(err_target.decl.module, "App.Errors");
    assert_eq!(err_target.decl.docs, "Missing resource error");
    assert_eq!(err_target.modifiers, ["type", "error"]);
    assert_eq!(err_target.visibility, Visibility::PublicConstructors);
}

#[test]
#[allow(clippy::too_many_lines)]
fn invalid_and_conflicting_external_plugin_metadata_is_diagnosed() {
    struct StaticMetaPlugin {
        id: &'static str,
        meta: PluginMetadata,
    }
    impl Expander for StaticMetaPlugin {
        fn id(&self) -> &str {
            self.id
        }
        fn metadata(&self) -> Result<PluginMetadata, String> {
            Ok(self.meta.clone())
        }
        fn collect(&self, _: &[Block], _: &mut BlockSource) -> Result<Vec<u8>, String> {
            Ok(Vec::new())
        }
        fn publish(&self, _: &[u8]) -> Result<Vec<Published>, String> {
            Ok(Vec::new())
        }
        fn expand(&self, b: &Block, _: &[u8], _: &mut BlockSource) -> Result<Arena, Failure> {
            Err(Failure::Refused(vec![PluginDiagnostic::error(
                "unused",
                b.whole(),
            )]))
        }
    }

    let main = "module App.Main exposing (x)\n\nx : Int\nlet x = 1\n";

    // Case 1: external plugin declaring own_codes + colliding std head + bad combo.
    let bad = Arc::new(StaticMetaPlugin {
        id: "bad",
        meta: PluginMetadata {
            heads: vec![
                HeadSpec {
                    head: "sql".into(),
                    module: "Bad".into(),
                    ..HeadSpec::default()
                },
                HeadSpec {
                    head: "clash".into(),
                    module: "Bad".into(),
                    top: true,
                    bare: true,
                    ..HeadSpec::default()
                },
            ],
            watch: vec!["invalid_watch".into()],
            derives: Vec::new(),
            own_codes: vec!["U001".into()],
        },
    });
    let host = MetaHost {
        files: RefCell::new(BTreeMap::from([
            (PathBuf::from("/p/src/App/Main.fl"), main.to_owned()),
            (PathBuf::from("/bad/flick.toml"), LIBRARY.to_owned()),
            (
                PathBuf::from("/bad/src/Bad.fl"),
                "module Bad exposing ()\n".to_owned(),
            ),
        ])),
        plugins: BTreeMap::from([("bad".into(), bad as Arc<dyn Expander>)]),
        store: RefCell::default(),
    };
    let mut sources = SourceMap::default();
    let project = project_with(&mut sources, &["bad"]);
    let diags = check(&host, &mut sources, Some(&project), &roots());
    assert!(
        diags
            .iter()
            .any(|d| d.code == codes::P036 && d.message.contains("own_codes")),
        "{diags:#?}"
    );
    assert!(
        diags
            .iter()
            .any(|d| d.code == codes::P044 && d.message.contains("`sql`")),
        "{diags:#?}"
    );
    assert!(
        diags
            .iter()
            .any(|d| d.code == codes::P036 && d.message.contains("contradictory")),
        "{diags:#?}"
    );

    // Case 2: two external plugins declaring conflicting syntax for the same head.
    let first = Arc::new(StaticMetaPlugin {
        id: "first",
        meta: PluginMetadata {
            heads: vec![HeadSpec {
                head: "shared_head".into(),
                module: "First".into(),
                bare: true,
                ..HeadSpec::default()
            }],
            ..PluginMetadata::default()
        },
    });
    let second = Arc::new(StaticMetaPlugin {
        id: "second",
        meta: PluginMetadata {
            heads: vec![HeadSpec {
                head: "shared_head".into(),
                module: "Second".into(),
                top: true,
                ..HeadSpec::default()
            }],
            ..PluginMetadata::default()
        },
    });
    let conflict_host = MetaHost {
        files: RefCell::new(BTreeMap::from([
            (PathBuf::from("/p/src/App/Main.fl"), main.to_owned()),
            (PathBuf::from("/first/flick.toml"), LIBRARY.to_owned()),
            (
                PathBuf::from("/first/src/First.fl"),
                "module First exposing ()\n".to_owned(),
            ),
            (PathBuf::from("/second/flick.toml"), LIBRARY.to_owned()),
            (
                PathBuf::from("/second/src/Second.fl"),
                "module Second exposing ()\n".to_owned(),
            ),
        ])),
        plugins: BTreeMap::from([
            ("first".into(), first as Arc<dyn Expander>),
            ("second".into(), second as Arc<dyn Expander>),
        ]),
        store: RefCell::default(),
    };
    let mut sources2 = SourceMap::default();
    let project2 = project_with(&mut sources2, &["first", "second"]);
    let diags2 = check(&conflict_host, &mut sources2, Some(&project2), &roots());
    assert!(
        diags2
            .iter()
            .any(|d| d.code == codes::P044 && d.message.contains("conflicting syntax")),
        "{diags2:#?}"
    );
}

#[test]
fn target_in_populates_variant_attributes_gadt_constraints_result_and_enum_literals() {
    use crate::plugin::derive::target_in;
    use crate::resolve::module_syntax;
    use crate::syntax::{kind::SyntaxKind as K, parse};

    let src: Arc<str> = r#"module App.Types exposing (Expr(..), HttpMethod(..))

type Expr a =
  /// Integer literal
  @lang some
  IntLit Int : Expr Int
  /// Existential showable
  | Show a => Packed a : Expr String

untagged type HttpMethod =
  /// Read
  Get = "GET"
  /// Negative error code
  | Fail = -404
"#
    .into();
    let file = FileId::default();
    let parsed = parse(&src, file);
    let syn = module_syntax(&parsed.tree, &src, file);
    let decls: Vec<_> = parsed
        .tree
        .nodes()
        .filter(|n| n.kind == K::TypeDecl)
        .collect();
    assert_eq!(decls.len(), 2);

    let expr_src = BlockSource::declaration(Arc::clone(&src), file, decls[0], Arc::default(), true)
        .with_module("App.Types");
    let expr_target = target_in(
        decls[0],
        Some(&parsed.tree),
        Some(&syn),
        &src,
        expr_src.block(),
        None,
    )
    .expect("Expr target");
    assert_eq!(expr_target.params, ["a"]);
    assert_eq!(expr_target.decl.text_of(expr_target.param_spans[0]), "a");
    assert_eq!(expr_target.cases[0].docs, "Integer literal");
    assert_eq!(expr_target.cases[0].attributes.len(), 1);
    assert_eq!(expr_target.cases[0].attributes[0].name, "lang");
    assert_eq!(expr_target.cases[0].attributes[0].args, "some");
    assert!(expr_target.cases[0].refined);
    assert!(expr_target.cases[0].result.is_some());
    assert!(expr_target.cases[1].constrained);
    assert_eq!(expr_target.cases[1].constraints.len(), 1);
    assert_eq!(expr_target.cases[1].constraints[0].class, "Show");

    let method_src =
        BlockSource::declaration(Arc::clone(&src), file, decls[1], Arc::default(), true)
            .with_module("App.Types");
    let method_target = target_in(
        decls[1],
        Some(&parsed.tree),
        Some(&syn),
        &src,
        method_src.block(),
        None,
    )
    .expect("HttpMethod target");
    assert_eq!(
        method_target.cases[0].literal_value,
        Some(LiteralValue::String("GET".into()))
    );
    assert_eq!(
        method_target.cases[1].literal_value,
        Some(LiteralValue::Int("-404".into()))
    );
}
