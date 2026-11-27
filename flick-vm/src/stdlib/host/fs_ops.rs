//! Complex `FileSystem` operations: recursive copy, temp files/directories
//! (including scoped cleanup), and glob matching.

use std::env;
use std::fs::{self, File, FileTimes, OpenOptions};
use std::io::{self, ErrorKind};
#[cfg(unix)]
use std::os::unix::fs as unix_fs;
use std::path::{Path, PathBuf};

use flick_core::ir::Lit;
use flick_core::ir::memory::HostServiceIntrinsic;

use crate::stdlib::hooks::VmHooks;
use crate::tier0::{Error, Heap, Value};

use super::HostMode;
use super::error_map::{
    check_no_nul, expect_bool, ok, option_some, platform_sys_err, record_field, string_list_to_vec,
    vec_to_string_list,
};
use super::random::random_hex;

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
        H::FsCopy => fs_copy(heap, &args[0], &args[1], &args[2]),
        H::FsCopyFile => fs_copy_file(heap, &args[0], &args[1]),
        H::FsGlob => fs_glob(heap, &args[0], &args[1]),
        H::FsMakeTempDirectory => fs_make_temp_dir(heap, &args[0], false, hooks),
        H::FsMakeTempDirectoryScoped => fs_make_temp_dir(heap, &args[0], true, hooks),
        H::FsMakeTempFile => fs_make_temp_file(heap, &args[0], false, hooks),
        H::FsMakeTempFileScoped => fs_make_temp_file(heap, &args[0], true, hooks),
        H::FsRemoveTemp => fs_remove_temp(heap, &args[0], false, hooks),
        H::FsRemoveTempParent => fs_remove_temp(heap, &args[0], true, hooks),
        _ => Err(Error::UnsupportedPrimitive(
            "unexpected fs_ops host service op".into(),
        )),
    }
}

fn fs_copy_file(heap: &mut Heap, from_val: &Value, to_val: &Value) -> Result<Value, Error> {
    let src = heap.string_lossy(from_val)?;
    let dst = heap.string_lossy(to_val)?;
    if let Some(bad) = check_no_nul(heap, "copyFile", &[&src, &dst])? {
        return Ok(bad);
    }
    match copy_one_file(Path::new(&src), Path::new(&dst), false) {
        Ok(()) => ok(heap, Value::Unit),
        Err(err) => platform_sys_err(
            heap,
            &err,
            "FileSystem",
            "copyFile",
            Some("copyfile"),
            Some(&src),
        ),
    }
}

fn fs_copy(
    heap: &mut Heap,
    opts: &Value,
    from_val: &Value,
    to_val: &Value,
) -> Result<Value, Error> {
    let src = heap.string_lossy(from_val)?;
    let dst = heap.string_lossy(to_val)?;
    if let Some(bad) = check_no_nul(heap, "copy", &[&src, &dst])? {
        return Ok(bad);
    }
    let overwrite = expect_bool(record_field(heap, opts, "overwrite")?)?;
    let preserve = expect_bool(record_field(heap, opts, "preserveTimestamps")?)?;
    match copy_tree(Path::new(&src), Path::new(&dst), overwrite, preserve) {
        Ok(()) => ok(heap, Value::Unit),
        Err(err) => platform_sys_err(heap, &err, "FileSystem", "copy", Some("cp"), Some(&src)),
    }
}

fn same_file(a: &fs::Metadata, b: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        a.dev() == b.dev() && a.ino() == b.ino()
    }
    #[cfg(not(unix))]
    {
        let _ = (a, b);
        false
    }
}

fn copy_one_file(src: &Path, dst: &Path, exclusive: bool) -> io::Result<()> {
    let mut input = File::open(src)?;
    let in_meta = input.metadata()?;
    if in_meta.is_dir() {
        return Err(io::Error::from_raw_os_error(21)); // EISDIR
    }
    let mut out_opts = OpenOptions::new();
    out_opts.write(true);
    if exclusive {
        out_opts.create_new(true);
    } else {
        out_opts.create(true);
    }
    let mut output = out_opts.open(dst)?;
    let out_meta = output.metadata()?;
    if same_file(&in_meta, &out_meta) {
        return Ok(());
    }
    output.set_len(0)?;
    let mut limited = io::Read::take(&mut input, in_meta.len());
    io::copy(&mut limited, &mut output)?;
    let _ = output.set_permissions(in_meta.permissions());
    Ok(())
}

