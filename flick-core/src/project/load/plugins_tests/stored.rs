use std::collections::{BTreeMap, HashMap};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use flick_plugin::{
    Arena, Block, CallSite, DeriveTarget, Diagnostic as PluginDiagnostic, Fact, Published,
    ReachError, Request, Response,
};
use flick_test_pairs::Pairs;

use super::super::{Mode, ModuleCache, ModuleStore, SavedModule, run};
use super::{Memory, blocks, host, library, roots};
use crate::ir;
use crate::plugin::reach::Outside;
use crate::plugin::source::BlockSource;
use crate::plugin::{Expander, Failure, Native};
use crate::project::config::Grants;
use crate::project::{Host, Project};
use crate::source::SourceMap;

/// An in-memory `ModuleStore` tracking saved payloads and load hits.
type StoreMap = Arc<Mutex<BTreeMap<String, Vec<u8>>>>;

fn store_of(
    salt: &str,
    saved: &StoreMap,
    saved_keys: &Arc<Mutex<Vec<String>>>,
    taken: &Arc<AtomicUsize>,
) -> ModuleStore {
    let (reader, writer, keys, taken) = (
        Arc::clone(saved),
        Arc::clone(saved),
        Arc::clone(saved_keys),
        Arc::clone(taken),
    );
    ModuleStore {
        salt: salt.to_owned(),
        inputs: Box::new(|_| None),
        keep_inputs: Box::new(|_, _| {}),
        load: Box::new(move |key| {
            let bytes = reader.lock().expect("the store").get(key).cloned()?;
            taken.fetch_add(1, Ordering::Relaxed);
            Some(serde_json::from_slice::<SavedModule>(&bytes).expect("a saved module"))
        }),
        save: Box::new(move |key, module| {
            let bytes = serde_json::to_vec(module).expect("a module to save");
            writer
                .lock()
                .expect("the store")
                .insert(key.to_owned(), bytes);
            keys.lock().expect("the keys").push(key.to_owned());
        }),
    }
}

/// Runs `host` with `store`, returning `(saved_keys_this_run, load_hits, module_irs)`.
fn run_stored(
    host: &dyn Host,
    saved: &StoreMap,
    roots: &[PathBuf],
    edit: impl Fn(&mut Project),
) -> (Vec<String>, usize, Vec<String>) {
    let saved_keys: Arc<Mutex<Vec<String>>> = Arc::default();
    let taken = Arc::new(AtomicUsize::new(0));
    let mut cache = ModuleCache::stored(store_of("one", saved, &saved_keys, &taken));
    let mut sources = SourceMap::default();
    let mut project = library(&mut sources);
    edit(&mut project);
    let output = run(
        host,
        &mut sources,
        Some(&project),
        roots,
        Mode::Compile,
        Some(&mut cache),
        None,
    );
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let irs = output
        .compilation
        .modules
        .iter()
        .map(|m| ir::print(&m.ir))
        .collect();
    let keys = saved_keys.lock().expect("the keys").clone();
    (keys, taken.load(Ordering::Relaxed), irs)
}

/// The keys of the modules a clean check of `host` gave a store.
fn saved_by(host: &dyn Host) -> Vec<String> {
    let saved = StoreMap::default();
    let saved_keys: Arc<Mutex<Vec<String>>> = Arc::default();
    let taken = Arc::new(AtomicUsize::new(0));
    let mut cache = ModuleCache::stored(store_of("one", &saved, &saved_keys, &taken));
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let roots = roots();
    let output = run(
        host,
        &mut sources,
        Some(&project),
        &roots,
        Mode::Check,
        Some(&mut cache),
        None,
    );
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    saved_keys.lock().expect("the keys").clone()
}

