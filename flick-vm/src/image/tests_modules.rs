//! Unit tests for host-resource rejection, per-module interface linking, and module unloading.

use std::collections::BTreeMap;
use std::{env, fs, process};

use flick_core::iface::{self, Decl, Entity, Export, Interface, Namespace};
use flick_core::ir::Lit;
use flick_core::ir::memory::{
    Block, CodeId, Exit, Function, Global, HostServiceIntrinsic, Instruction, Intrinsic, Name,
    Operation, Program,
};
use flick_core::types::Type;

use super::tests::{adder_module_program, apply_global_program, partial_consumer_program};
use super::{ImageError, SaveSelection, VmImage};
use crate::session::{EvalOutcome, VmSession};
use crate::tier0::{Data, HostMode, Value};

fn mk_interface(
    module: &str,
    export_name: &str,
    ty_name: &str,
    deps: &[(&str, &str)],
) -> Interface {
    let mut iface = Interface {
        module: module.to_owned(),
        exports: vec![Export {
            entity: Entity {
                module: module.to_owned(),
                namespace: Namespace::Value,
                name: export_name.to_owned(),
            },
            decl: Decl::Let,
            parent: None,
            scheme: Some(Type::con("Core", ty_name)),
            definition: None,
            blocked: None,
            at: None,
            via: Vec::new(),
        }],
        ..Interface::default()
    };
    for &(dep, fp) in deps {
        iface.dependencies.insert(dep.to_owned(), fp.to_owned());
    }
    iface
}

#[test]
fn saving_host_resources_fails_with_exact_global_and_field_or_index_path() {
    let mut s = VmSession::with_host_mode(HostMode::Deterministic);
    let mut funcs = BTreeMap::new();
    funcs.insert(
        0 as CodeId,
        Function {
            params: vec![],
            body: Block {
                instructions: vec![Instruction::Let {
                    binding: 0,
                    value: Operation::Literal(Lit::Unit),
                }],
                exit: Exit::Return(0),
            },
        },
    );
    s.program.load(&Program {
        functions: funcs,
        globals: vec![
            Global {
                name: Name::new("App", "server"),
                initializer: 0,
            },
            Global {
                name: Name::new("App", "handles"),
                initializer: 0,
            },
        ],
        op_spans: BTreeMap::new(),
    });

    // Case 1: App.server.socket -> ForeignHandle
    let sock = s.heap.allocate(Data::ForeignHandle {
        execution: s.execution,
        host_id: 99,
    });
    let server_rec = s
        .heap
        .record(vec![
            ("port".into(), Value::Int(8080)),
            ("socket".into(), sock),
        ])
        .expect("server record");
    s.persistent
        .globals
        .insert(Name::new("App", "server"), server_rec);
    let err = s
        .save_modules(SaveSelection::Modules(&["App"]), None)
        .expect_err("must reject socket");
    let msg = err.to_string();
    assert!(
        msg.contains("App.server.socket"),
        "expected path `App.server.socket`, got: {msg}"
    );
    if let Some(v) = s.persistent.globals.remove(&Name::new("App", "server")) {
        s.heap.drop_value(v).expect("drop server");
    }

    // Case 2: App.handles[1] -> $vm_file
    let f0 = s.heap.allocate(Data::String("ok".into()));
    let f1 = s
        .heap
        .construct("$vm_file", vec![Value::Int64(7)])
        .expect("file handle");
    let list = s.heap.construct("$list", vec![f0, f1]).expect("list");
    s.persistent
        .globals
        .insert(Name::new("App", "handles"), list);
    let err = s
        .save_modules(SaveSelection::Modules(&["App"]), None)
        .expect_err("must reject file handle");
    let msg = err.to_string();
    assert!(
        msg.contains("App.handles[1]"),
        "expected path `App.handles[1]`, got: {msg}"
    );
    assert_eq!(s.shutdown(), Ok(()));
}

#[test]
fn saving_live_os_file_handle_fails_with_exact_global_and_field_path() {
    let tmp = env::temp_dir().join(format!("flick_vm_image_host_res_{}.txt", process::id()));
    fs::write(&tmp, "hi").expect("write temp file");
    let mut live = VmSession::with_host_mode(HostMode::LiveOs);
    let mut lf = BTreeMap::new();
    lf.insert(
        0 as CodeId,
        Function {
            params: vec![],
            body: Block {
                instructions: vec![
                    Instruction::Let {
                        binding: 0,
                        value: Operation::Literal(Lit::String(tmp.to_string_lossy().into_owned())),
                    },
                    Instruction::Let {
                        binding: 1,
                        value: Operation::Intrinsic {
                            kind: Intrinsic::HostService(HostServiceIntrinsic::FileOpen),
                            args: vec![0],
                        },
                    },
                ],
                exit: Exit::Return(1),
            },
        },
    );
    let open_prog = Program {
        functions: lf,
        globals: vec![Global {
            name: Name::new("LiveApp", "opened"),
            initializer: 0,
        }],
        op_spans: BTreeMap::new(),
    };
    let res = live.eval_declarations(
        &open_prog,
        "LiveApp",
        &[Name::new("LiveApp", "opened")],
        None,
        None,
    );
    assert!(matches!(res, EvalOutcome::Declaration { .. }), "{res:?}");
    let err = live
        .save_modules(SaveSelection::Modules(&["LiveApp"]), None)
        .expect_err("must reject real opened file handle");
    let msg = err.to_string();
    assert!(
        msg.contains("LiveApp.opened.0") && msg.contains("file handle"),
        "expected path `LiveApp.opened.0` naming file handle, got: {msg}"
    );
    let _ = fs::remove_file(&tmp);
    assert_eq!(live.shutdown(), Ok(()));
}

