//! Files of a host build, including binary component and tool outputs.

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::{fs, io};

use flick_backend::{
    Artifact as BackendArtifact, ArtifactKind, Limits, validate_output_paths,
    validate_serialized_size,
};
use flick_core::build::Artifact as TextArtifact;
use serde::{Deserialize, Serialize};
use serde_bytes as byte_string;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub path: String,
    #[serde(with = "byte_string")]
    pub contents: Vec<u8>,
    pub kind: ArtifactKind,
}

impl Artifact {
    pub fn source(path: String, contents: String) -> Self {
        TextArtifact { path, contents }.into()
    }

    /// Legacy emit hooks transform text; object, executable and resource
    /// bytes retain their identity and never pass through a UTF-8 conversion.
    pub fn text(&self) -> Result<Option<TextArtifact>, String> {
        if self.is_binary() {
            return Ok(None);
        }
        let contents = String::from_utf8(self.contents.clone())
            .map_err(|_| format!("the text artifact `{}` is not UTF-8", self.path))?;
        Ok(Some(TextArtifact {
            path: self.path.clone(),
            contents,
        }))
    }

    pub fn is_binary(&self) -> bool {
        matches!(
            self.kind,
            ArtifactKind::Object | ArtifactKind::Executable | ArtifactKind::Resource
        )
    }

    pub fn restore_permissions(&self, path: &Path) -> io::Result<()> {
        #[cfg(unix)]
        if self.kind == ArtifactKind::Executable {
            let mut permissions = fs::metadata(path)?.permissions();
            if permissions.mode() & 0o111 != 0o111 {
                permissions.set_mode(permissions.mode() | 0o111);
                fs::set_permissions(path, permissions)?;
            }
        }
        #[cfg(not(unix))]
        let _ = path;
        Ok(())
    }

    pub fn current(&self, path: &Path) -> bool {
        if !fs::read(path).is_ok_and(|bytes| bytes == self.contents) {
            return false;
        }
        #[cfg(unix)]
        if self.kind == ArtifactKind::Executable {
            return fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 == 0o111);
        }
        true
    }
}

pub fn validate(files: &[Artifact]) -> Result<(), String> {
    let limits = Limits::default();
    validate_output_paths(files.iter().map(|f| f.path.as_str()), limits)?;
    if files
        .iter()
        .any(|f| f.contents.len() > limits.artifact_bytes)
    {
        return Err("a build artifact exceeds its file size limit".into());
    }
    validate_serialized_size(files, limits.result_bytes)
}

/// Refuse redirects and incompatible existing paths before touching any
/// earlier output. The host chooses the root; files only choose paths below it.
pub fn validate_destination(root: &Path, files: &[Artifact]) -> Result<(), String> {
    validate(files)?;
    validate_destination_paths(root, files.iter().map(|file| file.path.as_str()))
}

/// Include tool outputs and compiler-owned files in the same preflight.
pub fn validate_destination_paths<'a>(
    root: &Path,
    paths: impl IntoIterator<Item = &'a str>,
) -> Result<(), String> {
    validate_root(root)?;
    let paths: Vec<_> = paths.into_iter().collect();
    validate_output_paths(paths.iter().copied(), Limits::default())?;
    for file in paths {
        validate_ancestors(root, Path::new(file))?;
        match fs::symlink_metadata(root.join(file)) {
            Ok(metadata) if metadata.is_symlink() || !metadata.is_file() || shared(&metadata) => {
                return Err(format!(
                    "build output `{file}` conflicts with an existing path"
                ));
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("cannot inspect build output `{file}`: {e}")),
        }
    }
    Ok(())
}

/// Resolve only the host-selected project/directory prefix. Keep the output
/// suffix lexical so redirects inside it remain visible to validation.
pub fn resolve_root(trusted: &Path, output: &Path) -> PathBuf {
    output.strip_prefix(trusted).map_or_else(
        |_| output.to_path_buf(),
        |suffix| {
            fs::canonicalize(trusted)
                .unwrap_or_else(|_| trusted.to_path_buf())
                .join(suffix)
        },
    )
}

/// Output roots and their traversal must be ordinary directories. An external
/// directory is allowed; a symbolic redirect to another directory is not.
pub fn validate_root(root: &Path) -> Result<(), String> {
    for path in root.ancestors().filter(|path| !path.as_os_str().is_empty()) {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_symlink() || !metadata.is_dir() => {
                return Err(format!(
                    "build output root `{}` redirects or is not a directory",
                    path.display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "cannot inspect output root `{}`: {error}",
                    path.display()
                ));
            }
        }
    }
    Ok(())
}

fn shared(metadata: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        metadata.nlink() > 1
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        false
    }
}

/// Check parents for a path whose relative spelling the caller validated.
/// A stale file may itself be a symlink to unlink; its parents cannot redirect.
pub fn validate_ancestors(root: &Path, relative: &Path) -> Result<(), String> {
    let mut path = root.to_path_buf();
    for part in relative.parent().unwrap_or(Path::new("")).components() {
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_symlink() || !metadata.is_dir() => {
                return Err(format!(
                    "build output parent `{}` redirects or is not a directory",
                    path.display()
                ));
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => break,
            Err(e) => {
                return Err(format!(
                    "cannot inspect build output parent `{}`: {e}",
                    path.display()
                ));
            }
        }
    }
    Ok(())
}

impl From<TextArtifact> for Artifact {
    fn from(file: TextArtifact) -> Self {
        let ext = Path::new(&file.path).extension();
        let is = |extension: &str| ext.is_some_and(|e| e.eq_ignore_ascii_case(extension));
        let kind = if is("map") {
            ArtifactKind::SourceMap
        } else if file.path.to_ascii_lowercase().ends_with(".d.ts") {
            ArtifactKind::Declaration
        } else if is("iface") || is("json") {
            ArtifactKind::Metadata
        } else {
            ArtifactKind::Source
        };
        Self {
            path: file.path,
            contents: file.contents.into_bytes(),
            kind,
        }
    }
}

impl From<BackendArtifact> for Artifact {
    fn from(file: BackendArtifact) -> Self {
        Self {
            path: file.path,
            contents: file.bytes,
            kind: file.kind,
        }
    }
}
