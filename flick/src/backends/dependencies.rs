//! Content pins for the host's installed kernel. The guest receives the pin,
//! never permission to read or resolve a host filesystem path.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use flick_backend::RuntimeDependency;
use sha2::{Digest, Sha256};

pub(super) fn installed(root: &Path) -> Result<Vec<RuntimeDependency>, String> {
    let name = "@flick-lang/runtime";
    let Some(manifest) = root
        .ancestors()
        .filter(|dir| dir.file_name().is_none_or(|name| name != "node_modules"))
        .map(|dir| dir.join("node_modules").join(name).join("package.json"))
        .find(|path| path.is_file())
    else {
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

#[cfg(all(test, unix))]
#[test]
fn the_kernel_pin_follows_project_resolution_with_a_symlinked_adapter() {
    use std::os::unix::fs::symlink;
    use std::{env, process};

    let root = env::temp_dir().join(format!("flick-kernel-resolution-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    let project = root.join("project");
    let kernel = project.join("node_modules/@flick-lang/runtime");
    fs::create_dir_all(kernel.join("dist")).expect("kernel");
    fs::write(
        kernel.join("package.json"),
        r#"{"name":"@flick-lang/runtime","version":"0.1.0"}"#,
    )
    .expect("manifest");
    fs::write(kernel.join("dist/index.js"), "project kernel").expect("kernel bytes");
    let expected = installed(&project).expect("project pin");
    assert_eq!(expected.len(), 1);
    let adapter = root.join("external/adapter");
    let private = adapter.join("node_modules/@flick-lang/runtime");
    fs::create_dir_all(private.join("dist")).expect("private kernel");
    fs::write(
        private.join("package.json"),
        r#"{"name":"@flick-lang/runtime","version":"0.1.0"}"#,
    )
    .expect("manifest");
    fs::write(private.join("dist/index.js"), "private kernel").expect("private bytes");
    symlink(&adapter, project.join("node_modules/@flick-lang/effect-ts")).expect("adapter symlink");
    assert_eq!(installed(&project).expect("project pin"), expected);
    fs::write(kernel.join("dist/index.js"), "updated project kernel").expect("kernel change");
    assert_ne!(installed(&project).expect("changed project pin"), expected);
    fs::remove_dir_all(root).expect("cleanup");
}