#[test]
fn per_module_linking_checks_interfaces_and_unload_frees_slots_after_last_closure_drops() {
    let mut s1 = VmSession::with_host_mode(HostMode::Deterministic);
    let lib_iface = mk_interface("Lib", "make_adder", "Int", &[]);
    let lib_fp = iface::fingerprint(&lib_iface).expect("lib fp");
    let app_iface = mk_interface("App", "add10", "Int", &[("Lib", &lib_fp)]);

    s1.register_module_interface(lib_iface);
    s1.register_module_interface(app_iface);

    let lib_prog = adder_module_program("Lib");
    let _ = s1.eval_declarations(
        &lib_prog,
        "Lib",
        &[Name::new("Lib", "make_adder"), Name::new("Lib", "tag")],
        None,
        None,
    );
    let app_prog = partial_consumer_program("App", "Lib");
    let _ = s1.eval_declarations(&app_prog, "App", &[Name::new("App", "add10")], None, None);

    let lib_bytes = s1
        .save_modules(SaveSelection::Modules(&["Lib"]), None)
        .expect("save Lib")
        .encode()
        .expect("encode Lib");
    let app_bytes = s1
        .save_modules(SaveSelection::Modules(&["App"]), None)
        .expect("save App")
        .encode()
        .expect("encode App");
    assert_eq!(s1.shutdown(), Ok(()));

    let lib_img = VmImage::decode(&lib_bytes).expect("decode Lib");
    let app_img = VmImage::decode(&app_bytes).expect("decode App");

    // 1. Loading App without Lib fails with MissingDependency.
    let mut s_missing = VmSession::with_host_mode(HostMode::Deterministic);
    assert!(matches!(
        s_missing.load_image(&app_img),
        Err(ImageError::MissingDependency { ref dependency, .. }) if dependency == "Lib"
    ));
    assert_eq!(s_missing.shutdown(), Ok(()));

    // 2. Loading App with a DIFFERENT Lib interface fails with InterfaceMismatch.
    let mut s_mismatch = VmSession::with_host_mode(HostMode::Deterministic);
    let wrong_lib_iface = mk_interface("Lib", "make_adder", "String", &[]);
    s_mismatch.register_module_interface(wrong_lib_iface);
    assert!(matches!(
        s_mismatch.load_image(&app_img),
        Err(ImageError::InterfaceMismatch { ref dependency, .. }) if dependency == "Lib"
    ));
    assert_eq!(s_mismatch.shutdown(), Ok(()));

    // 3. Loading Lib then App succeeds; unloading Lib keeps App's live closure working.
    let mut s2 = VmSession::with_host_mode(HostMode::Deterministic);
    s2.load_image(&lib_img).expect("load Lib");
    s2.load_image(&app_img).expect("load App");
    assert!(s2.heap_inventory().live_slots_for("Lib") > 0);

    let report = s2.unload_module("Lib").expect("unload Lib");
    assert_eq!(report.module, "Lib");
    assert_eq!(report.dependents, vec!["App".to_owned()]);
    assert!(
        report.remaining_live_slots > 0,
        "App.add10 closure into Lib must keep Lib's closure slot alive"
    );
    assert!(!s2.loaded_modules().contains("Lib"));

    let call_prog = apply_global_program("Call", "App", "add10", 32);
    let out = s2.eval_phrase(&call_prog, &Name::new("Call", "it"), None, false, None);
    assert_eq!(
        out,
        EvalOutcome::Value {
            rendered: "42".into(),
            stdout: String::new(),
        }
    );

    let app_report = s2.unload_module("App").expect("unload App");
    assert_eq!(app_report.remaining_live_slots, 0);
    assert_eq!(
        s2.heap_inventory().live_slots_for("Lib"),
        0,
        "after releasing last closure into Lib, heap inventory must show zero live slots for Lib"
    );
    assert_eq!(s2.shutdown(), Ok(()));
}
