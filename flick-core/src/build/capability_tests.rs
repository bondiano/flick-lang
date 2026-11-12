use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{env, fs};

use super::capabilities::needs_adapter;
use super::{Extras, Settings, builtin_modules_of, emit};
use crate::codegen::EffectBridge;
use crate::diagnostics::{Diagnostic, codes};
use crate::ir::ItemKind;
use crate::ir::tests::check_text;
use crate::project::config::Backend;
use crate::project::{Compilation, Compiled};
use crate::source::SourceMap;

const CRON: &str = r#"module App.Main exposing (main)

main : Unit ->{} Unit
let main () =
  match Cron.parse (DateTime.zone_make_offset 0) "0 9 * * *" with
  | Ok cron -> ignore (Schedule.cron cron)
  | Err _ -> ()
"#;

fn diagnostics(text: &str, bridge: EffectBridge, release: bool) -> Vec<Diagnostic> {
    diagnostics_for(text, bridge, release, true)
}

fn diagnostics_for(text: &str, bridge: EffectBridge, release: bool, app: bool) -> Vec<Diagnostic> {
    diagnostics_with_ui(text, bridge, release, app, None)
}

fn diagnostics_with_ui(
    text: &str,
    bridge: EffectBridge,
    release: bool,
    app: bool,
    ui: Option<&str>,
) -> Vec<Diagnostic> {
    let (ir, interface) = check_text("App.Main", text).expect("the source checks");
    let mut sources = SourceMap::default();
    let file = sources.add("App/Main.fl", text);
    let compilation = Compilation {
        modules: vec![Compiled {
            name: ir.name.clone(),
            path: PathBuf::from("/app/src/App/Main.fl"),
            file,
            test: false,
            package: None,
            imports: vec![],
            ir,
            declarations: Arc::new(interface.clone()),
            interface,
            expansion: None,
        }],
        ..Compilation::default()
    };
    let entries = [("main".into(), "App.Main".into())];
    let settings = Settings {
        package: None,
        entries: if app { &entries } else { &[] },
        typescript: false,
        release,
        extras: Extras::default(),
        source_path: &|_| "App/Main.fl".into(),
        source_dir: &|_| None,
        out_dir: Path::new("/app/dist"),
        root: None,
        backend: Backend::Rt,
        effect_bridge: bridge,
        ui,
        runners: &[],
    };
    emit(&compilation, &sources, &settings).1
}

#[test]
fn rt_rejects_a_reachable_cron_without_the_effect_adapter() {
    for release in [false, true] {
        let found = diagnostics(CRON, EffectBridge::Absent, release);
        assert_eq!(found.len(), 3, "release={release}: {found:?}");
        let mut uses = Vec::new();
        for diagnostic in &found {
            assert_eq!(diagnostic.code, codes::P012);
            let span = diagnostic
                .span
                .expect("the unsupported use has a source span");
            uses.push(&CRON[span.start as usize..span.end as usize]);
            assert!(diagnostic.message.contains("@flick-lang/effect-ts"));
        }
        uses.sort_unstable();
        assert_eq!(
            uses,
            ["Cron.parse", "DateTime.zone_make_offset", "Schedule.cron"]
        );
    }
}

#[test]
fn rt_accepts_cron_when_the_adapter_is_available() {
    for release in [false, true] {
        let found = diagnostics(CRON, EffectBridge::Available, release);
        assert!(found.is_empty(), "release={release}: {found:?}");
    }
}

#[test]
fn rt_does_not_require_the_adapter_for_unused_schedule_bindings() {
    let source = r"module App.Main exposing (main)

unused : Cron.Cron ->{} Unit
let unused cron = ignore (Schedule.cron cron)

main : Unit ->{} Unit
let main () = ignore (Schedule.recurs 1)
";
    for release in [false, true] {
        let found = diagnostics(source, EffectBridge::Absent, release);
        assert!(found.is_empty(), "release={release}: {found:?}");
    }
}

#[test]
fn an_exported_library_binding_requires_its_runtime_capability() {
    let source = r"module App.Main exposing (cron)

cron : Cron.Cron -> Schedule.Schedule none none Int Duration.Duration
let cron = Schedule.cron
";
    let found = diagnostics_for(source, EffectBridge::Absent, false, false);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].code, codes::P012);
    let span = found[0]
        .span
        .expect("the exported binding has a source span");
    assert_eq!(
        &source[span.start as usize..span.end as usize],
        "Schedule.cron"
    );
}

#[test]
fn adapter_policy_overrides_are_not_missing_native_implementations() {
    assert!(!super::effects::unavailable_on_kernel(
        "@flick-lang/runtime/rt",
        "durationFormat"
    ));
    assert!(super::effects::unavailable_on_kernel(
        "@flick-lang/runtime/rt/effect/Schedule",
        "cron"
    ));
}

