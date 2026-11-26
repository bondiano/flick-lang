//! Declaration metadata, external plugin-declared `heads`/`watch`,
//! metadata validation, cache hit/invalidation, and native/IDE agreement.

mod cases;

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::sync::Arc;
#[cfg(feature = "ide")]
use std::sync::atomic::{AtomicUsize, Ordering};

use flick_plugin::{
    Arena, Block, CallSite, DeriveTarget, Diagnostic as PluginDiagnostic, PluginMetadata, Published,
};
use flick_test_meta::Meta;

use super::{LIBRARY, show};
use crate::plugin::source::BlockSource;
use crate::plugin::{Expander, Failure, Native};
use crate::project::config::{Build, Config, Dependency, Kind, Source};
use crate::project::load::check;
#[cfg(feature = "ide")]
use crate::project::load::{ModuleCache, analyze, analyze_with};
use crate::project::{Host, Project};
use crate::source::{FileId, SourceMap, Span};
use crate::types::Thresholds;

const META_LIB: &str = r#"module Meta exposing (..)

@derive
class Codec a =
  encode : a -> String
  decode : String -> Result String a

@derive
class CliForm a =
  help_text : Option a -> String
  parse_form : List String -> Result String a

guarded : String -> String
let guarded s = s

join_kv : List String -> String
let join_kv parts = String.join ";" parts

encode_int : Int -> String
let encode_int n = show n

encode_bool : Bool -> String
let encode_bool b = if b then "true" else "false"

encode_opt_string : Option String -> String
let encode_opt_string opt =
  match opt with
    | Some s -> s
    | None -> ""

strip_tag : String -> String -> Result String String
let strip_tag tag raw =
  let prefix = tag ++ ":"
  if String.starts_with prefix raw then
    Ok (String.slice (String.length prefix) (String.length raw) raw)
  else
    Err ("expected tag " ++ tag)

kv_get : String -> String -> Option String
let kv_get key raw =
  let prefix = key ++ "="
  let parts = String.split ";" raw
  match List.find (fun p -> String.starts_with prefix p) parts with
    | Some entry -> Some (String.slice (String.length prefix) (String.length entry) entry)
    | None -> None

decode_string : String -> String -> Result String String
let decode_string key raw =
  match kv_get key raw with
    | Some v -> Ok v
    | None -> Err ("missing " ++ key)

decode_string_default : String -> String -> String -> Result String String
let decode_string_default key fallback raw =
  match kv_get key raw with
    | Some v -> Ok v
    | None -> Ok fallback

decode_opt_string : String -> String -> Result String (Option String)
let decode_opt_string key raw =
  match kv_get key raw with
    | Some "" -> Ok None
    | Some v -> Ok (Some v)
    | None -> Ok None

decode_int : String -> String -> Result String Int
let decode_int key raw =
  match kv_get key raw with
    | Some v ->
      match Int.parse v with
        | Some n -> Ok n
        | None -> Err ("bad int for " ++ key)
    | None -> Err ("missing " ++ key)

decode_int_default : String -> Int -> String -> Result String Int
let decode_int_default key fallback raw =
  match kv_get key raw with
    | Some v ->
      match Int.parse v with
        | Some n -> Ok n
        | None -> Err ("bad int for " ++ key)
    | None -> Ok fallback

decode_bool : String -> String -> Result String Bool
let decode_bool key raw =
  match kv_get key raw with
    | Some "true" -> Ok true
    | Some "false" -> Ok false
    | Some _ -> Err ("bad bool for " ++ key)
    | None -> Err ("missing " ++ key)

decode_bool_default : String -> Bool -> String -> Result String Bool
let decode_bool_default key fallback raw =
  match kv_get key raw with
    | Some "true" -> Ok true
    | Some "false" -> Ok false
    | Some _ -> Err ("bad bool for " ++ key)
    | None -> Ok fallback

bind_res : Result String a -> (a -> Result String b) -> Result String b
let bind_res r f =
  match r with
    | Ok v -> f v
    | Err e -> Err e

flag_lookup : String -> List String -> Option String
let rec flag_lookup flag args =
  match List.head args with
    | Some a ->
      let rest = match List.tail args with | Some t -> t | None -> []
      if a == flag then List.head rest else flag_lookup flag rest
    | None -> None

flag_string : String -> List String -> Result String String
let flag_string flag args =
  match flag_lookup flag args with
    | Some v -> Ok v
    | None -> Err ("missing " ++ flag)

flag_opt_string : String -> List String -> Result String (Option String)
let flag_opt_string flag args = Ok (flag_lookup flag args)

flag_int : String -> List String -> Result String Int
let flag_int flag args =
  match flag_lookup flag args with
    | Some v ->
      match Int.parse v with
        | Some n -> Ok n
        | None -> Err ("bad int for " ++ flag)
    | None -> Err ("missing " ++ flag)