/// Modules with plugin blocks (`pairs`, `ui`, `sql`, `cli`, `codec`), custom
/// derives (`Schema`, `HttpApi`, `Pairs.Arbitrary`), or watched by a package
/// plugin are persisted to `ModuleStore` and restored identically on reload.
#[test]
fn modules_with_plugin_blocks_derives_and_watchers_are_saved_and_restored() {
    let plain = "module App.Main exposing (point)\n\npoint : Float\nlet point = 1.0\n";
    assert_eq!(saved_by(&host(plain)).len(), 1, "a module of no plugin");

    let expanded = host(&blocks(&[("x", "1.0")]));
    assert_eq!(saved_by(&expanded).len(), 1, "a module with a pairs block");

    let watched = host(plain);
    watched.write(
        "/pairs/flick.toml",
        "schema-version = 1\n[project]\nkind = \"lib\"\n[package]\nname = \"pairs\"\nversion = \"1.0.0\"\nplugin = \"pairs.wasm\"\nwatch = [\"Pairs.tally\"]\n",
    );
    assert_eq!(saved_by(&watched).len(), 1, "a module a package watches");

    let cases = [
        (
            "ui",
            "module App.Main exposing (view)\n\nimport Ui\n\nview : String -> Ui\nlet view label = ui { text label }\n",
        ),
        (
            "sql",
            "module App.Main exposing (query)\n\nsql table Users \"users\" {\n  id : bigint primary key as BigInt\n  name : text not null as String\n}\n\nlet query = sql {\n  from u in Users\n  select { id = u.id; name = u.name }\n}\n",
        ),
        (
            "cli",
            "module App.Main exposing (greet)\n\ngreet : Cli.Command none none\nlet greet = cli \"greet\" {\n  handle fun {} -> Platform.stdout_line \"hi\"\n}\n",
        ),
        (
            "codec_and_schema",
            "module App.Main exposing (User, user_json)\n\nimport Codec\n\ntype alias User = { name : String } deriving Schema\n\nuser_json : PureCodec Json User\nlet user_json = codec User.schema {\n  name = field \"name\" String.json\n}\n",
        ),
        (
            "http_api",
            "module App.Main exposing (Public)\n\nimport HttpApi.Derive exposing (..)\n\ntype alias Public = {\n  ping : Get \"/ping\" {} Unit {};\n} deriving HttpApi\n",
        ),
    ];
    for (name, source) in cases {
        let h = host(source);
        assert_eq!(saved_by(&h).len(), 1, "{name} should be saved");

        let saved = StoreMap::default();
        let (first_keys, first_taken, first_ir) = run_stored(&h, &saved, &roots(), |_| {});
        assert_eq!(first_keys.len(), 1, "{name}: saved on cold run");
        assert_eq!(first_taken, 0, "{name}: no hits on cold run");

        let (second_keys, second_taken, second_ir) = run_stored(&h, &saved, &roots(), |_| {});
        assert!(second_keys.is_empty(), "{name}: no re-save on warm run");
        assert_eq!(second_taken, 1, "{name}: restored from store");
        assert_eq!(second_ir, first_ir, "{name}: restored IR matches cold IR");
    }

    // Custom 3rd-party `@derive` (`Pairs.Arbitrary`) is also saved and restored.
    let pairs_derive = "module Pairs exposing (..)\n\n@derive\nclass Arbitrary a =\n  arbitrary : a\n\ninstance Arbitrary Float =\n  let arbitrary = 0.0\n";
    let main_derive = "module App.Main exposing (point)\n\nimport Pairs\n\ntype Point = Point Float Float\n  deriving Pairs.Arbitrary\n\npoint : Point\nlet point = Pairs.arbitrary\n";
    let h = host(main_derive);
    h.write("/pairs/src/Pairs.fl", pairs_derive);
    let saved = StoreMap::default();
    let (keys1, taken1, ir1) = run_stored(&h, &saved, &roots(), |_| {});
    assert_eq!((keys1.len(), taken1), (1, 0));
    let (keys2, taken2, ir2) = run_stored(&h, &saved, &roots(), |_| {});
    assert_eq!((keys2.len(), taken2), (0, 1));
    assert_eq!(ir1, ir2);
}

