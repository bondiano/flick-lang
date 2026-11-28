//! Plugin-declared `@attribute` validation, collisions, cache invalidation, and hover tests.

#[cfg(feature = "ide")]
#[test]
#[allow(clippy::too_many_lines)]
fn plugin_declared_attributes_validate_cache_and_hover_end_to_end() {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::atomic::Ordering;
    use std::sync::{Arc, Mutex};

    use flick_plugin::{
        Arena, AttributeSpec, AttributeTarget, Block, Diagnostic as PluginDiagnostic,
        PluginMetadata, Published,
    };

    use super::{LIBRARY, MetaHost, project_with, roots, show};
    use crate::diagnostics::codes;
    use crate::ide::Ide;
    use crate::plugin::source::BlockSource;
    use crate::plugin::{Expander, Failure};
    use crate::project::load::{ModuleCache, analyze_with, check};
    use crate::source::SourceMap;

    struct SyncAttrPlugin {
        id: &'static str,
        attrs: Mutex<Vec<AttributeSpec>>,
    }
    impl Expander for SyncAttrPlugin {
        fn id(&self) -> &str {
            self.id
        }
        fn metadata(&self) -> Result<PluginMetadata, String> {
            Ok(PluginMetadata {
                attributes: self.attrs.lock().expect("lock").clone(),
                ..PluginMetadata::default()
            })
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

    // 1. Valid declaration (`@meta.prefix`), variant (`@alias`), and record-field
    //    (`@rename`, `@skip`, `@default`) attributes pass checking, populate IDE
    //    hover, and keep field hover showing the field's type.
    let valid = r#"module App.Main exposing (User(..), Cmd(..), encoded)

import Meta

@meta.prefix "usr"
type User = User {
  /// Primary handle
  @rename "user_id"
  userName : String
  @skip
  @default "local"
  internalTag : String
  @default -1
  retries : Int
} deriving Meta.Codec

type Cmd =
  @alias "s"
  Serve {
    @rename "bind-port"
    port : Int
  }
  | Status
  deriving Meta.CliForm

encoded : User -> String
let encoded u = Meta.encode u
"#;
    let (host, plugin) = MetaHost::with_meta(valid);
    let mut cache = ModuleCache::default();
    let mut sources = SourceMap::default();
    let project = project_with(&mut sources, &["meta"]);
    let analysis = analyze_with(&host, &mut sources, Some(&project), &roots(), &mut cache);
    assert!(
        analysis.diagnostics.is_empty(),
        "{:#?}",
        analysis
            .diagnostics
            .iter()
            .map(|d| show(&sources, d))
            .collect::<Vec<_>>()
    );
    assert_eq!(plugin.derives.load(Ordering::Relaxed), 2);

    // IDE hover on plain field attribute `@rename`, namespaced `@meta.prefix`,
    // and attributed field `userName` (must show `String`, not `@rename`).
    let file = analysis.module("App.Main").expect("App.Main").file;
    let ide = Ide {
        analysis: &analysis,
        sources: &sources,
    };
    let prefix_off = u32::try_from(valid.find("meta.prefix").expect("prefix offset")).expect("u32");
    let (prefix_md, prefix_range) = ide
        .hover(file, prefix_off)
        .expect("hover on namespaced @meta.prefix");
    assert!(prefix_md.contains("@meta.prefix \"usr\""), "{prefix_md}");
    assert!(prefix_md.contains("Namespaced prefix"), "{prefix_md}");
    assert!(prefix_md.contains("`meta`"), "{prefix_md}");
    assert!(prefix_md.contains("a declaration"), "{prefix_md}");
    assert_eq!(
        &valid[prefix_range.0 as usize..prefix_range.1 as usize],
        "@meta.prefix"
    );

    let rename_off = u32::try_from(valid.find("@rename").expect("rename offset")).expect("u32");
    let (rename_md, rename_range) = ide.hover(file, rename_off + 2).expect("hover on @rename");
    assert!(rename_md.contains("@rename \"user_id\""), "{rename_md}");
    assert!(rename_md.contains("a record field"), "{rename_md}");
    assert_eq!(
        &valid[rename_range.0 as usize..rename_range.1 as usize],
        "@rename"
    );

    let field_off =
        u32::try_from(valid.find("userName : String").expect("field offset")).expect("u32");
    let (field_md, _) = ide.hover(file, field_off).expect("hover on userName");
    assert!(
        field_md.contains("String") && !field_md.contains("@rename"),
        "expected field type String, got:\n{field_md}"
    );

    // 2. Source-equivalent edit outside declarations hits derive cache; editing a
    //    field, variant, or declaration attribute invalidates it.
    host.write("/p/src/App/Main.fl", &format!("{valid}\n// note\n"));
    let mut s_hit = SourceMap::default();
    let p_hit = project_with(&mut s_hit, &["meta"]);
    let a_hit = analyze_with(&host, &mut s_hit, Some(&p_hit), &roots(), &mut cache);
    assert!(a_hit.diagnostics.is_empty());
    assert_eq!(plugin.derives.load(Ordering::Relaxed), 2);

    let edited_field = valid.replace("@default -1", "@default 7");
    host.write("/p/src/App/Main.fl", &edited_field);
    let mut s_field = SourceMap::default();
    let p_field = project_with(&mut s_field, &["meta"]);
    let a_field = analyze_with(&host, &mut s_field, Some(&p_field), &roots(), &mut cache);
    assert!(a_field.diagnostics.is_empty());
    assert_eq!(plugin.derives.load(Ordering::Relaxed), 4);

    let edited_variant = edited_field.replace("@alias \"s\"", "@alias \"srv\"");
    host.write("/p/src/App/Main.fl", &edited_variant);
    let mut s_var = SourceMap::default();
    let p_var = project_with(&mut s_var, &["meta"]);
    let a_var = analyze_with(&host, &mut s_var, Some(&p_var), &roots(), &mut cache);
    assert!(a_var.diagnostics.is_empty());
    assert_eq!(plugin.derives.load(Ordering::Relaxed), 6);

    let edited_decl = edited_variant.replace("@meta.prefix \"usr\"", "@meta.prefix \"acct\"");
    host.write("/p/src/App/Main.fl", &edited_decl);
    let mut s_decl = SourceMap::default();
    let p_decl = project_with(&mut s_decl, &["meta"]);
    let a_decl = analyze_with(&host, &mut s_decl, Some(&p_decl), &roots(), &mut cache);
    assert!(a_decl.diagnostics.is_empty());
    assert_eq!(plugin.derives.load(Ordering::Relaxed), 8);

    // 3. Misplaced and unknown attributes report FLICK-S014 at exact spans.
    let bad = r#"module App.Main exposing (Bad(..))

@rename "wrong_place"
type Bad =
  @tag "wrong_variant"
  Bad {
    @alias "wrong_field"
    @ghost
    x : Int
  }
"#;
    let (bad_host, _) = MetaHost::with_meta(bad);
    let mut bad_sources = SourceMap::default();
    let bad_project = project_with(&mut bad_sources, &["meta"]);
    let diags = check(&bad_host, &mut bad_sources, Some(&bad_project), &roots());
    let rendered: Vec<String> = diags.iter().map(|d| show(&bad_sources, d)).collect();
    assert_eq!(diags.len(), 4, "{rendered:#?}");
    assert!(diags.iter().all(|d| d.code == codes::S014), "{rendered:#?}");
    assert!(
        diags
            .iter()
            .any(|d| d.message.contains("`@rename` belongs on a record field")),
        "{rendered:#?}"
    );
    assert!(
        diags
            .iter()
            .any(|d| d.message.contains("`@tag` belongs on a declaration")),
        "{rendered:#?}"
    );
    assert!(
        diags
            .iter()
            .any(|d| d.message.contains("`@alias` belongs on a variant")),
        "{rendered:#?}"
    );
    assert!(
        diags
            .iter()
            .any(|d| d.message.contains("unknown attribute `@ghost`")
                && d.message.contains("`@rename`")),
        "{rendered:#?}"
    );

    // 4. Cross-plugin attribute collision & backend collision (`@ts`) report FLICK-P044;
    //    changing a plugin's declared attributes invalidates ModuleCache!
    let p1 = Arc::new(SyncAttrPlugin {
        id: "p1",
        attrs: Mutex::new(vec![
            AttributeSpec {
                name: "dup".into(),
                targets: vec![AttributeTarget::Declaration],
                doc: String::new(),
            },
            AttributeSpec {
                name: "foo.bar".into(),
                targets: vec![AttributeTarget::Declaration],
                doc: String::new(),
            },
            AttributeSpec {
                name: "p2.baz".into(),
                targets: vec![AttributeTarget::Declaration],
                doc: String::new(),
            },
            AttributeSpec {
                name: "my_pkg.qux".into(),
                targets: vec![AttributeTarget::Declaration],
                doc: String::new(),
            },
        ]),
    });
    let p2 = Arc::new(SyncAttrPlugin {
        id: "p2",
        attrs: Mutex::new(vec![
            AttributeSpec {
                name: "dup".into(),
                targets: vec![AttributeTarget::Declaration],
                doc: String::new(),
            },
            AttributeSpec {
                name: "foo.bar".into(),
                targets: vec![AttributeTarget::Declaration],
                doc: String::new(),
            },
            AttributeSpec {
                name: "p2.baz".into(),
                targets: vec![AttributeTarget::Declaration],
                doc: String::new(),
            },
            AttributeSpec {
                name: "ts".into(),
                targets: vec![AttributeTarget::Declaration],
                doc: String::new(),
            },
        ]),
    });
    let my_pkg = Arc::new(SyncAttrPlugin {
        id: "my-pkg",
        attrs: Mutex::new(vec![AttributeSpec {
            name: "my_pkg.qux".into(),
            targets: vec![AttributeTarget::Declaration],
            doc: String::new(),
        }]),
    });
    let simple_src = "module App.Main exposing (A(..))\n\n@dup\ntype A = A\n";
    let collision_host = MetaHost {
        files: RefCell::new(BTreeMap::from([
            (PathBuf::from("/p/src/App/Main.fl"), simple_src.to_owned()),
            (PathBuf::from("/p1/flick.toml"), LIBRARY.to_owned()),
            (
                PathBuf::from("/p1/src/P1.fl"),
                "module P1 exposing ()\n".to_owned(),
            ),
            (PathBuf::from("/p2/flick.toml"), LIBRARY.to_owned()),
            (
                PathBuf::from("/p2/src/P2.fl"),
                "module P2 exposing ()\n".to_owned(),
            ),
            (PathBuf::from("/my-pkg/flick.toml"), LIBRARY.to_owned()),
            (
                PathBuf::from("/my-pkg/src/MyPkg.fl"),
                "module MyPkg exposing ()\n".to_owned(),
            ),
        ])),
        plugins: BTreeMap::from([
            ("p1".into(), Arc::clone(&p1) as Arc<dyn Expander>),
            ("p2".into(), Arc::clone(&p2) as Arc<dyn Expander>),
            ("my-pkg".into(), Arc::clone(&my_pkg) as Arc<dyn Expander>),
        ]),
        store: RefCell::default(),
    };
    let mut s_col = SourceMap::default();
    let proj_col = project_with(&mut s_col, &["p1", "p2", "my-pkg"]);
    let col_diags = check(&collision_host, &mut s_col, Some(&proj_col), &roots());
    assert!(
        col_diags.iter().any(|d| d.code == codes::P044
            && d.message.contains("both declare attribute `@dup`")
            && d.message.contains("`@p2.dup`")),
        "{col_diags:#?}"
    );
    assert!(
        col_diags.iter().any(|d| d.code == codes::P044
            && d.message.contains("both declare attribute `@foo.bar`")
            && d.message.contains("`@p2.bar`")
            && !d.message.contains("`@p2.foo.bar`")),
        "{col_diags:#?}"
    );
    assert!(
        col_diags.iter().any(|d| d.code == codes::P044
            && d.message.contains("both declare attribute `@p2.baz`")
            && d.message.contains("rename one to avoid the collision")
            && !d.message.contains("`@p2.p2.baz`")),
        "{col_diags:#?}"
    );
    assert!(
        col_diags.iter().any(|d| d.code == codes::P044
            && d.message.contains("both declare attribute `@my_pkg.qux`")
            && d.message.contains("rename one to avoid the collision")
            && !d.message.contains("namespace one as `@my_pkg.qux`")),
        "{col_diags:#?}"
    );
    assert!(
        col_diags
            .iter()
            .any(|d| d.code == codes::P044 && d.message.contains("`@ts`")),
        "{col_diags:#?}"
    );

    // ModuleCache invalidation when plugin removes `@dup`.
    let single_host = MetaHost {
        files: RefCell::new(BTreeMap::from([
            (PathBuf::from("/p/src/App/Main.fl"), simple_src.to_owned()),
            (PathBuf::from("/p1/flick.toml"), LIBRARY.to_owned()),
            (
                PathBuf::from("/p1/src/P1.fl"),
                "module P1 exposing ()\n".to_owned(),
            ),
        ])),
        plugins: BTreeMap::from([("p1".into(), Arc::clone(&p1) as Arc<dyn Expander>)]),
        store: RefCell::default(),
    };
    let mut mod_cache = ModuleCache::default();
    mod_cache.explain(true);
    let mut s1 = SourceMap::default();
    let pr1 = project_with(&mut s1, &["p1"]);
    let a1 = analyze_with(&single_host, &mut s1, Some(&pr1), &roots(), &mut mod_cache);
    assert!(a1.diagnostics.is_empty(), "{:#?}", a1.diagnostics);

    // Simulate a rebuilt plugin dropping `@dup` (clearing the metadata cache entry).
    p1.attrs.lock().expect("lock").clear();
    single_host.store.borrow_mut().clear();
    let mut s2 = SourceMap::default();
    let pr2 = project_with(&mut s2, &["p1"]);
    let a2 = analyze_with(&single_host, &mut s2, Some(&pr2), &roots(), &mut mod_cache);
    assert!(
        a2.diagnostics
            .iter()
            .any(|d| d.code == codes::S014 && d.message.contains("`@dup`")),
        "expected cache invalidation + S014 after plugin dropped @dup: {:#?}",
        a2.diagnostics
    );
    assert!(
        mod_cache
            .reasons()
            .iter()
            .any(|(m, r)| m == "App.Main" && r.contains("plugin inputs")),
        "expected ModuleCache invalidation reason `plugin inputs`, got {:?}",
        mod_cache.reasons()
    );
}