flag_bool : String -> List String -> Result String Bool
let flag_bool flag args =
  match flag_lookup flag args with
    | Some "true" -> Ok true
    | Some "false" -> Ok false
    | Some _ -> Err ("bad bool for " ++ flag)
    | None -> Ok false

cmd_head : List String -> Option (String, List String)
let cmd_head args =
  match List.head args with
    | Some h ->
      let rest = match List.tail args with | Some t -> t | None -> []
      Some (h, rest)
    | None -> None

matches_cmd : String -> List String -> Bool
let matches_cmd cmd names = List.any (fun n -> n == cmd) names
"#;

struct CountingMeta {
    inner: Native<Meta>,
    #[cfg(feature = "ide")]
    derives: AtomicUsize,
}

impl CountingMeta {
    fn new() -> Self {
        Self {
            inner: Native::new("meta", Meta),
            #[cfg(feature = "ide")]
            derives: AtomicUsize::new(0),
        }
    }
}

impl Expander for CountingMeta {
    fn id(&self) -> &str {
        self.inner.id()
    }

    fn metadata(&self) -> Result<PluginMetadata, String> {
        self.inner.metadata()
    }

    fn collect(&self, blocks: &[Block], source: &mut BlockSource) -> Result<Vec<u8>, String> {
        self.inner.collect(blocks, source)
    }

    fn publish(&self, state: &[u8]) -> Result<Vec<Published>, String> {
        self.inner.publish(state)
    }

    fn expand(
        &self,
        block: &Block,
        state: &[u8],
        source: &mut BlockSource,
    ) -> Result<Arena, Failure> {
        self.inner.expand(block, state, source)
    }

    fn derive(
        &self,
        target: &DeriveTarget,
        class: &str,
        args: Option<&Block>,
        module: &[DeriveTarget],
        state: &[u8],
        source: &mut BlockSource,
    ) -> Result<Arena, Failure> {
        #[cfg(feature = "ide")]
        self.derives.fetch_add(1, Ordering::Relaxed);
        self.inner
            .derive(target, class, args, module, state, source)
    }

    fn check_calls(
        &self,
        calls: &[CallSite],
        state: &[u8],
    ) -> Result<Vec<PluginDiagnostic>, String> {
        self.inner.check_calls(calls, state)
    }
}

struct MetaHost {
    files: RefCell<BTreeMap<PathBuf, String>>,
    plugins: BTreeMap<String, Arc<dyn Expander>>,
    store: RefCell<HashMap<Vec<u8>, Vec<u8>>>,
}

impl MetaHost {
    fn with_meta(main: &str) -> (Self, Arc<CountingMeta>) {
        let plugin = Arc::new(CountingMeta::new());
        let mut plugins: BTreeMap<String, Arc<dyn Expander>> = BTreeMap::new();
        plugins.insert("meta".into(), plugin.clone());
        let host = Self {
            files: RefCell::new(BTreeMap::from([
                (PathBuf::from("/p/src/App/Main.fl"), main.to_owned()),
                (PathBuf::from("/meta/flick.toml"), LIBRARY.to_owned()),
                (PathBuf::from("/meta/src/Meta.fl"), META_LIB.to_owned()),
            ])),
            plugins,
            store: RefCell::default(),
        };
        (host, plugin)
    }

    #[cfg(feature = "ide")]
    fn write(&self, path: &str, text: &str) {
        self.files
            .borrow_mut()
            .insert(PathBuf::from(path), text.to_owned());
    }
}

impl Host for MetaHost {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.files
            .borrow()
            .get(path)
            .map(|text| text.clone().into_bytes())
            .ok_or_else(|| ErrorKind::NotFound.into())
    }

    fn display(&self, path: &Path) -> String {
        path.display().to_string()
    }

    fn plugin(&self, package: &str, _: Option<&Path>) -> Result<Arc<dyn Expander>, String> {
        self.plugins
            .get(package)
            .cloned()
            .ok_or_else(|| format!("no plugin `{package}`"))
    }

    fn cached(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.store.borrow().get(key).cloned()
    }

    fn save(&self, key: &[u8], value: &[u8]) {
        self.store.borrow_mut().insert(key.to_vec(), value.to_vec());
    }
}

fn project_with(sources: &mut SourceMap, deps: &[&str]) -> Project {
    Project {
        root: PathBuf::from("/p"),
        config_file: sources.add("flick.toml", ""),
        workspace: None,
        config: Config {
            kind: Kind::Lib,
            source_dir: "src".into(),
            test_dir: "test".into(),
            proof_dir: "proofs".into(),
            entries: Vec::new(),
            dependencies: deps
                .iter()
                .map(|name| Dependency {
                    name: (*name).into(),
                    source: Source::Path(format!("../{name}")),
                    span: Span::new(FileId::default(), 0, 0),
                })
                .collect(),
            target_dependencies: Vec::new(),
            package: None,
            host: None,
            build: Build::default(),
            targets: BTreeMap::new(),
            sql_dialect: Default::default(),
            allow: Vec::new(),
            no_let_assert: false,
            unused_proof_premises: false,
            thresholds: Thresholds::default(),
            plugins: BTreeMap::new(),
            tools: BTreeMap::new(),
        },
    }
}