struct VersionedPlugin {
    id: &'static str,
    inner: Native<Pairs>,
}

impl Expander for VersionedPlugin {
    fn id(&self) -> &str {
        self.id
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
        self.inner
            .derive(target, class, args, module, state, source)
    }
    fn check(
        &self,
        block: &Block,
        state: &[u8],
        facts: &[Fact],
    ) -> Result<Vec<PluginDiagnostic>, String> {
        self.inner.check(block, state, facts)
    }
    fn check_calls(
        &self,
        calls: &[CallSite],
        state: &[u8],
    ) -> Result<Vec<PluginDiagnostic>, String> {
        self.inner.check_calls(calls, state)
    }
}

struct SwappableHost {
    base: Memory,
    plugin_id: Mutex<&'static str>,
}

impl Host for SwappableHost {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.base.read(path)
    }
    fn display(&self, path: &Path) -> String {
        self.base.display(path)
    }
    fn plugin(&self, package: &str, path: Option<&Path>) -> Result<Arc<dyn Expander>, String> {
        if package == "pairs" {
            let id = *self.plugin_id.lock().expect("id");
            return Ok(Arc::new(VersionedPlugin {
                id,
                inner: Native::new(id, Pairs),
            }));
        }
        self.base.plugin(package, path)
    }
    fn cached(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.base.cached(key)
    }
    fn save(&self, key: &[u8], value: &[u8]) {
        self.base.save(key, value);
    }
    fn outside(&self) -> Option<Arc<dyn Outside>> {
        self.base.outside()
    }
}

/// Modifying a plugin's component ID (`Expander::id()`) invalidates the
/// stored module cache for both block expansion and watching plugins.
#[test]
fn modifying_plugin_binary_invalidates_stored_module() {
    let h = SwappableHost {
        base: host(&blocks(&[("x", "1.0")])),
        plugin_id: Mutex::new("pairs sha256-v1"),
    };
    let saved = StoreMap::default();
    let (keys1, taken1, _) = run_stored(&h, &saved, &roots(), |_| {});
    assert_eq!((keys1.len(), taken1), (1, 0));

    let (keys2, taken2, _) = run_stored(&h, &saved, &roots(), |_| {});
    assert_eq!((keys2.len(), taken2), (0, 1));

    // Rebuild plugin binary -> Expander::id() changes -> cache miss & re-save.
    *h.plugin_id.lock().expect("id") = "pairs sha256-v2";
    let (keys3, taken3, _) = run_stored(&h, &saved, &roots(), |_| {});
    assert_eq!(
        (keys3.len(), taken3),
        (1, 0),
        "plugin id change misses cache"
    );
    assert_ne!(keys3[0], keys1[0], "new cache key generated");

    // Subsequent run with v2 hits the cache.
    let (keys4, taken4, _) = run_stored(&h, &saved, &roots(), |_| {});
    assert_eq!((keys4.len(), taken4), (0, 1));

    // Also verify watching-only plugin binary change invalidates a plain module.
    let plain = "module App.Main exposing (point)\n\npoint : Float\nlet point = 1.0\n";
    let watched = SwappableHost {
        base: host(plain),
        plugin_id: Mutex::new("pairs watch-v1"),
    };
    watched.base.write(
        "/pairs/flick.toml",
        "schema-version = 1\n[project]\nkind = \"lib\"\n[package]\nname = \"pairs\"\nversion = \"1.0.0\"\nplugin = \"pairs.wasm\"\nwatch = [\"Pairs.tally\"]\n",
    );
    let saved_watch = StoreMap::default();
    assert_eq!(
        run_stored(&watched, &saved_watch, &roots(), |_| {}).0.len(),
        1
    );
    assert_eq!(run_stored(&watched, &saved_watch, &roots(), |_| {}).1, 1);
    *watched.plugin_id.lock().expect("id") = "pairs watch-v2";
    let (k, t, _) = run_stored(&watched, &saved_watch, &roots(), |_| {});
    assert_eq!(
        (k.len(), t),
        (1, 0),
        "watching plugin id change misses cache"
    );
}

