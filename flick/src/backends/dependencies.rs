//! Content pins for the host's installed kernel. The guest receives the pin,
//! never permission to read or resolve a host filesystem path.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use flick_backend::RuntimeDependency;
use sha2::{Digest, Sha256};

use crate::toolchain;

pub(super) fn installed(root: &Path) -> Result<Vec<RuntimeDependency>, String> {
    let name = "@flick-lang/runtime";
    let Some(manifest) = toolchain::dependency_manifest(root, name) else {
        return Ok(vec![]);
    };
    let package = manifest
        .parent()
        .ok_or("runtime manifest has no directory")?;
    let json: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let version = json["version"]
        .as_str()
        .ok_or("runtime has no version")?
        .to_owned();
    if !package.join("dist").is_dir() {
        return Ok(vec![]);
    }
    let mut files = vec![PathBuf::from("package.json")];
    collect(package, Path::new("dist"), &mut files)
        .map_err(|e| format!("cannot pin the installed runtime: {e}"))?;
    files.sort();
    let mut hash = Sha256::new();
    for relative in files {
        let name = relative.to_string_lossy().replace('\\', "/");
        let path = package.join(&relative);
        let size = fs::metadata(&path).map_err(|e| e.to_string())?.len();
        hash.update(u64::try_from(name.len()).unwrap_or(u64::MAX).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update(size.to_le_bytes());
        io::copy(
            &mut fs::File::open(path).map_err(|e| e.to_string())?,
            &mut hash,
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(vec![RuntimeDependency {
        package: name.into(),
        version,
        sha256: format!("{:x}", hash.finalize()),
    }])
}

fn collect(package: &Path, relative: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(package.join(relative))? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let path = relative.join(entry.file_name());
        if kind.is_dir() {
            collect(package, &path, files)?;
        } else if kind.is_file() {
            files.push(path);
        } else {
            return Err(io::Error::other("runtime contains a nonregular file"));
        }
    }
    Ok(())
}
