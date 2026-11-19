//! The command interface is one contract: the backend's and the plugin's
//! copies of its WIT stay the same.

#[test]
fn the_backend_and_the_plugin_share_one_command_interface() {
    let backend = include_str!("../wit/deps/command/command.wit");
    let plugin = include_str!("../../flick-plugin/wit/deps/command/command.wit");
    assert_eq!(
        backend, plugin,
        "copy flick-backend/wit/deps/command to flick-plugin/wit/deps"
    );
}
