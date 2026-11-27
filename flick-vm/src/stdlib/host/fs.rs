//! Native Rust `File` and core `FileSystem` host service operations.

use std::fs::{self, File, FileTimes, OpenOptions};
use std::io::{self, Write as _};
#[cfg(unix)]
use std::os::unix::fs as unix_fs;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use flick_core::ir::Lit;
use flick_core::ir::memory::HostServiceIntrinsic;

use crate::stdlib::hooks::VmHooks;
use crate::tier0::{Error, Heap, Value};

use super::HostMode;
use super::error_map::{
    bytes_from_int_list, check_no_nul, decode_utf8_bom, expect_bool, expect_float, expect_int,
    int_list_from_bytes, ok, option_some, platform_bad_arg, platform_sys_err, platform_sys_err_tag,
    record_field, system_error_tag, vec_to_string_list,
};
use super::fs_stat::{open_options_from_flag, stat_info_from_metadata};

pub(super) fn run(
    heap: &mut Heap,
    op: HostServiceIntrinsic,
    args: &[Value],
    hooks: &mut dyn VmHooks,
) -> Result<Value, Error> {
    use HostServiceIntrinsic as H;
    if hooks.host_mode() == HostMode::Deterministic {
        return Err(Error::UnsupportedOperation(
            "host filesystem in deterministic mode",
        ));
    }
    match op {
        H::FsAccess => fs_access(heap, &args[0], &args[1]),
        H::FsChmod => fs_chmod(heap, &args[0], &args[1]),
        H::FsChown => fs_chown(heap, &args[0], &args[1], &args[2]),
        H::FsExists => fs_exists(heap, &args[0]),
        H::FsLink => fs_link(heap, &args[0], &args[1]),
        H::FsMakeDirectory => fs_make_directory(heap, &args[0], &args[1]),
        H::FsReadDirectory => fs_read_directory(heap, &args[0], &args[1]),
        H::FsReadFile => fs_read_file(heap, &args[0]),
        H::FsReadFileString => fs_read_file_string(heap, &args[0]),
        H::FsReadLink => fs_read_link(heap, &args[0]),
        H::FsRealPath => fs_real_path(heap, &args[0]),
        H::FsRemove => fs_remove(heap, &args[0], &args[1]),
        H::FsRename => fs_rename(heap, &args[0], &args[1]),
        H::FsStat => fs_stat(heap, &args[0]),
        H::FsSymlink => fs_symlink(heap, &args[0], &args[1]),
        H::FsTruncate => fs_truncate(heap, &args[0], &args[1]),
        H::FsUtimes => fs_utimes(heap, &args[0], &args[1], &args[2]),
        H::FsWriteFile => fs_write_file(heap, &args[0], &args[1], &args[2]),
        H::FsWriteFileString => fs_write_file_string(heap, &args[0], &args[1], &args[2]),
        _ => Err(Error::UnsupportedPrimitive(
            "unexpected fs host service op".into(),
        )),
    }
}

fn fs_done(
    heap: &mut Heap,
    res: io::Result<()>,
    method: &str,
    syscall: &str,
    path: &str,
) -> Result<Value, Error> {
    match res {
        Ok(()) => ok(heap, Value::Unit),
        Err(err) => platform_sys_err(heap, &err, "FileSystem", method, Some(syscall), Some(path)),
    }
}

fn fs_access(heap: &mut Heap, opts: &Value, path_val: &Value) -> Result<Value, Error> {
    let path = heap.string_lossy(path_val)?;
    if let Some(bad) = check_no_nul(heap, "access", &[&path])? {
        return Ok(bad);
    }
    let readable = expect_bool(record_field(heap, opts, "readable")?)?;
    let writable = expect_bool(record_field(heap, opts, "writable")?)?;
    let meta = match fs::metadata(&path) {
        Ok(m) => m,
        Err(err) => {
            return platform_sys_err(
                heap,
                &err,
                "FileSystem",
                "access",
                Some("access"),
                Some(&path),
            );
        }
    };
    if readable {
        let res = if meta.is_dir() {
            fs::read_dir(&path).map(|_| ())
        } else {
            File::open(&path).map(|_| ())
        };
        if let Err(err) = res {
            return platform_sys_err(
                heap,
                &err,
                "FileSystem",
                "access",
                Some("access"),
                Some(&path),
            );
        }
    }
    if writable {
        if meta.is_dir() {
            if meta.permissions().readonly() {
                return platform_sys_err_tag(
                    heap,
                    "PermissionDenied",
                    "FileSystem",
                    "access",
                    None,
                    Some("access"),
                    Some(&path),
                );
            }
        } else if let Err(err) = OpenOptions::new().write(true).open(&path) {
            return platform_sys_err(
                heap,
                &err,
                "FileSystem",
                "access",
                Some("access"),
                Some(&path),
            );
        }
    }
    ok(heap, Value::Unit)
}

