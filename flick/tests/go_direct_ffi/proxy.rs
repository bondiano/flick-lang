//! Offline `GOPROXY=file://...` fixture builder for Go e2e tests.
//!
//! Writes Go module proxy files (`@v/list`, `@v/<ver>.info`, `@v/<ver>.mod`,
//! `@v/<ver>.zip`) using uncompressed ZIP (store method 0) so tests can verify
//! `[target.go.dependencies]` and generated `go.mod`/`go.sum` without network access.

use std::fs;
use std::path::Path;

/// Writes a single Go module version into an offline `GOPROXY` directory.
pub(crate) fn write_module(
    proxy_root: &Path,
    module_path: &str,
    version: &str,
    files: &[(&str, &str)],
) {
    let go_mod = format!("module {module_path}\n\ngo 1.23\n");
    let vdir = proxy_root.join(escape_module_path(module_path)).join("@v");
    fs::create_dir_all(&vdir).expect("create proxy @v dir");
    fs::write(vdir.join("list"), format!("{version}\n")).expect("write list");
    fs::write(
        vdir.join(format!("{version}.info")),
        format!("{{\"Version\":\"{version}\",\"Time\":\"2026-01-01T00:00:00Z\"}}"),
    )
    .expect("write info");
    fs::write(vdir.join(format!("{version}.mod")), &go_mod).expect("write mod");

    let prefix = format!("{module_path}@{version}/");
    let mut entries: Vec<(String, Vec<u8>)> = Vec::with_capacity(files.len() + 1);
    entries.push((format!("{prefix}go.mod"), go_mod.into_bytes()));
    for (rel, body) in files {
        entries.push((format!("{prefix}{rel}"), body.as_bytes().to_vec()));
    }
    let zip = store_zip(&entries);
    fs::write(vdir.join(format!("{version}.zip")), zip).expect("write zip");
}

/// Formats a `GOPROXY` `file://` URL for `proxy_root` on both Unix and Windows.
pub(crate) fn file_url(proxy_root: &Path) -> String {
    let canonical = fs::canonicalize(proxy_root).unwrap_or_else(|_| proxy_root.to_path_buf());
    let raw = canonical.to_string_lossy().replace('\\', "/");
    let stripped = raw.strip_prefix("//?/").unwrap_or(&raw);
    if stripped.starts_with('/') {
        format!("file://{stripped}")
    } else {
        format!("file:///{stripped}")
    }
}

/// Clears read-only flags left by Go's module cache and removes `root` if present.
pub(crate) fn remove_readonly_tree(root: &Path) {
    fn make_writable(path: &Path) {
        if let Ok(meta) = fs::metadata(path) {
            let mut perms = meta.permissions();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                perms.set_mode(perms.mode() | 0o700);
            }
            #[cfg(not(unix))]
            #[allow(clippy::permissions_set_readonly_false)]
            perms.set_readonly(false);
            let _ = fs::set_permissions(path, perms);
        }
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                make_writable(&entry.path());
            }
        }
    }
    if root.exists() {
        make_writable(root);
        let _ = fs::remove_dir_all(root);
    }
}

/// Go proxy module path escaping: uppercase ASCII letters become `!<lowercase>`.
fn escape_module_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for c in path.chars() {
        if c.is_ascii_uppercase() {
            out.push('!');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// Builds a minimal valid ZIP archive using method 0 (STORE).
fn store_zip(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in entries {
        let offset = u32::try_from(out.len()).expect("zip offset fits u32");
        let crc = crc32(data);
        let size = u32::try_from(data.len()).expect("entry size fits u32");
        let name_bytes = name.as_bytes();
        let name_len = u16::try_from(name_bytes.len()).expect("name len fits u16");

        // Local file header
        out.extend_from_slice(&0x0403_4B50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes()); // version needed
        out.extend_from_slice(&0u16.to_le_bytes()); // flags
        out.extend_from_slice(&0u16.to_le_bytes()); // method = STORE
        out.extend_from_slice(&0u16.to_le_bytes()); // mod time
        out.extend_from_slice(&0u16.to_le_bytes()); // mod date
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&name_len.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // extra len
        out.extend_from_slice(name_bytes);
        out.extend_from_slice(data);

        // Central directory header
        central.extend_from_slice(&0x0201_4B50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes()); // version made by
        central.extend_from_slice(&20u16.to_le_bytes()); // version needed
        central.extend_from_slice(&0u16.to_le_bytes()); // flags
        central.extend_from_slice(&0u16.to_le_bytes()); // method = STORE
        central.extend_from_slice(&0u16.to_le_bytes()); // mod time
        central.extend_from_slice(&0u16.to_le_bytes()); // mod date
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&name_len.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes()); // extra len
        central.extend_from_slice(&0u16.to_le_bytes()); // comment len
        central.extend_from_slice(&0u16.to_le_bytes()); // disk number
        central.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
        central.extend_from_slice(&0u32.to_le_bytes()); // external attrs
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name_bytes);
    }
    let cd_offset = u32::try_from(out.len()).expect("cd offset fits u32");
    let cd_size = u32::try_from(central.len()).expect("cd size fits u32");
    let count = u16::try_from(entries.len()).expect("entry count fits u16");
    out.extend_from_slice(&central);

    // End of central directory record
    out.extend_from_slice(&0x0605_4B50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}
