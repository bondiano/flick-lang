//! Scoped open-file handle operations for `File` and `FileSystem.File s`.

use std::fs::{self, File};
use std::io::{self, Read as _, Write as _};

use flick_core::ir::Lit;
use flick_core::ir::memory::HostServiceIntrinsic;

use crate::stdlib::hooks::VmHooks;
use crate::tier0::{Error, Heap, Value};

use super::HostMode;
use super::error_map::{
    bytes_from_int_list, check_no_nul, expect_bigint_i64, expect_bool, expect_float, expect_int,
    file_err, handle_id, int_list_from_bytes, ok, opt_none, opt_some, option_some,
    platform_bad_arg, platform_sys_err, record_field,
};
use super::fs_stat::{open_options_from_flag, stat_info_from_metadata};

pub(super) const FILE_HANDLE_TAG: &str = "$vm_file";
pub(super) const FS_FILE_HANDLE_TAG: &str = "$vm_fs_file";

#[derive(Debug)]
pub(crate) struct FsFileEntry {
    file: File,
    path: String,
    append: bool,
    pos: u64,
}

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
        H::FileOpen => file_open(heap, &args[0], hooks),
        H::FileClose => file_close(heap, &args[0], hooks),
        H::FileReadHandle => file_read_handle(heap, &args[0], hooks),
        H::FileRead => file_read(heap, &args[0]),
        H::FileWrite => file_write(heap, &args[0], &args[1]),
        H::FsOpen => fs_open(heap, &args[0], &args[1], hooks),
        H::FsClose => fs_close(heap, &args[0], hooks),
        H::FsFileStat => fs_file_stat(heap, &args[0], hooks),
        H::FsSeek => fs_seek(heap, &args[0], &args[1], &args[2], hooks),
        H::FsSync => fs_sync(heap, &args[0], hooks),
        H::FsReadAlloc => fs_read_alloc(heap, &args[0], &args[1], &args[2], hooks),
        H::FsFileTruncate => fs_file_truncate(heap, &args[0], &args[1], hooks),
        H::FsWrite => fs_write(heap, &args[0], &args[1], hooks, false),
        H::FsWriteAll => fs_write(heap, &args[0], &args[1], hooks, true),
        _ => Err(Error::UnsupportedPrimitive(
            "unexpected fs_handle op".into(),
        )),
    }
}

fn file_open(heap: &mut Heap, path_val: &Value, hooks: &mut dyn VmHooks) -> Result<Value, Error> {
    let path = heap.string_lossy(path_val)?;
    match File::open(&path) {
        Ok(file) => {
            let svc = hooks
                .host_services()
                .ok_or(Error::UnsupportedOperation("host services unavailable"))?;
            let id = svc.alloc_handle();
            svc.files.insert(id, file);
            let handle = heap.construct(FILE_HANDLE_TAG, vec![Value::Int64(id)])?;
            ok(heap, handle)
        }
        Err(err) => file_err(heap, "OpenError", &err, "open", &path),
    }
}

fn file_close(heap: &Heap, handle_val: &Value, hooks: &mut dyn VmHooks) -> Result<Value, Error> {
    let id = handle_id(heap, handle_val, FILE_HANDLE_TAG)?;
    if let Some(svc) = hooks.host_services() {
        svc.files.remove(&id);
    }
    Ok(Value::Unit)
}

fn file_read_handle(
    heap: &mut Heap,
    handle_val: &Value,
    hooks: &mut dyn VmHooks,
) -> Result<Value, Error> {
    let id = handle_id(heap, handle_val, FILE_HANDLE_TAG)?;
    let svc = hooks
        .host_services()
        .ok_or(Error::UnsupportedOperation("host services unavailable"))?;
    let Some(file) = svc.files.get_mut(&id) else {
        let e = io::Error::from_raw_os_error(9);
        return file_err(heap, "ReadError", &e, "read", "");
    };
    let mut buf = Vec::new();
    match file.read_to_end(&mut buf) {
        Ok(_) => {
            let text = String::from_utf8_lossy(&buf).into_owned();
            let v = heap.literal(&Lit::String(text))?;
            ok(heap, v)
        }
        Err(err) => file_err(heap, "ReadError", &err, "read", ""),
    }
}