fn fs_chmod(heap: &mut Heap, mode_val: &Value, path_val: &Value) -> Result<Value, Error> {
    let path = heap.string_lossy(path_val)?;
    if let Some(bad) = check_no_nul(heap, "chmod", &[&path])? {
        return Ok(bad);
    }
    let mode = expect_int(mode_val)?;
    if mode < 0 {
        return platform_bad_arg(heap, "FileSystem", "chmod", "mode must be non-negative");
    }
    #[cfg(unix)]
    let res = {
        use std::os::unix::fs::PermissionsExt as _;
        #[expect(clippy::cast_sign_loss, reason = "checked mode >= 0")]
        fs::set_permissions(&path, fs::Permissions::from_mode(mode as u32))
    };
    #[cfg(not(unix))]
    let res = fs::metadata(&path).and_then(|m| {
        let mut perms = m.permissions();
        perms.set_readonly((mode & 0o222) == 0);
        fs::set_permissions(&path, perms)
    });
    fs_done(heap, res, "chmod", "chmod", &path)
}

fn fs_chown(
    heap: &mut Heap,
    uid_val: &Value,
    gid_val: &Value,
    path_val: &Value,
) -> Result<Value, Error> {
    let path = heap.string_lossy(path_val)?;
    if let Some(bad) = check_no_nul(heap, "chown", &[&path])? {
        return Ok(bad);
    }
    let uid_f = expect_float(uid_val)?;
    let gid_f = expect_float(gid_val)?;
    let max_id = f64::from(u32::MAX);
    if !uid_f.is_finite()
        || uid_f.fract() != 0.0
        || uid_f < -1.0
        || uid_f > max_id
        || !gid_f.is_finite()
        || gid_f.fract() != 0.0
        || gid_f < -1.0
        || gid_f > max_id
    {
        return platform_bad_arg(
            heap,
            "FileSystem",
            "chown",
            "uid and gid must be integers from -1 to 4294967295",
        );
    }
    #[cfg(unix)]
    let res = {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "validated integer in 0..=u32::MAX"
        )]
        let uid = if uid_f < 0.0 {
            None
        } else {
            Some(uid_f as u32)
        };
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "validated integer in 0..=u32::MAX"
        )]
        let gid = if gid_f < 0.0 {
            None
        } else {
            Some(gid_f as u32)
        };
        unix_fs::chown(&path, uid, gid)
    };
    #[cfg(not(unix))]
    let res = Ok(());
    fs_done(heap, res, "chown", "chown", &path)
}

fn fs_exists(heap: &mut Heap, path_val: &Value) -> Result<Value, Error> {
    let path = heap.string_lossy(path_val)?;
    match fs::metadata(&path) {
        Ok(_) => ok(heap, Value::Bool(true)),
        Err(err) if system_error_tag(&err) == "NotFound" => ok(heap, Value::Bool(false)),
        Err(err) => platform_sys_err(
            heap,
            &err,
            "FileSystem",
            "access",
            Some("access"),
            Some(&path),
        ),
    }
}

fn fs_link(heap: &mut Heap, from_val: &Value, to_val: &Value) -> Result<Value, Error> {
    let from = heap.string_lossy(from_val)?;
    let to = heap.string_lossy(to_val)?;
    if let Some(bad) = check_no_nul(heap, "link", &[&from, &to])? {
        return Ok(bad);
    }
    fs_done(heap, fs::hard_link(&from, &to), "link", "link", &from)
}

fn fs_make_directory(heap: &mut Heap, opts: &Value, path_val: &Value) -> Result<Value, Error> {
    let path = heap.string_lossy(path_val)?;
    let recursive = expect_bool(record_field(heap, opts, "recursive")?)?;
    let mode_opt = match option_some(heap, record_field(heap, opts, "mode")?)? {
        Some(v) => Some(expect_int(v)?),
        None => None,
    };
    let res = if recursive {
        fs::create_dir_all(&path)
    } else {
        fs::create_dir(&path)
    };
    if let Err(err) = res {
        return platform_sys_err(
            heap,
            &err,
            "FileSystem",
            "makeDirectory",
            Some("mkdir"),
            Some(&path),
        );
    }
    #[cfg(unix)]
    if let Some(m) = mode_opt
        && m >= 0
    {
        use std::os::unix::fs::PermissionsExt as _;
        #[expect(clippy::cast_sign_loss, reason = "checked m >= 0")]
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(m as u32));
    }
    #[cfg(not(unix))]
    let _ = mode_opt;
    ok(heap, Value::Unit)
}

