//! The plugin of std for user interfaces. `component name (props : P) =
//! body` is a `Ui.View` of its props: a body of one expression is `Ui.view
//! (fun props -> body)`; statements before its last expression are the setup
//! of an instance and the last its render, `Ui.stateful (fun props -> setup;
//! fun props -> render)`. The setup runs once per instance, in the
//! instance's region; the render again whenever an atom it read changes.

use flick_plugin::{HeadSpec, PluginMetadata};

pub mod block;
pub mod calls;
pub mod component;

/// Static metadata declared by the first-party UI plugin.
#[must_use]
pub fn metadata() -> PluginMetadata {
    PluginMetadata {
        heads: vec![
            HeadSpec {
                head: "component".into(),
                module: "Ui".into(),
                named: true,
                ..HeadSpec::default()
            },
            HeadSpec {
                head: "ui".into(),
                module: "Ui".into(),
                ..HeadSpec::default()
            },
        ],
        watch: vec![
            "@cell".into(),
            "@maker".into(),
            "Atom.Read".into(),
            "Prelude.Setup".into(),
            "Ui.view".into(),
            "Ui.Event.exhaust".into(),
            "Ui.Event.latest".into(),
            "Ui.Event.queue".into(),
            "Ui.Event.concurrent".into(),
            "Action.queue".into(),
            "Action.concurrent".into(),
            "Atom.serializable".into(),
            "Atom.serializable_family".into(),
            "Atom.serializable_task".into(),
            "Atom.serializable_result".into(),
        ],
        derives: Vec::new(),
        own_codes: vec![
            "U001".into(),
            "U002".into(),
            "U003".into(),
            "U004".into(),
            "U005".into(),
            "U006".into(),
            "U007".into(),
            "E002".into(),
            "T036".into(),
            "T037".into(),
            "T038".into(),
        ],
    }
}