#[test]
fn a_custom_ui_provider_can_supply_an_export_missing_from_the_native_renderer() {
    // no export of `flick:ui/react` is missing on RT any more, so the binding names one RT lacks
    let source = "module App.Main exposing (gap)\ngap : String -> Ui\nforeign pure gap from \"flick:ui/react\" \"uiNotOnTheRuntime\"\n";
    assert_eq!(
        diagnostics_for(source, EffectBridge::Absent, false, false).len(),
        1
    );
    let found = diagnostics_with_ui(
        source,
        EffectBridge::Absent,
        false,
        false,
        Some("./renderer.mjs"),
    );
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_route_link_needs_neither_the_adapter_nor_a_custom_renderer() {
    let source = "module App.Main exposing (link)\nlink : Ui.RouteLinkProps a -> List (Ui^c) -> Ui^c\nlet link = Ui.route_link\n";
    let native = diagnostics_for(source, EffectBridge::Absent, false, false);
    assert!(native.is_empty(), "{native:?}");
}

#[test]
fn a_library_reexport_requires_its_runtime_capability() {
    let source = "module App.Main exposing (cron)\nimport Schedule exposing (cron)\n";
    for release in [false, true] {
        let found = diagnostics_for(source, EffectBridge::Absent, release, false);
        assert_eq!(found.len(), 1, "release={release}: {found:?}");
        assert_eq!(found[0].code, codes::P012);
        let span = found[0].span.expect("the reexport has a source span");
        assert_eq!(&source[span.start as usize..span.end as usize], "cron");
        assert!(diagnostics_for(source, EffectBridge::Available, release, false).is_empty());
    }
}

#[test]
fn eager_dictionary_values_and_discarded_initializers_need_their_capabilities() {
    for declaration in [
        "class Seed a =\n  seed : a\ninstance Seed DateTime.TimeZone =\n  let seed = DateTime.zone_make_offset 0\n",
        "let _ = DateTime.zone_make_offset 0\n",
    ] {
        let source = format!(
            "module App.Main exposing (main)\n{declaration}\nmain : Unit ->{{}} Unit\nlet main () = ()\n"
        );
        for release in [false, true] {
            let found = diagnostics(&source, EffectBridge::Absent, release);
            assert_eq!(found.len(), 1, "{source}: {found:?}");
            assert_eq!(found[0].code, codes::P012);
            let span = found[0].span.expect("initializer reference");
            assert_eq!(
                &source[span.start as usize..span.end as usize],
                "DateTime.zone_make_offset"
            );
        }
    }
}

#[test]
fn an_unused_dictionary_function_does_not_run_at_initialization() {
    let source = "module App.Main exposing (main)\nclass Seed a =\n  seed : Unit -> a\ninstance Seed DateTime.TimeZone =\n  let seed () = DateTime.zone_make_offset 0\nmain : Unit ->{} Unit\nlet main () = ()\n";
    assert!(diagnostics(source, EffectBridge::Absent, false).is_empty());
}

/// The bindings of std and core only the adapter implements, by the first
/// part of their module's name, as `runtime/adapter_only/<Family>.txt` lists
/// them: the list only shrinks. `FLICK_WRITE_ADAPTER_ONLY=1` writes it.
#[test]
fn the_bindings_only_the_adapter_has_only_go_down() {
    let mut computed: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for module in builtin_modules_of(true) {
        for item in &module.items {
            if let ItemKind::Foreign(foreign) = &item.kind
                && needs_adapter(foreign, None)
            {
                let family = module.name.split('.').next().unwrap_or(&module.name);
                computed
                    .entry(family.to_owned())
                    .or_default()
                    .insert(format!("{}.{}", module.name, foreign.name));
            }
        }
    }
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../runtime/adapter_only");
    if env::var_os("FLICK_WRITE_ADAPTER_ONLY").is_some() {
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("writable");
        for (family, bindings) in &computed {
            let mut lines = bindings.iter().cloned().collect::<Vec<_>>().join("\n");
            lines.push('\n');
            fs::write(dir.join(format!("{family}.txt")), lines).expect("writable");
        }
    }
    let listed: BTreeSet<String> = fs::read_dir(&dir)
        .map(|entries| entries.flatten().map(|e| e.path()).collect::<Vec<_>>())
        .unwrap_or_default()
        .iter()
        .flat_map(|path| {
            fs::read_to_string(path)
                .unwrap_or_default()
                .lines()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect();
    let computed: BTreeSet<String> = computed.into_values().flatten().collect();
    let new: Vec<&String> = computed.difference(&listed).collect();
    let moved: Vec<&String> = listed.difference(&computed).collect();
    assert!(
        new.is_empty(),
        "only the adapter implements these bindings of std: implement them in runtime/\n{new:#?}"
    );
    assert!(
        moved.is_empty(),
        "Flick's runtime implements these bindings now: delete their lines from runtime/adapter_only/\n{moved:#?}"
    );
}