fn file_read(heap: &mut Heap, path_val: &Value) -> Result<Value, Error> {
    let path = heap.string_lossy(path_val)?;
    match fs::read(&path) {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            let v = heap.literal(&Lit::String(text))?;
            ok(heap, v)
        }
        Err(err) => file_err(heap, "ReadError", &err, "open", &path),
    }
}

fn file_write(heap: &mut Heap, path_val: &Value, text_val: &Value) -> Result<Value, Error> {
    let path = heap.string_lossy(path_val)?;
    let text = heap.string_lossy(text_val)?;
    match fs::write(&path, text.as_bytes()) {
        Ok(()) => ok(heap, Value::Unit),
        Err(err) => file_err(heap, "WriteError", &err, "open", &path),
    }
}

fn fs_open(
    heap: &mut Heap,
    opts: &Value,
    path_val: &Value,
    hooks: &mut dyn VmHooks,
) -> Result<Value, Error> {
    let path = heap.string_lossy(path_val)?;
    if let Some(bad) = check_no_nul(heap, "open", &[&path])? {
        return Ok(bad);
    }
    let flag_tag = match option_some(heap, record_field(heap, opts, "flag")?)? {
        Some(f) => heap.tag(f)?.to_owned(),
        None => "FileSystem.OpenFlag.Read".to_owned(),
    };
    let mode_opt = match option_some(heap, record_field(heap, opts, "mode")?)? {
        Some(m) => {
            let n = expect_int(m)?;
            if n < 0 {
                return platform_bad_arg(heap, "FileSystem", "open", "mode must be non-negative");
            }
            Some(n)
        }
        None => None,
    };
    let no_follow = expect_bool(record_field(heap, opts, "noFollow")?)?;
    #[cfg(not(unix))]
    if no_follow {
        return platform_bad_arg(
            heap,
            "FileSystem",
            "open",
            "noFollow is not supported on this platform",
        );
    }
    let (open_opts, append) = open_options_from_flag(&flag_tag, mode_opt, no_follow);
    match open_opts.open(&path) {
        Ok(file) => {
            let svc = hooks
                .host_services()
                .ok_or(Error::UnsupportedOperation("host services unavailable"))?;
            let id = svc.alloc_handle();
            svc.fs_files.insert(
                id,
                FsFileEntry {
                    file,
                    path,
                    append,
                    pos: 0,
                },
            );
            let handle = heap.construct(FS_FILE_HANDLE_TAG, vec![Value::Int64(id)])?;
            ok(heap, handle)
        }
        Err(err) => platform_sys_err(heap, &err, "FileSystem", "open", Some("open"), Some(&path)),
    }
}

fn fs_close(heap: &Heap, handle_val: &Value, hooks: &mut dyn VmHooks) -> Result<Value, Error> {
    let id = handle_id(heap, handle_val, FS_FILE_HANDLE_TAG)?;
    if let Some(svc) = hooks.host_services() {
        svc.fs_files.remove(&id);
    }
    Ok(Value::Unit)
}

fn missing_fd(heap: &mut Heap, method: &str, syscall: &str) -> Result<Value, Error> {
    let e = io::Error::from_raw_os_error(9);
    platform_sys_err(heap, &e, "FileSystem", method, Some(syscall), None)
}

fn fs_file_stat(
    heap: &mut Heap,
    handle_val: &Value,
    hooks: &mut dyn VmHooks,
) -> Result<Value, Error> {
    let id = handle_id(heap, handle_val, FS_FILE_HANDLE_TAG)?;
    let svc = hooks
        .host_services()
        .ok_or(Error::UnsupportedOperation("host services unavailable"))?;
    let Some(entry) = svc.fs_files.get(&id) else {
        return missing_fd(heap, "stat", "fstat");
    };
    let path = entry.path.clone();
    match entry.file.metadata() {
        Ok(meta) => {
            let info = stat_info_from_metadata(heap, &meta)?;
            ok(heap, info)
        }
        Err(err) => platform_sys_err(heap, &err, "FileSystem", "stat", Some("fstat"), Some(&path)),
    }
}