/// Modifying a watching plugin's watched names (`watch = [...]`) invalidates
/// stored modules even when module source text is unchanged.
#[test]
fn modifying_watching_plugin_configuration_invalidates_stored_module() {
    let plain = "module App.Main exposing (point)\n\npoint : Float\nlet point = 1.0\n";
    let h = host(plain);
    h.write(
        "/pairs/flick.toml",
        "schema-version = 1\n[project]\nkind = \"lib\"\n[package]\nname = \"pairs\"\nversion = \"1.0.0\"\nplugin = \"pairs.wasm\"\nwatch = [\"Pairs.tally\"]\n",
    );
    let saved = StoreMap::default();
    let (keys1, taken1, _) = run_stored(&h, &saved, &roots(), |_| {});
    assert_eq!((keys1.len(), taken1), (1, 0));
    let (keys2, taken2, _) = run_stored(&h, &saved, &roots(), |_| {});
    assert_eq!((keys2.len(), taken2), (0, 1));

    h.write(
        "/pairs/flick.toml",
        "schema-version = 1\n[project]\nkind = \"lib\"\n[package]\nname = \"pairs\"\nversion = \"1.0.0\"\nplugin = \"pairs.wasm\"\nwatch = [\"Pairs.keyed\"]\n",
    );
    let (keys3, taken3, _) = run_stored(&h, &saved, &roots(), |_| {});
    assert_eq!((keys3.len(), taken3), (1, 0));
    assert_ne!(keys3[0], keys1[0]);
}

#[derive(Default)]
struct TestWorld {
    files: Mutex<HashMap<PathBuf, Vec<u8>>>,
}

impl TestWorld {
    fn write(&self, path: &str, text: &str) {
        self.files
            .lock()
            .expect("files")
            .insert(PathBuf::from(path), text.as_bytes().to_vec());
    }
}

impl Outside for TestWorld {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.files
            .lock()
            .expect("files")
            .get(path)
            .cloned()
            .ok_or_else(|| ErrorKind::NotFound.into())
    }
    fn env(&self, _: &str) -> Option<String> {
        None
    }
    fn fetch(&self, _: &Path, request: &Request) -> Result<Response, ReachError> {
        Err(ReachError::Offline(request.url.clone()))
    }
    fn replay(&self, _: &Path) -> Option<Response> {
        None
    }
}

/// Modifying a file reached by a plugin during `collect` or `expand`
/// invalidates the `ModuleStore` entry and produces the updated IR.
#[test]
fn modifying_reached_file_invalidates_stored_module() {
    let world = Arc::new(TestWorld::default());
    world.write("/p/notes/a.txt", "first");
    world.write("/p/stated.txt", "collected-v1");
    let main = "module App.Main exposing (from_expand, from_collect)\n\nimport Pairs exposing (plugin read_file, plugin stated)\n\nfrom_expand : String\nlet from_expand = read_file \"notes/a.txt\" {}\n\nfrom_collect : String\nlet from_collect = stated {}\n";
    let mut h = host(main);
    h.outside = Some(Arc::clone(&world) as Arc<dyn Outside>);
    let grant = |p: &mut Project| {
        p.config.plugins.insert(
            "pairs".into(),
            Grants {
                fs_read: vec!["notes".into(), "stated.txt".into()],
                ..Grants::default()
            },
        );
    };

    let saved = StoreMap::default();
    let (keys1, taken1, ir1) = run_stored(&h, &saved, &roots(), grant);
    assert_eq!((keys1.len(), taken1), (1, 0));
    assert!(ir1[0].contains("\"first\"") && ir1[0].contains("\"collected-v1\""));

    // Unchanged files hit the store.
    let (keys2, taken2, ir2) = run_stored(&h, &saved, &roots(), grant);
    assert_eq!((keys2.len(), taken2), (0, 1));
    assert_eq!(ir2, ir1);

    // Modify file read during `expand`: cache miss & updated IR.
    world.write("/p/notes/a.txt", "second");
    let (keys3, taken3, ir3) = run_stored(&h, &saved, &roots(), grant);
    assert_eq!((keys3.len(), taken3), (1, 0));
    assert!(ir3[0].contains("\"second\""));

    // Modify file read during `collect`: cache miss & updated IR.
    world.write("/p/stated.txt", "collected-v2");
    let (keys4, taken4, ir4) = run_stored(&h, &saved, &roots(), grant);
    assert_eq!((keys4.len(), taken4), (1, 0));
    assert!(ir4[0].contains("\"collected-v2\""));
}

