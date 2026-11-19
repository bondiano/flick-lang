//! The commands a plugin adds, through the same instances as its blocks.

use std::path::Path;
use std::sync::Arc;

use flick_backend::command;
use flick_backend::host::command as commands;
use flick_backend::host::runtime::Cancellation;
use flick_core::plugin::source::BlockSource;

use super::{Wasm, read};
use crate::commands::contributed::Contributor;

/// The plugin of `package` at `path`, as a contributor of commands.
pub fn contributor(package: &str, path: &Path) -> Result<Arc<dyn Contributor>, String> {
    let read = read(package, path)?;
    Ok(Arc::new(read))
}

impl Contributor for Wasm {
    fn describe(&self) -> Result<Vec<command::Descriptor>, String> {
        let (result, _) = self.run(BlockSource::default(), |plugin, store| {
            plugin.flick_command_command().call_describe(store)
        });
        result.map(commands::descriptors)
    }

    fn run(
        &self,
        invocation: &command::Invocation,
    ) -> Result<command::Plan, Vec<command::Diagnostic>> {
        let wire = commands::invocation(invocation);
        let (result, _) = self.run(BlockSource::default(), |plugin, store| {
            plugin.flick_command_command().call_run(store, &wire)
        });
        match result {
            Ok(Ok(plan)) => Ok(commands::plan(plan)),
            Ok(Err(diagnostics)) => Err(commands::diagnostics(diagnostics)),
            Err(why) => Err(command::unknown(&format!("{}: {why}", invocation.name))),
        }
    }

    fn resume(
        &self,
        state: Vec<u8>,
        outputs: Vec<command::File>,
        answers: Vec<command::Answer>,
    ) -> Result<command::Plan, Vec<command::Diagnostic>> {
        let wire: Vec<_> = outputs.iter().map(commands::file_wire).collect();
        let answers: Vec<_> = answers.iter().map(commands::answer_wire).collect();
        let (result, _) = self.run(BlockSource::default(), |plugin, store| {
            plugin
                .flick_command_command()
                .call_resume(store, &state, &wire, &answers)
        });
        match result {
            Ok(Ok(plan)) => Ok(commands::plan(plan)),
            Ok(Err(diagnostics)) => Err(commands::diagnostics(diagnostics)),
            Err(why) => Err(vec![command::Diagnostic::error(
                "P013",
                format!("resume: {why}"),
            )]),
        }
    }

    fn open(
        &self,
        invocation: &command::Invocation,
    ) -> Result<command::Opened, Vec<command::Diagnostic>> {
        let wire = commands::invocation(invocation);
        let (result, _) = self.run(BlockSource::default(), |plugin, store| {
            plugin.flick_command_command().call_open(store, &wire)
        });
        match result {
            Ok(Ok(opened)) => Ok(commands::opened(opened)),
            Ok(Err(diagnostics)) => Err(commands::diagnostics(diagnostics)),
            Err(why) => Err(command::unknown(&format!("{}: {why}", invocation.name))),
        }
    }

    // shortcut: a plugin call has no cancellation; fuel bounds it until the plugin host gets one
    fn handle(
        &self,
        state: Vec<u8>,
        request: Vec<u8>,
        answers: &[command::Answer],
        _: &Cancellation,
    ) -> Result<command::Handled, String> {
        let answers: Vec<_> = answers.iter().map(commands::answer_wire).collect();
        let (result, _) = self.run(BlockSource::default(), |plugin, store| {
            plugin
                .flick_command_command()
                .call_handle(store, &state, &request, &answers)
        });
        match result {
            Ok(Ok(handled)) => Ok(commands::handled(handled)),
            Ok(Err(why)) | Err(why) => Err(why),
        }
    }

    fn close(&self, state: Vec<u8>) -> command::Plan {
        let (result, _) = self.run(BlockSource::default(), |plugin, store| {
            plugin.flick_command_command().call_close(store, &state)
        });
        result.map_or_else(
            |why| {
                let mut plan = command::Plan::empty();
                plan.diagnostics = command::unknown(&format!("close: {why}"));
                plan
            },
            commands::plan,
        )
    }
}