fn copy_tree(src: &Path, dst: &Path, overwrite: bool, preserve: bool) -> io::Result<()> {
    let meta = fs::symlink_metadata(src)?;
    if let Ok(dst_meta) = fs::symlink_metadata(dst)
        && (same_file(&meta, &dst_meta) || meta.is_dir() != dst_meta.is_dir())
    {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "invalid copy target",
        ));
    }
    if let Some(parent) = dst.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    if meta.is_dir() {
        if !dst.exists() {
            fs::create_dir(dst)?;
        }
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_tree(
                &entry.path(),
                &dst.join(entry.file_name()),
                overwrite,
                preserve,
            )?;
        }
        let _ = fs::set_permissions(dst, meta.permissions());
        return Ok(());
    }
    if meta.file_type().is_symlink() {
        let target = fs::read_link(src)?;
        if dst.symlink_metadata().is_ok() {
            if !overwrite {
                return Ok(());
            }
            let _ = fs::remove_file(dst);
        }
        #[cfg(unix)]
        return unix_fs::symlink(target, dst);
        #[cfg(not(unix))]
        return Err(io::Error::new(
            ErrorKind::Unsupported,
            "symlink unsupported",
        ));
    }
    if dst.symlink_metadata().is_ok() {
        if !overwrite {
            return Ok(());
        }
        fs::remove_file(dst)?;
    }
    copy_one_file(src, dst, true)?;
    if preserve
        && let (Ok(at), Ok(mt)) = (meta.accessed(), meta.modified())
        && let Ok(f) = OpenOptions::new().write(true).open(dst)
    {
        let _ = f.set_times(FileTimes::new().set_accessed(at).set_modified(mt));
    }
    Ok(())
}

fn create_temp_dir_raw(heap: &Heap, opts: &Value) -> Result<io::Result<PathBuf>, Error> {
    let base = match option_some(heap, record_field(heap, opts, "directory")?)? {
        Some(v) => PathBuf::from(heap.string_lossy(v)?),
        None => env::temp_dir(),
    };
    let prefix = match option_some(heap, record_field(heap, opts, "prefix")?)? {
        Some(v) => heap.string_lossy(v)?,
        None => String::new(),
    };
    let (parent, name_prefix) = if prefix.is_empty() {
        (base, String::new())
    } else {
        let full = base.join(&prefix);
        let p = full
            .parent()
            .map_or_else(|| base.clone(), Path::to_path_buf);
        let b = full
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        (p, b)
    };
    for _ in 0..64 {
        let candidate = parent.join(format!("{name_prefix}{}", random_hex(6)));
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(Ok(candidate)),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
            Err(e) => return Ok(Err(e)),
        }
    }
    Ok(Err(io::Error::new(
        ErrorKind::AlreadyExists,
        "could not allocate unique temp directory",
    )))
}

fn fs_make_temp_dir(
    heap: &mut Heap,
    opts: &Value,
    scoped: bool,
    hooks: &mut dyn VmHooks,
) -> Result<Value, Error> {
    let method = if scoped {
        "makeTempDirectoryScoped"
    } else {
        "makeTempDirectory"
    };
    match create_temp_dir_raw(heap, opts)? {
        Ok(dir) => {
            if scoped && let Some(svc) = hooks.host_services() {
                svc.scoped_temps.insert(dir.clone());
            }
            let text = heap.literal(&Lit::String(dir.to_string_lossy().into_owned()))?;
            ok(heap, text)
        }
        Err(err) => platform_sys_err(heap, &err, "FileSystem", method, Some("mkdtemp"), None),
    }
}

fn fs_make_temp_file(
    heap: &mut Heap,
    opts: &Value,
    scoped: bool,
    hooks: &mut dyn VmHooks,
) -> Result<Value, Error> {
    let method = if scoped {
        "makeTempFileScoped"
    } else {
        "makeTempFile"
    };
    let suffix = match option_some(heap, record_field(heap, opts, "suffix")?)? {
        Some(v) => heap.string_lossy(v)?,
        None => String::new(),
    };
    let dir = match create_temp_dir_raw(heap, opts)? {
        Ok(d) => d,
        Err(err) => {
            return platform_sys_err(heap, &err, "FileSystem", method, Some("mkdtemp"), None);
        }
    };
    let file_path = dir.join(format!("{}{suffix}", random_hex(6)));
    match File::create(&file_path) {
        Ok(_) => {
            if scoped && let Some(svc) = hooks.host_services() {
                svc.scoped_temps.insert(dir);
            }
            let text = heap.literal(&Lit::String(file_path.to_string_lossy().into_owned()))?;
            ok(heap, text)
        }
        Err(err) => {
            let _ = fs::remove_dir_all(&dir);
            let p = file_path.to_string_lossy().into_owned();
            platform_sys_err(heap, &err, "FileSystem", method, Some("open"), Some(&p))
        }
    }
}

fn fs_remove_temp(
    heap: &Heap,
    path_val: &Value,
    parent_dir: bool,
    hooks: &mut dyn VmHooks,
) -> Result<Value, Error> {
    let raw = PathBuf::from(heap.string_lossy(path_val)?);
    let target = if parent_dir {
        raw.parent().map_or_else(|| raw.clone(), Path::to_path_buf)
    } else {
        raw
    };
    if let Some(svc) = hooks.host_services() {
        svc.scoped_temps.remove(&target);
    }
    let _ = fs::remove_dir_all(&target).or_else(|_| fs::remove_file(&target));
    Ok(Value::Unit)
}