/// Modifying another module's `sql table` invalidates a dependent module's
/// `sql` query block even when the querying module's source text stays unchanged.
#[test]
fn modifying_group_sql_table_invalidates_query_module() {
    let schema_v1 = "module App.Schema exposing (Users)\n\nsql table Users \"users\" {\n  id : bigint primary key as BigInt\n  name : text not null as String\n}\n";
    let query = "module App.Query exposing (get_user)\n\nimport App.Schema exposing (Users)\n\nlet get_user = sql {\n  from u in Users\n  select { id = u.id; name = u.name }\n}\n";
    let h = host(query);
    h.write("/p/src/App/Schema.fl", schema_v1);
    h.write("/p/src/App/Query.fl", query);
    let two_roots = vec![
        PathBuf::from("/p/src/App/Schema.fl"),
        PathBuf::from("/p/src/App/Query.fl"),
    ];
    let saved = StoreMap::default();
    let (keys1, taken1, _) = run_stored(&h, &saved, &two_roots, |_| {});
    assert_eq!((keys1.len(), taken1), (2, 0));
    let (keys2, taken2, _) = run_stored(&h, &saved, &two_roots, |_| {});
    assert_eq!((keys2.len(), taken2), (0, 2));

    // Change table name in Schema (`"users"` -> `"accounts"`): the Flick types
    // of `Users` stay the same, but SQL group state & published data change.
    let schema_v2 = schema_v1.replace("\"users\"", "\"accounts\"");
    h.write("/p/src/App/Schema.fl", &schema_v2);
    let (keys3, taken3, ir3) = run_stored(&h, &saved, &two_roots, |_| {});
    assert_eq!(
        (keys3.len(), taken3),
        (2, 0),
        "both Schema and Query re-checked when SQL group state changed"
    );
    assert!(
        ir3.iter().any(|ir| ir.contains("accounts")),
        "updated SQL table name reflected in IR"
    );
}

/// Non-storable modules (inline modules and module templates) remain excluded
/// from `ModuleStore`.
#[test]
fn inline_modules_and_module_templates_remain_non_storable() {
    let inline = "module App.Main exposing (x)\n\nmodule Inner exposing (y) =\n  y : Float\n  let y = 1.0\n\nx : Float\nlet x = Inner.y\n";
    assert_eq!(
        saved_by(&host(inline)),
        Vec::<String>::new(),
        "inline modules are not saved"
    );
    let template = "module App.Main(K : App.Contracts.Source) : PublicResult exposing (answer)\nimport App.Contracts\nanswer : Int\nlet answer = K.value\n\ntype module PublicResult =\n  answer : Int\n";
    let h = host(template);
    h.write(
        "/p/src/App/Contracts.fl",
        "module App.Contracts exposing (Source)\ntype module Source =\n  value : Int\n",
    );
    // Only `App.Contracts` (plain module) is saved; `App.Main` (template) is not.
    assert_eq!(
        saved_by(&h).len(),
        1,
        "only the plain imported module is saved, not the module template"
    );
}