fn fs_seek(
    heap: &mut Heap,
    offset_val: &Value,
    from_val: &Value,
    handle_val: &Value,
    hooks: &mut dyn VmHooks,
) -> Result<Value, Error> {
    let id = handle_id(heap, handle_val, FS_FILE_HANDLE_TAG)?;
    let offset = expect_bigint_i64(heap, offset_val)?;
    let is_start = heap.tag(from_val)? == "FileSystem.SeekMode.Start";
    let svc = hooks
        .host_services()
        .ok_or(Error::UnsupportedOperation("host services unavailable"))?;
    let Some(entry) = svc.fs_files.get_mut(&id) else {
        return missing_fd(heap, "seek", "lseek");
    };
    let base = if is_start {
        0_i128
    } else {
        i128::from(entry.pos)
    };
    let next = base + i128::from(offset);
    if next < 0 {
        return platform_bad_arg(
            heap,
            "FileSystem",
            "seek",
            "Cannot seek before the start of the file",
        );
    }
    let next_u64 = u64::try_from(next).unwrap_or(u64::MAX);
    entry.pos = next_u64;
    let bi = heap.literal(&Lit::BigInt(next_u64.to_string()))?;
    ok(heap, bi)
}

fn fs_sync(heap: &mut Heap, handle_val: &Value, hooks: &mut dyn VmHooks) -> Result<Value, Error> {
    let id = handle_id(heap, handle_val, FS_FILE_HANDLE_TAG)?;
    let svc = hooks
        .host_services()
        .ok_or(Error::UnsupportedOperation("host services unavailable"))?;
    let Some(entry) = svc.fs_files.get(&id) else {
        return missing_fd(heap, "sync", "fsync");
    };
    let path = entry.path.clone();
    match entry.file.sync_all() {
        Ok(()) => ok(heap, Value::Unit),
        Err(err) => platform_sys_err(heap, &err, "FileSystem", "sync", Some("fsync"), Some(&path)),
    }
}

fn fs_read_alloc(
    heap: &mut Heap,
    size_val: &Value,
    pos_val: &Value,
    handle_val: &Value,
    hooks: &mut dyn VmHooks,
) -> Result<Value, Error> {
    let id = handle_id(heap, handle_val, FS_FILE_HANDLE_TAG)?;
    let size = expect_int(size_val)?;
    if size < 0 {
        return platform_bad_arg(
            heap,
            "FileSystem",
            "readAlloc",
            "size must be a non-negative integer",
        );
    }
    let explicit_pos = if let Some(p) = option_some(heap, pos_val)? {
        let off = expect_bigint_i64(heap, p)?;
        if off < 0 {
            return platform_bad_arg(
                heap,
                "FileSystem",
                "readAlloc",
                "position must be non-negative",
            );
        }
        #[expect(clippy::cast_sign_loss, reason = "checked off >= 0")]
        Some(off as u64)
    } else {
        None
    };
    let svc = hooks
        .host_services()
        .ok_or(Error::UnsupportedOperation("host services unavailable"))?;
    let Some(entry) = svc.fs_files.get_mut(&id) else {
        return missing_fd(heap, "readAlloc", "read");
    };
    let path = entry.path.clone();
    let pos = explicit_pos.unwrap_or(entry.pos);
    #[expect(clippy::cast_sign_loss, reason = "checked size >= 0")]
    let mut buf = vec![0u8; size as usize];
    let read_res = read_at_offset(&mut entry.file, &mut buf, pos);
    match read_res {
        Ok(0) => {
            let none = opt_none(heap)?;
            ok(heap, none)
        }
        Ok(n) => {
            if explicit_pos.is_none() {
                entry.pos = pos.saturating_add(n as u64);
            }
            let list = int_list_from_bytes(heap, &buf[..n])?;
            let some = opt_some(heap, list)?;
            ok(heap, some)
        }
        Err(err) => platform_sys_err(
            heap,
            &err,
            "FileSystem",
            "readAlloc",
            Some("read"),
            Some(&path),
        ),
    }
}