fn fs_glob(heap: &mut Heap, opts: &Value, pattern_val: &Value) -> Result<Value, Error> {
    let pattern = heap.string_lossy(pattern_val)?;
    let root = match option_some(heap, record_field(heap, opts, "root")?)? {
        Some(v) => heap.string_lossy(v)?,
        None => String::new(),
    };
    let excludes = string_list_to_vec(heap, record_field(heap, opts, "exclude")?)?;
    let expanded = expand_braces(&pattern);
    let mut exclude_expanded = Vec::new();
    for ex in &excludes {
        exclude_expanded.extend(expand_braces(ex));
    }
    let mut results = Vec::new();
    let start_dir = if root.is_empty() {
        PathBuf::from(".")
    } else {
        PathBuf::from(&root)
    };
    if start_dir.exists() {
        collect_glob_matches(&start_dir, "", &expanded, &exclude_expanded, &mut results);
    }
    results.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    results.dedup();
    let list = vec_to_string_list(heap, results)?;
    ok(heap, list)
}

fn collect_glob_matches(
    dir: &Path,
    rel_prefix: &str,
    patterns: &[String],
    excludes: &[String],
    out: &mut Vec<String>,
) {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = read_dir.flatten().collect();
    entries.sort_by(|a, b| {
        a.file_name()
            .to_string_lossy()
            .encode_utf16()
            .cmp(b.file_name().to_string_lossy().encode_utf16())
    });
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let rel = if rel_prefix.is_empty() {
            name.clone()
        } else {
            format!("{rel_prefix}/{name}")
        };
        let is_dir = entry.file_type().is_ok_and(|ft| ft.is_dir());
        if excludes.iter().any(|ex| glob_match_path(ex, &rel)) {
            continue;
        }
        if patterns.iter().any(|pat| glob_match_path(pat, &rel)) {
            out.push(rel.clone());
        }
        if is_dir {
            collect_glob_matches(&entry.path(), &rel, patterns, excludes, out);
        }
    }
}

fn expand_braces(pattern: &str) -> Vec<String> {
    let Some(open) = pattern.find('{') else {
        return vec![pattern.to_owned()];
    };
    let mut depth = 0usize;
    let mut close = None;
    for (idx, ch) in pattern[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(open + idx);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(close) = close else {
        return vec![pattern.to_owned()];
    };
    let prefix = &pattern[..open];
    let body = &pattern[open + 1..close];
    let suffix = &pattern[close + 1..];
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut d = 0usize;
    for (idx, ch) in body.char_indices() {
        match ch {
            '{' => d += 1,
            '}' => d = d.saturating_sub(1),
            ',' if d == 0 => {
                parts.push(&body[start..idx]);
                start = idx + 1;
            }
            _ => {}
        }
    }
    parts.push(&body[start..]);
    let mut out = Vec::new();
    for part in parts {
        let combined = format!("{prefix}{part}{suffix}");
        out.extend(expand_braces(&combined));
    }
    out
}

fn glob_match_path(pattern: &str, path: &str) -> bool {
    let p_segs: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
    let t_segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    match_segments(&p_segs, &t_segs)
}

fn match_segments(pat: &[&str], target: &[&str]) -> bool {
    match (pat.first(), target.first()) {
        (None, None) => true,
        (Some(&"**"), _) => {
            match_segments(&pat[1..], target)
                || (!target.is_empty() && match_segments(pat, &target[1..]))
        }
        (Some(p), Some(t)) => {
            if t.starts_with('.') && !p.starts_with('.') {
                return false;
            }
            match_segment_chars(p.as_bytes(), t.as_bytes())
                && match_segments(&pat[1..], &target[1..])
        }
        _ => false,
    }
}

fn match_segment_chars(p: &[u8], t: &[u8]) -> bool {
    match (p.first(), t.first()) {
        (None, None) => true,
        (Some(b'*'), _) => {
            match_segment_chars(&p[1..], t) || (!t.is_empty() && match_segment_chars(p, &t[1..]))
        }
        (Some(b'?'), Some(_)) => match_segment_chars(&p[1..], &t[1..]),
        (Some(b'['), Some(&tc)) => {
            if let Some(end) = p[1..].iter().position(|&b| b == b']') {
                let class = &p[1..=end];
                let (neg, chars) = if let Some(rest) = class.strip_prefix(b"!") {
                    (true, rest)
                } else {
                    (false, class)
                };
                let matched = class_matches(chars, tc);
                (matched != neg) && match_segment_chars(&p[end + 2..], &t[1..])
            } else {
                tc == b'[' && match_segment_chars(&p[1..], &t[1..])
            }
        }
        (Some(&pc), Some(&tc)) if pc == tc => match_segment_chars(&p[1..], &t[1..]),
        _ => false,
    }
}

fn class_matches(class: &[u8], ch: u8) -> bool {
    let mut i = 0;
    while i < class.len() {
        if i + 2 < class.len() && class[i + 1] == b'-' {
            if ch >= class[i] && ch <= class[i + 2] {
                return true;
            }
            i += 3;
        } else {
            if ch == class[i] {
                return true;
            }
            i += 1;
        }
    }
    false
}
