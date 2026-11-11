//! The plugin of std for user interfaces. `component name (props : P) =
//! body` is a `Ui.View` of its props: a body of one expression is `Ui.view
//! (fun props -> body)`; statements before its last expression are the setup
//! of an instance and the last its render, `Ui.stateful (fun props -> setup;
//! fun props -> render)`. The setup runs once per instance, in the
//! instance's region; the render again whenever an atom it read changes.

pub mod block;
pub mod calls;
pub mod component;
