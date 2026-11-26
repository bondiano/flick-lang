use std::collections::BTreeMap;
use std::path::PathBuf;

use super::{Memory, analyze, library};
use crate::diagnostics::codes;
use crate::iface::Namespace;
use crate::source::SourceMap;
use crate::types::{self, Elem, Type};

const CONTRACTS: &str = "\
module App.Contracts exposing (StoreConfig, StoreRole)

type module StoreConfig =
  prefix : String

type module StoreRole =
  service Store =
    get : String ->{} Option String
  layer : Layer none none {Store}
";

const PRIMARY_CONFIG: &str = "\
module App.PrimaryConfig exposing (prefix)

prefix : String
let prefix = \"primary\"
";

const REPLICA_CONFIG: &str = "\
module App.ReplicaConfig exposing (prefix)

prefix : String
let prefix = \"replica\"
";

const MAKE_STORE: &str = "\
module App.MakeStore(C : App.Contracts.StoreConfig) : App.Contracts.StoreRole

service Store =
  get : String ->{} Option String

layer : Layer none none {Store}
let layer =
  Layer.succeed Store {
    get = fun key -> Some (C.prefix ++ \":\" ++ key);
  }
";

const ENV: &str = "\
module App.Env exposing (Primary, Replica)

module Primary = App.MakeStore(App.PrimaryConfig)
module Replica = App.MakeStore(App.ReplicaConfig)
";

fn base_files() -> BTreeMap<PathBuf, String> {
    BTreeMap::from([
        (PathBuf::from("/p/src/App/Contracts.fl"), CONTRACTS.into()),
        (
            PathBuf::from("/p/src/App/PrimaryConfig.fl"),
            PRIMARY_CONFIG.into(),
        ),
        (
            PathBuf::from("/p/src/App/ReplicaConfig.fl"),
            REPLICA_CONFIG.into(),
        ),
        (PathBuf::from("/p/src/App/MakeStore.fl"), MAKE_STORE.into()),
        (PathBuf::from("/p/src/App/Env.fl"), ENV.into()),
    ])
}

#[test]
fn two_generative_provider_roles_merge_cleanly_without_e004() {
    let mut files = base_files();
    files.insert(
        PathBuf::from("/p/src/App/Use.fl"),
        "\
module App.Use exposing (read_both, run_both)

import App.Env.Primary as Primary
import App.Env.Replica as Replica

read_both : String ->{Primary.Store, Replica.Store} (Option String, Option String)
let read_both key = (Primary.Store.get key, Replica.Store.get key)

run_both : String ->{} (Option String, Option String)
let run_both key =
  provide Layer.merge Primary.layer Replica.layer {
    read_both key
  }
"
        .into(),
    );
    let host = Memory(files);
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let analysis = analyze(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/App/Use.fl")],
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "expected clean composition of distinct roles, got: {:?}",
        analysis.diagnostics
    );
}

#[test]
fn providing_only_primary_fails_with_e002_when_calling_replica() {
    let mut files = base_files();
    files.insert(
        PathBuf::from("/p/src/App/Use.fl"),
        "\
module App.Use exposing (run_missing)

import App.Env.Primary as Primary
import App.Env.Replica as Replica

run_missing : String ->{} Option String
let run_missing key =
  provide Primary.layer {
    Replica.Store.get key
  }
"
        .into(),
    );
    let host = Memory(files);
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let analysis = analyze(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/App/Use.fl")],
    );
    assert!(
        analysis.diagnostics.iter().any(|d| d.code == codes::E002),
        "expected E002 for missing Replica.Store, got: {:?}",
        analysis.diagnostics
    );
}

#[test]
fn substituting_primary_layer_tag_or_role_impl_for_replica_fails_with_t003_naming_both_paths() {
    for (name, module_path, source, expected_a, expected_b) in [
        (
            "layer_full_paths",
            "/p/src/App/BadLayerFull.fl",
            "\
module App.BadLayerFull exposing (bad)

import App.Env.Primary
import App.Env.Replica

bad : Layer none none {App.Env.Replica.Store}
let bad = App.Env.Primary.layer
",
            "App.Env.Primary.Store",
            "App.Env.Replica.Store",
        ),
        (
            "layer_aliased",
            "/p/src/App/BadLayer.fl",
            "\
module App.BadLayer exposing (bad)

import App.Env.Primary as Primary
import App.Env.Replica as Replica

bad : Layer none none {Replica.Store}
let bad = Primary.layer
",
            "Primary.Store",
            "Replica.Store",
        ),
        (
            "service_tag_full_paths",
            "/p/src/App/BadTag.fl",
            "\
module App.BadTag exposing (bad)

import App.Env.Primary
import App.Env.Replica

bad : ServiceTag {App.Env.Replica.Store}
let bad = App.Env.Primary.Store
",
            "App.Env.Primary.Store",
            "App.Env.Replica.Store",
        ),
        (
            "role_impl_full_paths",
            "/p/src/App/BadRoleImpl.fl",
            "\
module App.BadRoleImpl exposing (bad)

type module HandleRole =
  type Record
  service Store =
    get : String ->{} Option Record

module MakeRole(C : App.Contracts.StoreConfig) : HandleRole =
  type Record = Record String
  service Store =
    get : String ->{} Option Record

module First = MakeRole(App.PrimaryConfig)
module Second = MakeRole(App.ReplicaConfig)

first_impl : First.Store.Impl
let first_impl = { get = fun _ -> None }

bad : Second.Store.Impl
let bad = first_impl
",
            "App.BadRoleImpl.First.Store.Impl",
            "App.BadRoleImpl.Second.Store.Impl",
        ),
    ] {
        let mut files = base_files();
        let module_path = PathBuf::from(module_path);
        files.insert(module_path.clone(), source.into());
        let host = Memory(files);
        let mut sources = SourceMap::default();
        let project = library(&mut sources);
        let analysis = analyze(&host, &mut sources, Some(&project), &[module_path]);
        let mismatch = analysis
            .diagnostics
            .iter()
            .find(|d| d.code == codes::T003)
            .unwrap_or_else(|| {
                panic!(
                    "expected T003 for {name} substitution, got: {:?}",
                    analysis.diagnostics
                )
            });
        assert!(
            mismatch.message.contains(expected_a) && mismatch.message.contains(expected_b),
            "{name}: expected both full paths `{expected_a}` and `{expected_b}` in T003 message, got: {mismatch:?}"
        );
    }
}

#[test]
fn merging_same_provider_role_twice_fails_with_e004() {
    let mut files = base_files();
    files.insert(
        PathBuf::from("/p/src/App/Ambiguous.fl"),
        "\
module App.Ambiguous exposing (dup)

import App.Env.Primary as Primary

dup : Layer none none {Primary.Store}
let dup = Layer.merge Primary.layer Primary.layer
"
        .into(),
    );
    let host = Memory(files);
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let analysis = analyze(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/App/Ambiguous.fl")],
    );
    assert!(
        analysis.diagnostics.iter().any(|d| d.code == codes::E004),
        "expected E004 for duplicate Primary.layer merge, got: {:?}",
        analysis.diagnostics
    );
}

#[test]
fn repeated_import_of_same_role_from_two_consumers_agrees_on_identity() {
    let mut files = base_files();
    files.insert(
        PathBuf::from("/p/src/App/First.fl"),
        "\
module App.First exposing (read_first, custom_impl)

import App.Env.Primary as Primary

read_first : String ->{Primary.Store} Option String
let read_first key = Primary.Store.get key

custom_impl : Primary.Store.Impl
let custom_impl = { get = fun key -> Some (\"custom:\" ++ key) }
"
        .into(),
    );
    files.insert(
        PathBuf::from("/p/src/App/Second.fl"),
        "\
module App.Second exposing (run_default, run_custom)

import App.Env.Primary as Primary
import App.First as First

run_default : String ->{} Option String
let run_default key =
  provide Primary.layer {
    First.read_first key
  }

run_custom : String ->{} Option String
let run_custom key =
  provide Layer.succeed Primary.Store First.custom_impl {
    First.read_first key
  }
"
        .into(),
    );
    let host = Memory(files);
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let analysis = analyze(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/App/Second.fl")],
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "expected repeated imports to share Primary.Store identity, got: {:?}",
        analysis.diagnostics
    );
}

#[test]
fn projected_service_row_is_relabeled_canonically_for_display_and_row_lookup() {
    let host = Memory(base_files());
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let analysis = analyze(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/App/Env.fl")],
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let primary = analysis
        .modules
        .iter()
        .find(|m| m.name == "App.Env.Primary")
        .expect("App.Env.Primary module");
    let scheme = primary
        .interface
        .find(Namespace::Value, "layer")
        .and_then(|e| e.scheme.as_ref())
        .expect("layer scheme");
    let shown = types::show_type(scheme);
    assert!(
        !shown.contains("@Store"),
        "projected service row must not leak `@Store`: {shown}"
    );
    assert_eq!(shown, "Layer none none {Store}");
    let (_, args) = scheme.spine();
    let Type::Row(row) = args.get(2).copied().expect("Layer output row") else {
        panic!("expected Row for Layer output, got: {scheme:?}");
    };
    assert_eq!(
        row.entries,
        vec![(
            Elem::Named("App.Env.Primary.Store".into()),
            Type::con("App.Env.Primary", "Store"),
        )]
    );
    assert_eq!(
        row.get("App.Env.Primary.Store"),
        Some(&Type::con("App.Env.Primary", "Store"))
    );
}

#[cfg(feature = "native-project")]
fn package_role_files() -> BTreeMap<PathBuf, String> {
    BTreeMap::from([
        (
            PathBuf::from("/lib/flick.toml"),
            "schema-version = 1\n[project]\nkind = \"lib\"\n".into(),
        ),
        (
            PathBuf::from("/lib/src/Lib/Contracts.fl"),
            CONTRACTS.replace("App.", "Lib."),
        ),
        (
            PathBuf::from("/lib/src/Lib/PrimaryConfig.fl"),
            PRIMARY_CONFIG.replace("App.", "Lib."),
        ),
        (
            PathBuf::from("/lib/src/Lib/ReplicaConfig.fl"),
            REPLICA_CONFIG.replace("App.", "Lib."),
        ),
        (
            PathBuf::from("/lib/src/Lib/MakeStore.fl"),
            MAKE_STORE.replace("App.", "Lib."),
        ),
        (
            PathBuf::from("/lib/src/Lib/Env.fl"),
            ENV.replace("App.", "Lib."),
        ),
        (
            PathBuf::from("/p/src/App/Main.fl"),
            "\
module App.Main exposing (run_both)

import Lib.Env.Primary as Primary
import Lib.Env.Replica as Replica

run_both : String ->{} (Option String, Option String)
let run_both key =
  provide Layer.merge Primary.layer Replica.layer {
    (Primary.Store.get key, Replica.Store.get key)
  }
"
            .into(),
        ),
    ])
}

#[cfg(feature = "native-project")]
fn assert_unique_role_modules(stage: &str, names: &[&str], check_roles: bool) {
    use std::collections::BTreeSet;

    assert_eq!(
        names.len(),
        names.iter().copied().collect::<BTreeSet<_>>().len(),
        "{stage} compilation had duplicate modules: {names:?}"
    );
    if check_roles {
        assert!(
            names.contains(&"Lib.Env.Primary") && names.contains(&"Lib.Env.Replica"),
            "{stage} cache missing role modules: {names:?}"
        );
    }
}

#[cfg(feature = "native-project")]
#[test]
fn package_exported_provider_roles_avoid_warm_cache_duplicates_and_work_offline() {
    use std::path::Path;

    use super::{Dependency, Source};
    use crate::project::{CachedModule, PackageCache, compile_with};
    use crate::source::Span;

    let mut host = Memory(package_role_files());
    let mut sources = SourceMap::default();
    let mut project = library(&mut sources);
    project.config.dependencies.push(Dependency {
        name: "lib".into(),
        source: Source::Path("../lib".into()),
        span: Span::new(project.config_file, 0, 0),
    });
    let identity = |_: &Path, _: Option<&str>| Some("provider-roles-pkg-v1".to_owned());
    let empty = PackageCache {
        salt: "provider-roles",
        identity: &identity,
        saved: &|_| None,
    };
    let roots = [PathBuf::from("/p/src/App/Main.fl")];

    // 1. Fresh compile populates the package cache.
    let (diagnostics, fresh_compilation, fresh) = compile_with(
        &host,
        &mut sources,
        Some(&project),
        &roots,
        Some(&empty),
        None,
        None,
    );
    assert!(diagnostics.is_empty(), "fresh: {diagnostics:?}");
    let fresh_names: Vec<_> = fresh_compilation
        .modules
        .iter()
        .map(|m| m.name.as_str())
        .collect();
    assert_unique_role_modules("fresh", &fresh_names, false);

    let saved: BTreeMap<_, Vec<CachedModule>> = fresh
        .into_iter()
        .map(|package| {
            let json = serde_json::to_vec(&package.modules).expect("package serializes");
            (
                package.key,
                serde_json::from_slice(&json).expect("package round trips"),
            )
        })
        .collect();

    let cache = PackageCache {
        salt: "provider-roles",
        identity: &identity,
        saved: &|key| saved.get(key).cloned(),
    };

    // 2. Warm cache with sources still on disk must not produce duplicate Compiled modules.
    let (diagnostics, warm_compilation, _) = compile_with(
        &host,
        &mut sources,
        Some(&project),
        &roots,
        Some(&cache),
        None,
        None,
    );
    assert!(diagnostics.is_empty(), "warm: {diagnostics:?}");
    let warm_names: Vec<_> = warm_compilation
        .modules
        .iter()
        .map(|m| m.name.as_str())
        .collect();
    assert_unique_role_modules("warm", &warm_names, true);

    // 3. Offline (library sources deleted from disk) continues to work from cache.
    host.0.retain(|path, _| !path.starts_with("/lib/src"));
    let (diagnostics, offline_compilation, _) = compile_with(
        &host,
        &mut sources,
        Some(&project),
        &roots,
        Some(&cache),
        None,
        None,
    );
    assert!(diagnostics.is_empty(), "offline: {diagnostics:?}");
    let offline_names: Vec<_> = offline_compilation
        .modules
        .iter()
        .map(|m| m.name.as_str())
        .collect();
    assert_unique_role_modules("offline", &offline_names, true);
}

#[cfg(feature = "native-js")]
#[test]
fn dts_projection_emits_distinct_brands_and_keys_for_each_provider_role() {
    use crate::codegen::ts::{Projection, declaration_file};
    use crate::iface::Interface;

    let mut files = base_files();
    files.insert(
        PathBuf::from("/p/src/App/Use.fl"),
        "\
module App.Use exposing (read_both)

import App.Env.Primary as Primary
import App.Env.Replica as Replica

read_both : String ->{Primary.Store, Replica.Store} (Option String, Option String)
let read_both key = (Primary.Store.get key, Replica.Store.get key)
"
        .into(),
    );
    let host = Memory(files);
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let analysis = analyze(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/App/Use.fl")],
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );

    let interfaces: BTreeMap<String, Interface> = analysis
        .modules
        .iter()
        .map(|m| (m.name.clone(), m.interface.clone()))
        .collect();
    let lookup = |name: &str| {
        interfaces
            .get(name)
            .cloned()
            .or_else(|| types::builtins().iter().find(|i| i.module == name).cloned())
    };
    let package = |_: &str| Some("@acme/stores".to_owned());
    let path = |m: &str| format!("./{}.js", m.replace('.', "/"));

    let primary_proj = Projection::new("App.Env.Primary", &lookup, &package);
    let (primary_dts, primary_refused) =
        declaration_file(&primary_proj, &interfaces["App.Env.Primary"], &path);
    assert!(primary_refused.is_empty(), "{primary_refused:?}");

    let replica_proj = Projection::new("App.Env.Replica", &lookup, &package);
    let (replica_dts, replica_refused) =
        declaration_file(&replica_proj, &interfaces["App.Env.Replica"], &path);
    assert!(replica_refused.is_empty(), "{replica_refused:?}");

    let primary_brand = primary_dts
        .lines()
        .find(|l| l.contains("\"~flick/brand\""))
        .unwrap_or_else(|| panic!("Primary.d.ts missing ~flick/brand:\n{primary_dts}"));
    let replica_brand = replica_dts
        .lines()
        .find(|l| l.contains("\"~flick/brand\""))
        .unwrap_or_else(|| panic!("Replica.d.ts missing ~flick/brand:\n{replica_dts}"));
    assert_ne!(
        primary_brand, replica_brand,
        "Primary and Replica must have distinct ~flick/brand interfaces"
    );
    assert!(
        primary_dts.contains("export declare const $s_Store:"),
        "Primary.d.ts missing $s_Store:\n{primary_dts}"
    );
    assert!(
        replica_dts.contains("export declare const $s_Store:"),
        "Replica.d.ts missing $s_Store:\n{replica_dts}"
    );

    let use_proj = Projection::new("App.Use", &lookup, &package);
    let (use_dts, use_refused) = declaration_file(&use_proj, &interfaces["App.Use"], &path);
    assert!(use_refused.is_empty(), "{use_refused:?}");
    let primary_alias = Projection::alias("App.Env.Primary");
    let replica_alias = Projection::alias("App.Env.Replica");
    assert_ne!(primary_alias, replica_alias);
    assert!(
        use_dts.contains(&format!("{primary_alias}.Store"))
            && use_dts.contains(&format!("{replica_alias}.Store")),
        "App.Use.d.ts must preserve both provider role identities:\n{use_dts}"
    );
}
