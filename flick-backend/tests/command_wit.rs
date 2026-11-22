//! The command interface is one contract: the backend's and the plugin's
//! copies of its WIT stay the same.

use flick_backend::command::{FlickModule, Plan, RunFlickStep, Span, Step};

#[test]
fn the_backend_and_the_plugin_share_one_command_interface() {
    let backend = include_str!("../wit/deps/command/command.wit");
    let plugin = include_str!("../../flick-plugin/wit/deps/command/command.wit");
    assert_eq!(
        backend, plugin,
        "copy flick-backend/wit/deps/command to flick-plugin/wit/deps"
    );
}

#[test]
fn run_flick_step_round_trips_through_serde() {
    let mut plan = Plan::empty();
    plan.steps.push(Step::RunFlick(RunFlickStep {
        id: "export".into(),
        entry: "FlickSchemaExport".into(),
        modules: vec![FlickModule {
            name: "FlickSchemaExport".into(),
            source: "module FlickSchemaExport exposing (main)\n".into(),
        }],
        arguments: vec!["--flag".into()],
        output: Some(".flick-schema-export.out".into()),
        origin: Some(Span {
            path: "src/Lib/Wire.fl".into(),
            start: 10,
            end: 21,
        }),
    }));
    let json = serde_json::to_vec(&plan).expect("serialize");
    let decoded: Plan = serde_json::from_slice(&json).expect("deserialize");
    assert_eq!(decoded, plan);
}