fn roots() -> [PathBuf; 1] {
    [PathBuf::from("/p/src/App/Main.fl")]
}

#[test]
fn external_declared_heads_and_watch_work_and_agree_with_ide_analysis() {
    let main = r#"module App.Main exposing (..)

import Meta exposing (plugin route, plugin widget, plugin ref_of)

route fn greeting = "hello"

widget banner = "welcome"

target_name : String
let target_name = ref_of App.Main.banner

bad_call : String
let bad_call = Meta.guarded "forbidden"
"#;
    let (host, _) = MetaHost::with_meta(main);
    let mut sources = SourceMap::default();
    let project = project_with(&mut sources, &["meta"]);
    let found = check(&host, &mut sources, Some(&project), &roots());
    let shown: Vec<String> = found.iter().map(|d| show(&sources, d)).collect();
    assert_eq!(shown.len(), 1, "{shown:#?}");
    assert!(
        shown[0].contains("meta/M003") && shown[0].contains("forbidden"),
        "{shown:#?}"
    );

    // IDE / LSP analysis agrees identically on external heads and watch diagnostics.
    #[cfg(feature = "ide")]
    {
        let mut ide_sources = SourceMap::default();
        let ide_project = project_with(&mut ide_sources, &["meta"]);
        let analyzed = analyze(&host, &mut ide_sources, Some(&ide_project), &roots());
        let ide_shown: Vec<String> = analyzed
            .diagnostics
            .iter()
            .map(|d| show(&ide_sources, d))
            .collect();
        assert_eq!(ide_shown, shown);
    }
}

#[cfg(feature = "ide")]
#[test]
fn derive_cache_hits_on_unchanged_and_invalidates_on_docs_attrs_visibility_and_types() {
    let base = r"module App.Main exposing (User(..), encoded, help)

import Meta

/// Account profile
@label user
type User = User {
  /// Display name
  zebra : String;
  /// Age in years
  alpha : Int
} deriving Meta.Codec, Meta.CliForm

encoded : User -> String
let encoded u = Meta.encode u

help : String
let help = Meta.help_text (None : Option User)
";
    let (host, plugin) = MetaHost::with_meta(base);
    let mut cache = ModuleCache::default();
    let run = |text: &str, cache: &mut ModuleCache| {
        host.write("/p/src/App/Main.fl", text);
        let mut sources = SourceMap::default();
        let project = project_with(&mut sources, &["meta"]);
        let analysis = analyze_with(&host, &mut sources, Some(&project), &roots(), cache);
        analysis
            .diagnostics
            .iter()
            .map(|d| show(&sources, d))
            .collect::<Vec<_>>()
    };

    // Initial run: derives Codec + CliForm = 2 calls.
    let initial_diags = run(base, &mut cache);
    assert!(initial_diags.is_empty(), "{initial_diags:#?}");
    assert_eq!(plugin.derives.load(Ordering::Relaxed), 2);

    // Re-running with identical source hits cache: 0 new derive calls.
    assert!(run(base, &mut cache).is_empty());
    assert_eq!(plugin.derives.load(Ordering::Relaxed), 2);

    // Adding an unrelated comment outside the type declaration hits derive cache!
    let with_comment = format!("{base}\n// unrelated comment\n");
    let comment_diags = run(&with_comment, &mut cache);
    assert!(comment_diags.is_empty(), "{comment_diags:#?}");
    assert_eq!(plugin.derives.load(Ordering::Relaxed), 2);

    // Changing a `///` doc comment invalidates the derive cache!
    let changed_doc = base.replace("Account profile", "Customer profile");
    assert!(run(&changed_doc, &mut cache).is_empty());
    assert_eq!(plugin.derives.load(Ordering::Relaxed), 4);

    // Changing an `@label` attribute invalidates the derive cache!
    let changed_attr = changed_doc.replace("@label user", "@label acct");
    assert!(run(&changed_attr, &mut cache).is_empty());
    assert_eq!(plugin.derives.load(Ordering::Relaxed), 6);

    // Changing visibility (`User(..)` -> `User`, making it Opaque) invalidates
    // the derive cache and makes `Meta.Codec` refuse with M002!
    let opaque_vis = changed_attr.replace("exposing (User(..),", "exposing (User,");
    let diags = run(&opaque_vis, &mut cache);
    assert!(
        diags.iter().any(|d| d.contains("meta/M002")),
        "expected M002 on opaque visibility change: {diags:#?}"
    );
    assert!(plugin.derives.load(Ordering::Relaxed) >= 7);

    // Changing a field type invalidates the derive cache!
    let changed_type = changed_attr.replace("alpha : Int", "alpha : Bool");
    assert!(run(&changed_type, &mut cache).is_empty());
    assert!(plugin.derives.load(Ordering::Relaxed) >= 9);
}