fn fs_read_directory(heap: &mut Heap, rec_val: &Value, path_val: &Value) -> Result<Value, Error> {
    let recursive = expect_bool(rec_val)?;
    let path = heap.string_lossy(path_val)?;
    let mut entries = Vec::new();
    let root = Path::new(&path);
    if let Err(err) = walk_dir(root, root, recursive, &mut entries) {
        return platform_sys_err(
            heap,
            &err,
            "FileSystem",
            "readDirectory",
            Some("scandir"),
            Some(&path),
        );
    }
    entries.sort();
    let list = vec_to_string_list(heap, entries)?;
    ok(heap, list)
}

fn walk_dir(base: &Path, dir: &Path, recursive: bool, out: &mut Vec<String>) -> io::Result<()> {
    let mut children = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        children.push((entry.path(), entry.file_type()?));
    }
    children.sort_by(|a, b| a.0.cmp(&b.0));
    for (p, ft) in children {
        if let Ok(rel) = p.strip_prefix(base) {
            out.push(rel.to_string_lossy().into_owned());
        }
        if recursive && ft.is_dir() {
            walk_dir(base, &p, true, out)?;
        }
    }
    Ok(())
}

fn fs_read_file(heap: &mut Heap, path_val: &Value) -> Result<Value, Error> {
    let path = heap.string_lossy(path_val)?;
    match fs::read(&path) {
        Ok(bytes) => {
            let list = int_list_from_bytes(heap, &bytes)?;
            ok(heap, list)
        }
        Err(err) => platform_sys_err(
            heap,
            &err,
            "FileSystem",
            "readFile",
            Some("open"),
            Some(&path),
        ),
    }
}

fn fs_read_file_string(heap: &mut Heap, path_val: &Value) -> Result<Value, Error> {
    let path = heap.string_lossy(path_val)?;
    match fs::read(&path) {
        Ok(bytes) => {
            let text = decode_utf8_bom(&bytes);
            let v = heap.literal(&Lit::String(text))?;
            ok(heap, v)
        }
        Err(err) => platform_sys_err(
            heap,
            &err,
            "FileSystem",
            "readFile",
            Some("open"),
            Some(&path),
        ),
    }
}

fn fs_read_link(heap: &mut Heap, path_val: &Value) -> Result<Value, Error> {
    let path = heap.string_lossy(path_val)?;
    if let Some(bad) = check_no_nul(heap, "readLink", &[&path])? {
        return Ok(bad);
    }
    match fs::read_link(&path) {
        Ok(target) => {
            let v = heap.literal(&Lit::String(target.to_string_lossy().into_owned()))?;
            ok(heap, v)
        }
        Err(err) => platform_sys_err(
            heap,
            &err,
            "FileSystem",
            "readLink",
            Some("readlink"),
            Some(&path),
        ),
    }
}

fn fs_real_path(heap: &mut Heap, path_val: &Value) -> Result<Value, Error> {
    let path = heap.string_lossy(path_val)?;
    if let Some(bad) = check_no_nul(heap, "realPath", &[&path])? {
        return Ok(bad);
    }
    match fs::canonicalize(&path) {
        Ok(canonical) => {
            let v = heap.literal(&Lit::String(canonical.to_string_lossy().into_owned()))?;
            ok(heap, v)
        }
        Err(err) => platform_sys_err(
            heap,
            &err,
            "FileSystem",
            "realPath",
            Some("realpath"),
            Some(&path),
        ),
    }
}

fn fs_remove(heap: &mut Heap, opts: &Value, path_val: &Value) -> Result<Value, Error> {
    let path = heap.string_lossy(path_val)?;
    let recursive = expect_bool(record_field(heap, opts, "recursive")?)?;
    let force = expect_bool(record_field(heap, opts, "force")?)?;
    let meta = match fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(err) if force && system_error_tag(&err) == "NotFound" => return ok(heap, Value::Unit),
        Err(err) => {
            return platform_sys_err(heap, &err, "FileSystem", "remove", Some("rm"), Some(&path));
        }
    };
    if meta.is_dir() {
        if !recursive {
            return platform_sys_err_tag(
                heap,
                "UnknownError",
                "FileSystem",
                "remove",
                None,
                Some("rm"),
                Some(&path),
            );
        }
        return fs_done(heap, fs::remove_dir_all(&path), "remove", "rm", &path);
    }
    fs_done(heap, fs::remove_file(&path), "remove", "rm", &path)
}

fn fs_rename(heap: &mut Heap, from_val: &Value, to_val: &Value) -> Result<Value, Error> {
    let from = heap.string_lossy(from_val)?;
    let to = heap.string_lossy(to_val)?;
    fs_done(heap, fs::rename(&from, &to), "rename", "rename", &from)
}