fn read_at_offset(file: &mut File, buf: &mut [u8], pos: u64) -> io::Result<usize> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt as _;
        file.read_at(buf, pos)
    }
    #[cfg(not(unix))]
    {
        use std::io::{Read as _, Seek as _, SeekFrom};
        file.seek(SeekFrom::Start(pos))?;
        file.read(buf)
    }
}

fn fs_file_truncate(
    heap: &mut Heap,
    len_val: &Value,
    handle_val: &Value,
    hooks: &mut dyn VmHooks,
) -> Result<Value, Error> {
    let id = handle_id(heap, handle_val, FS_FILE_HANDLE_TAG)?;
    let len = if let Some(v) = option_some(heap, len_val)? {
        let f = expect_float(v)?;
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped >= 0"
        )]
        {
            f.max(0.0) as u64
        }
    } else {
        0
    };
    let svc = hooks
        .host_services()
        .ok_or(Error::UnsupportedOperation("host services unavailable"))?;
    let Some(entry) = svc.fs_files.get_mut(&id) else {
        return missing_fd(heap, "truncate", "ftruncate");
    };
    let path = entry.path.clone();
    match entry.file.set_len(len) {
        Ok(()) => {
            if !entry.append && entry.pos > len {
                entry.pos = len;
            }
            ok(heap, Value::Unit)
        }
        Err(err) => platform_sys_err(
            heap,
            &err,
            "FileSystem",
            "truncate",
            Some("ftruncate"),
            Some(&path),
        ),
    }
}

fn fs_write(
    heap: &mut Heap,
    data_val: &Value,
    handle_val: &Value,
    hooks: &mut dyn VmHooks,
    write_all: bool,
) -> Result<Value, Error> {
    let id = handle_id(heap, handle_val, FS_FILE_HANDLE_TAG)?;
    let bytes = bytes_from_int_list(heap, data_val)?;
    let method = if write_all { "writeAll" } else { "write" };
    let svc = hooks
        .host_services()
        .ok_or(Error::UnsupportedOperation("host services unavailable"))?;
    let Some(entry) = svc.fs_files.get_mut(&id) else {
        return missing_fd(heap, method, "write");
    };
    let path = entry.path.clone();
    let res = if entry.append {
        if write_all {
            entry.file.write_all(&bytes).map(|()| bytes.len())
        } else {
            entry.file.write(&bytes)
        }
    } else {
        let written = write_at_offset(&mut entry.file, &bytes, entry.pos, write_all);
        if let Ok(n) = written {
            entry.pos = entry.pos.saturating_add(n as u64);
        }
        written
    };
    match res {
        Ok(n) => {
            if write_all {
                ok(heap, Value::Unit)
            } else {
                let count = i32::try_from(n).unwrap_or(i32::MAX);
                ok(heap, Value::Int(count))
            }
        }
        Err(err) => platform_sys_err(heap, &err, "FileSystem", method, Some("write"), Some(&path)),
    }
}

fn write_at_offset(file: &mut File, bytes: &[u8], pos: u64, all: bool) -> io::Result<usize> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt as _;
        if all {
            file.write_all_at(bytes, pos)?;
            Ok(bytes.len())
        } else {
            file.write_at(bytes, pos)
        }
    }
    #[cfg(not(unix))]
    {
        use std::io::{Seek as _, SeekFrom};
        file.seek(SeekFrom::Start(pos))?;
        if all {
            file.write_all(bytes)?;
            Ok(bytes.len())
        } else {
            file.write(bytes)
        }
    }
}
