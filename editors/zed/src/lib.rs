//! Zed uses `flick` from PATH, otherwise the release matching this extension.

use std::{fs, path::Path};
use zed_extension_api::{self as zed, LanguageServerId, Result, settings::LspSettings};

const SERVER_REPO: &str = "bondiano/flick-lang";
const SERVER_DIR_PREFIX: &str = "flick-v";

struct Flick {
    cached_server: Option<String>,
}

fn is_file(path: impl AsRef<Path>) -> bool {
    fs::metadata(path).is_ok_and(|meta| meta.is_file())
}

fn server_asset() -> Result<(String, zed::DownloadedFileType, &'static str)> {
    let (os, arch) = zed::current_platform();
    let arch = match arch {
        zed::Architecture::Aarch64 => "aarch64",
        zed::Architecture::X8664 => "x86_64",
        zed::Architecture::X86 => return Err("32-bit x86 is not supported".to_owned()),
    };
    Ok(match os {
        zed::Os::Mac => (
            format!("flick-{arch}-apple-darwin.tar.gz"),
            zed::DownloadedFileType::GzipTar,
            "flick",
        ),
        zed::Os::Linux => (
            format!("flick-{arch}-unknown-linux-gnu.tar.gz"),
            zed::DownloadedFileType::GzipTar,
            "flick",
        ),
        zed::Os::Windows => (
            format!("flick-{arch}-pc-windows-msvc.zip"),
            zed::DownloadedFileType::Zip,
            "flick.exe",
        ),
    })
}

impl Flick {
    fn server_path(&mut self, id: &LanguageServerId, worktree: &zed::Worktree) -> Result<String> {
        if let Some(path) = worktree.which("flick") {
            return Ok(path);
        }
        if let Some(path) = self.cached_server.as_ref().filter(|path| is_file(path)) {
            return Ok(path.clone());
        }
        let (asset_name, file_type, binary) = server_asset()?;
        let path = release_server(id, &asset_name, file_type, binary)
            .or_else(|err| installed_server(Path::new("."), binary).ok_or(err));
        let status = match &path {
            Ok(_) => zed::LanguageServerInstallationStatus::None,
            Err(err) => zed::LanguageServerInstallationStatus::Failed(err.clone()),
        };
        zed::set_language_server_installation_status(id, &status);
        let path = path?;
        self.cached_server = Some(path.clone());
        Ok(path)
    }
}

fn release_server(
    id: &LanguageServerId,
    asset_name: &str,
    file_type: zed::DownloadedFileType,
    binary: &str,
) -> Result<String> {
    let tag = concat!("v", env!("CARGO_PKG_VERSION"));
    let dir = format!("flick-{tag}");
    let path = format!("{dir}/{binary}");
    if is_file(&path) {
        return Ok(path);
    }
    zed::set_language_server_installation_status(
        id,
        &zed::LanguageServerInstallationStatus::CheckingForUpdate,
    );
    let release = zed::github_release_by_tag_name(SERVER_REPO, tag)?;
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == asset_name)
        .ok_or_else(|| format!("{SERVER_REPO} {tag} has no {asset_name}"))?;
    zed::set_language_server_installation_status(
        id,
        &zed::LanguageServerInstallationStatus::Downloading,
    );
    zed::download_file(&asset.download_url, &dir, file_type)?;
    zed::make_file_executable(&path)?;
    if let Ok(entries) = fs::read_dir(".") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(SERVER_DIR_PREFIX) && name != dir {
                fs::remove_dir_all(entry.path()).ok();
            }
        }
    }
    Ok(path)
}

fn installed_server(work_dir: &Path, binary: &str) -> Option<String> {
    fs::read_dir(work_dir)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let mut version = name.to_str()?.strip_prefix(SERVER_DIR_PREFIX)?.split('.');
            let version_key = (
                version.next()?.parse::<u64>().ok()?,
                version.next()?.parse::<u64>().ok()?,
                version.next()?.parse::<u64>().ok()?,
            );
            if version.next().is_some() {
                return None;
            }
            let path = entry.path().join(binary);
            is_file(&path).then(|| (version_key, path.to_string_lossy().into_owned()))
        })
        .max()
        .map(|(_, path)| path)
}

impl zed::Extension for Flick {
    fn new() -> Self {
        Self {
            cached_server: None,
        }
    }

    fn language_server_command(
        &mut self,
        id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let (path, args, env) = LspSettings::for_worktree("flick", worktree)
            .ok()
            .and_then(|settings| settings.binary)
            .map(|binary| {
                (
                    binary.path,
                    binary.arguments,
                    binary.env.unwrap_or_default(),
                )
            })
            .unwrap_or_default();
        let command = match path {
            Some(path) => path,
            None => self.server_path(id, worktree)?,
        };
        Ok(zed::Command {
            command,
            args: args.unwrap_or_else(|| vec!["lsp".to_owned(), "--stdio".to_owned()]),
            env: worktree.shell_env().into_iter().chain(env).collect(),
        })
    }
}

zed::register_extension!(Flick);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_cache_uses_newest_complete_server() {
        let dir = std::env::temp_dir().join(format!("zed-flick-cache-{}", std::process::id()));
        for version in ["flick-v0.9.0", "flick-v0.10.0", "unrelated-v9.0.0"] {
            let path = dir.join(version).join("flick");
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "").unwrap();
        }
        fs::create_dir_all(dir.join("flick-v1.0.0")).unwrap();
        assert_eq!(
            installed_server(&dir, "flick"),
            Some(
                dir.join("flick-v0.10.0/flick")
                    .to_string_lossy()
                    .into_owned()
            )
        );
        fs::remove_file(dir.join("flick-v0.10.0/flick")).unwrap();
        assert_eq!(
            installed_server(&dir, "flick"),
            Some(
                dir.join("flick-v0.9.0/flick")
                    .to_string_lossy()
                    .into_owned()
            )
        );
        fs::remove_file(dir.join("flick-v0.9.0/flick")).unwrap();
        assert_eq!(installed_server(&dir, "flick"), None);
        fs::remove_dir_all(dir).unwrap();
    }
}