fn fs_stat(heap: &mut Heap, path_val: &Value) -> Result<Value, Error> {
    let path = heap.string_lossy(path_val)?;
    match fs::metadata(&path) {
        Ok(meta) => {
            let info = stat_info_from_metadata(heap, &meta)?;
            ok(heap, info)
        }
        Err(err) => platform_sys_err(heap, &err, "FileSystem", "stat", Some("stat"), Some(&path)),
    }
}

fn fs_symlink(heap: &mut Heap, from_val: &Value, to_val: &Value) -> Result<Value, Error> {
    let target = heap.string_lossy(from_val)?;
    let path = heap.string_lossy(to_val)?;
    if let Some(bad) = check_no_nul(heap, "symlink", &[&target, &path])? {
        return Ok(bad);
    }
    #[cfg(unix)]
    let res = unix_fs::symlink(&target, &path);
    #[cfg(not(unix))]
    let res = Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "symlink unsupported",
    ));
    fs_done(heap, res, "symlink", "symlink", &target)
}

fn fs_truncate(heap: &mut Heap, len_val: &Value, path_val: &Value) -> Result<Value, Error> {
    let path = heap.string_lossy(path_val)?;
    if let Some(bad) = check_no_nul(heap, "truncate", &[&path])? {
        return Ok(bad);
    }
    let size = if let Some(v) = option_some(heap, len_val)? {
        let n = expect_float(v)?;
        if !n.is_finite() || n.fract() != 0.0 || n.abs() > 9_007_199_254_740_991.0 {
            return platform_bad_arg(
                heap,
                "FileSystem",
                "truncate",
                "length must be a safe integer",
            );
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped >= 0 safe integer"
        )]
        {
            n.max(0.0) as u64
        }
    } else {
        0
    };
    let res = OpenOptions::new()
        .write(true)
        .open(&path)
        .and_then(|f| f.set_len(size));
    fs_done(heap, res, "truncate", "ftruncate", &path)
}

fn fs_utimes(
    heap: &mut Heap,
    atime_val: &Value,
    mtime_val: &Value,
    path_val: &Value,
) -> Result<Value, Error> {
    let path = heap.string_lossy(path_val)?;
    if let Some(bad) = check_no_nul(heap, "utime", &[&path])? {
        return Ok(bad);
    }
    let atime = expect_float(atime_val)?;
    let mtime = expect_float(mtime_val)?;
    if !atime.is_finite() || !mtime.is_finite() || atime.abs() > 8.64e15 || mtime.abs() > 8.64e15 {
        return platform_sys_err_tag(
            heap,
            "UnknownError",
            "FileSystem",
            "utime",
            None,
            Some("utime"),
            Some(&path),
        );
    }
    let at = millis_to_system_time(atime);
    let mt = millis_to_system_time(mtime);
    let times = FileTimes::new().set_accessed(at).set_modified(mt);
    let res = OpenOptions::new()
        .write(true)
        .open(&path)
        .or_else(|_| File::open(&path))
        .and_then(|f| f.set_times(times));
    fs_done(heap, res, "utime", "utime", &path)
}

fn millis_to_system_time(ms: f64) -> SystemTime {
    #[expect(clippy::cast_possible_truncation, reason = "validated ms <= 8.64e15")]
    let whole = ms as i64;
    if whole >= 0 {
        UNIX_EPOCH + Duration::from_millis(whole.unsigned_abs())
    } else {
        UNIX_EPOCH - Duration::from_millis(whole.unsigned_abs())
    }
}

fn fs_write_file(
    heap: &mut Heap,
    opts: &Value,
    path_val: &Value,
    data_val: &Value,
) -> Result<Value, Error> {
    let path = heap.string_lossy(path_val)?;
    let bytes = bytes_from_int_list(heap, data_val)?;
    write_with_options(heap, opts, &path, &bytes)
}

fn fs_write_file_string(
    heap: &mut Heap,
    opts: &Value,
    path_val: &Value,
    text_val: &Value,
) -> Result<Value, Error> {
    let path = heap.string_lossy(path_val)?;
    let text = heap.string_lossy(text_val)?;
    write_with_options(heap, opts, &path, text.as_bytes())
}

fn write_with_options(
    heap: &mut Heap,
    opts: &Value,
    path: &str,
    bytes: &[u8],
) -> Result<Value, Error> {
    let flag_tag = match option_some(heap, record_field(heap, opts, "flag")?)? {
        Some(f) => heap.tag(f)?.to_owned(),
        None => "FileSystem.OpenFlag.Write".to_owned(),
    };
    let mode_opt = match option_some(heap, record_field(heap, opts, "mode")?)? {
        Some(m) => Some(expect_int(m)?),
        None => None,
    };
    let (open_opts, _) = open_options_from_flag(&flag_tag, mode_opt, false);
    let res = open_opts.open(path).and_then(|mut f| f.write_all(bytes));
    fs_done(heap, res, "writeFile", "open", path)
}
