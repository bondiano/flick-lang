//! Zed starts `flick lsp --stdio` from the `flick` on the worktree's `PATH`.

use zed_extension_api::{self as zed, LanguageServerId, Result};

struct Flick;

impl zed::Extension for Flick {
    fn new() -> Self {
        Flick
    }

    fn language_server_command(
        &mut self,
        _id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let command = worktree
            .which("flick")
            .ok_or_else(|| "`flick` is not on PATH: install the Flick toolchain".to_owned())?;
        Ok(zed::Command {
            command,
            args: vec!["lsp".to_owned(), "--stdio".to_owned()],
            env: worktree.shell_env(),
        })
    }
}

zed::register_extension!(Flick);
