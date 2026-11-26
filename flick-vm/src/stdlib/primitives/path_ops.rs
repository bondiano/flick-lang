//! Counted POSIX `Path` primitives (`packages/std/src/Path.fl`), matching `runtime-go/flickrt/path.go`.

use std::env;

use flick_core::ir::Lit;
use flick_core::ir::memory::PathIntrinsic;

use super::list_ops::slice_reps;
use super::path_url::{path_from_file_url, path_to_file_url};
use super::{Error, Fuel, Heap, Value, charge};

pub(super) fn run(
    heap: &mut Heap,
    op: PathIntrinsic,
    args: &[Value],
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    charge(fuel, 1)?;
    match op {
        PathIntrinsic::Basename => {
            let p = heap.string_lossy(&args[0])?;
            heap.literal(&Lit::String(path_basename(&p, "")))
        }
        PathIntrinsic::BasenameWithout => {
            let ext = heap.string_lossy(&args[0])?;
            let p = heap.string_lossy(&args[1])?;
            heap.literal(&Lit::String(path_basename(&p, &ext)))
        }
        PathIntrinsic::Dirname => {
            let p = heap.string_lossy(&args[0])?;
            heap.literal(&Lit::String(path_dirname(&p)))
        }
        PathIntrinsic::Extname => {
            let p = heap.string_lossy(&args[0])?;
            heap.literal(&Lit::String(path_extname(&p)))
        }
        PathIntrinsic::Format => {
            let root = heap.string_lossy(heap.named_field(&args[0], "root")?)?;
            let dir = heap.string_lossy(heap.named_field(&args[0], "dir")?)?;
            let base = heap.string_lossy(heap.named_field(&args[0], "base")?)?;
            let name = heap.string_lossy(heap.named_field(&args[0], "name")?)?;
            let ext = heap.string_lossy(heap.named_field(&args[0], "ext")?)?;
            heap.literal(&Lit::String(path_format(&root, &dir, &base, &name, &ext)))
        }
        PathIntrinsic::FromFileUrl => {
            let url = heap.string_lossy(&args[0])?;
            match path_from_file_url(&url) {
                Ok(path) => {
                    let sv = heap.literal(&Lit::String(path))?;
                    heap.construct("Prelude.Result.Ok", vec![sv])
                }
                Err(desc) => bad_argument_err(heap, "fromFileUrl", &desc),
            }
        }
        PathIntrinsic::IsAbsolute => {
            let p = heap.string_lossy(&args[0])?;
            Ok(Value::Bool(p.starts_with('/')))
        }
        PathIntrinsic::Join => {
            let parts = string_list(heap, &args[0], fuel)?;
            heap.literal(&Lit::String(path_join(&parts)))
        }
        PathIntrinsic::Normalize => {
            let p = heap.string_lossy(&args[0])?;
            heap.literal(&Lit::String(path_normalize(&p)))
        }
        PathIntrinsic::Parse => {
            let p = heap.string_lossy(&args[0])?;
            let (root, dir, base, ext, name) = path_parse(&p);
            let r_val = heap.literal(&Lit::String(root))?;
            let d_val = heap.literal(&Lit::String(dir))?;
            let b_val = heap.literal(&Lit::String(base))?;
            let e_val = heap.literal(&Lit::String(ext))?;
            let n_val = heap.literal(&Lit::String(name))?;
            heap.record(vec![
                ("root".into(), r_val),
                ("dir".into(), d_val),
                ("base".into(), b_val),
                ("ext".into(), e_val),
                ("name".into(), n_val),
            ])
        }
        PathIntrinsic::Relative => {
            let from = heap.string_lossy(&args[0])?;
            let to = heap.string_lossy(&args[1])?;
            heap.literal(&Lit::String(path_relative(&from, &to)))
        }
        PathIntrinsic::Resolve => {
            let parts = string_list(heap, &args[0], fuel)?;
            heap.literal(&Lit::String(path_resolve(&parts)))
        }
        PathIntrinsic::ResolveWithin => {
            let root = heap.string_lossy(&args[0])?;
            let input = heap.string_lossy(&args[1])?;
            match path_resolve_within(&root, &input) {
                Some(path) => {
                    let sv = heap.literal(&Lit::String(path))?;
                    heap.construct("Prelude.Option.Some", vec![sv])
                }
                None => heap.construct("Prelude.Option.None", vec![]),
            }
        }
        PathIntrinsic::ToFileUrl => {
            let p = heap.string_lossy(&args[0])?;
            let url = path_to_file_url(&p);
            let sv = heap.literal(&Lit::String(url))?;
            heap.construct("Prelude.Result.Ok", vec![sv])
        }
        PathIntrinsic::ToNamespacedPath => heap.dup(&args[0]),
    }
}

fn string_list(heap: &Heap, val: &Value, fuel: Fuel<'_>) -> Result<Vec<String>, Error> {
    let reps = slice_reps(heap, val)?;
    charge(fuel, reps.len())?;
    let mut out = Vec::with_capacity(reps.len());
    for item in &reps {
        out.push(heap.string_lossy(item)?);
    }
    Ok(out)
}

fn bad_argument_err(heap: &mut Heap, method: &str, description: &str) -> Result<Value, Error> {
    let module_v = heap.literal(&Lit::String("Path".into()))?;
    let method_v = heap.literal(&Lit::String(method.into()))?;
    let desc_s = heap.literal(&Lit::String(description.into()))?;
    let desc_v = heap.construct("Prelude.Option.Some", vec![desc_s])?;
    let rec = heap.record(vec![
        ("module_".into(), module_v),
        ("method".into(), method_v),
        ("description".into(), desc_v),
    ])?;
    let reason = heap.construct("PlatformError.Reason.Bad", vec![rec])?;
    let err = heap.construct("PlatformError.PlatformError.PlatformError", vec![reason])?;
    heap.construct("Prelude.Result.Err", vec![err])
}

fn path_normalize_string(path: &str, allow_above_root: bool) -> String {
    let bytes = path.as_bytes();
    let mut res = String::new();
    let mut last_segment: usize = 0;
    let mut last_slash: isize = -1;
    let mut dots: isize = 0;
    let mut code: u8 = 0;
    for i in 0..=bytes.len() {
        if i < bytes.len() {
            code = bytes[i];
        } else if code == b'/' {
            break;
        } else {
            code = b'/';
        }
        if code != b'/' {
            if code == b'.' && dots != -1 {
                dots += 1;
            } else {
                dots = -1;
            }
            continue;
        }
        let i_signed = isize::try_from(i).unwrap_or(isize::MAX);
        if last_slash == i_signed - 1 || dots == 1 {
            // noop
        } else if dots == 2 {
            if res.len() < 2 || last_segment != 2 || !res.ends_with("..") {
                if res.len() > 2 {
                    let last = res.rfind('/');
                    if last != Some(res.len() - 1) {
                        if let Some(pos) = last {
                            res.truncate(pos);
                            last_segment = res
                                .rfind('/')
                                .map_or(res.len(), |prev| res.len() - 1 - prev);
                        } else {
                            res.clear();
                            last_segment = 0;
                        }
                        last_slash = i_signed;
                        dots = 0;
                        continue;
                    }
                } else if res.len() == 2 || res.len() == 1 {
                    res.clear();
                    last_segment = 0;
                    last_slash = i_signed;
                    dots = 0;
                    continue;
                }
            }
            if allow_above_root {
                if res.is_empty() {
                    res.push_str("..");
                } else {
                    res.push_str("/..");
                }
                last_segment = 2;
            }
        } else {
            let start = usize::try_from(last_slash + 1).unwrap_or(0);
            if !res.is_empty() {
                res.push('/');
            }
            res.push_str(&path[start..i]);
            last_segment = i - start;
        }
        last_slash = i_signed;
        dots = 0;
    }
    res
}

pub(super) fn path_resolve(paths: &[String]) -> String {
    let mut resolved = String::new();
    let mut absolute = false;
    let mut idx = isize::try_from(paths.len()).unwrap_or(0) - 1;
    while idx >= -1 && !absolute {
        let owned;
        let p: &str = if idx >= 0 {
            &paths[usize::try_from(idx).unwrap_or(0)]
        } else {
            owned = env::current_dir()
                .ok()
                .and_then(|d| d.into_os_string().into_string().ok())
                .unwrap_or_default();
            &owned
        };
        idx -= 1;
        if p.is_empty() {
            continue;
        }
        resolved = format!("{p}/{resolved}");
        absolute = p.starts_with('/');
    }
    let normalized = path_normalize_string(&resolved, !absolute);
    if absolute {
        format!("/{normalized}")
    } else if !normalized.is_empty() {
        normalized
    } else {
        ".".into()
    }
}

fn path_normalize(path: &str) -> String {
    if path.is_empty() {
        return ".".into();
    }
    let absolute = path.starts_with('/');
    let trailing = path.ends_with('/');
    let mut out = path_normalize_string(path, !absolute);
    if out.is_empty() && !absolute {
        out.push('.');
    }
    if !out.is_empty() && trailing {
        out.push('/');
    }
    if absolute { format!("/{out}") } else { out }
}

fn path_join(paths: &[String]) -> String {
    let mut joined = String::new();
    let mut any = false;
    for p in paths {
        if p.is_empty() {
            continue;
        }
        if any {
            joined.push('/');
            joined.push_str(p);
        } else {
            joined.push_str(p);
            any = true;
        }
    }
    if any {
        path_normalize(&joined)
    } else {
        ".".into()
    }
}

fn path_root_end(p: &str) -> usize {
    let bytes = p.as_bytes();
    let mut i = 1;
    while i < bytes.len() && bytes[i] == b'/' {
        i += 1;
    }
    i
}

fn path_relative(from: &str, to: &str) -> String {
    if from == to {
        return String::new();
    }
    let from = path_resolve(&[from.to_owned()]);
    let to = path_resolve(&[to.to_owned()]);
    if from == to {
        return String::new();
    }
    let from_start = path_root_end(&from);
    let mut to_start = path_root_end(&to);
    let from_len = from.len() - from_start;
    let to_len = to.len() - to_start;
    let length = from_len.min(to_len);
    let fb = from.as_bytes();
    let tb = to.as_bytes();
    let mut last_common_sep: isize = -1;
    for i in 0..=length {
        if i == length {
            if to_len > length {
                if tb[to_start + i] == b'/' {
                    return to[to_start + i + 1..].to_owned();
                } else if i == 0 {
                    return to[to_start + i..].to_owned();
                }
            } else if from_len > length {
                if fb[from_start + i] == b'/' {
                    last_common_sep = isize::try_from(i).unwrap_or(0);
                } else if i == 0 {
                    last_common_sep = 0;
                }
            }
            break;
        }
        if fb[from_start + i] != tb[to_start + i] {
            break;
        } else if fb[from_start + i] == b'/' {
            last_common_sep = isize::try_from(i).unwrap_or(0);
        }
    }
    let mut out = String::new();
    let start_up = usize::try_from(isize::try_from(from_start).unwrap_or(0) + last_common_sep + 1)
        .unwrap_or(0);
    if start_up <= from.len() {
        let mut push_up = || {
            if out.is_empty() {
                out.push_str("..");
            } else {
                out.push_str("/..");
            }
        };
        for &b in &fb[start_up..] {
            if b == b'/' {
                push_up();
            }
        }
        push_up();
    }
    let to_off =
        usize::try_from(isize::try_from(to_start).unwrap_or(0) + last_common_sep).unwrap_or(0);
    if !out.is_empty() {
        out.push_str(&to[to_off..]);
        return out;
    }
    to_start = to_off;
    if to_start < to.len() && tb[to_start] == b'/' {
        to_start += 1;
    }
    to[to_start..].to_owned()
}

fn path_dirname(path: &str) -> String {
    if path.is_empty() {
        return ".".into();
    }
    let bytes = path.as_bytes();
    let has_root = bytes[0] == b'/';
    let mut end: isize = -1;
    let mut matched_slash = true;
    for i in (1..bytes.len()).rev() {
        if bytes[i] == b'/' {
            if !matched_slash {
                end = isize::try_from(i).unwrap_or(-1);
                break;
            }
        } else {
            matched_slash = false;
        }
    }
    if end == -1 && has_root {
        "/".into()
    } else if end == -1 {
        ".".into()
    } else if has_root && end == 1 {
        "//".into()
    } else {
        path[..usize::try_from(end).unwrap_or(0)].to_owned()
    }
}

fn path_basename(path: &str, ext: &str) -> String {
    let pb = path.as_bytes();
    let mut start = 0usize;
    let mut end: isize = -1;
    let mut matched_slash = true;
    if !ext.is_empty() && ext.len() <= path.len() {
        if ext == path {
            return String::new();
        }
        let eb = ext.as_bytes();
        let mut ext_idx = isize::try_from(eb.len()).unwrap_or(0) - 1;
        let mut first_non_slash_end: isize = -1;
        for i in (0..pb.len()).rev() {
            if pb[i] == b'/' {
                if !matched_slash {
                    start = i + 1;
                    break;
                }
                continue;
            }
            if first_non_slash_end == -1 {
                matched_slash = false;
                first_non_slash_end = isize::try_from(i + 1).unwrap_or(-1);
            }
            if ext_idx >= 0 {
                if pb[i] == eb[usize::try_from(ext_idx).unwrap_or(0)] {
                    ext_idx -= 1;
                    if ext_idx == -1 {
                        end = isize::try_from(i).unwrap_or(-1);
                    }
                } else {
                    ext_idx = -1;
                    end = first_non_slash_end;
                }
            }
        }
        if isize::try_from(start).unwrap_or(-2) == end {
            end = first_non_slash_end;
        } else if end == -1 {
            end = isize::try_from(path.len()).unwrap_or(0);
        }
        return path[start..usize::try_from(end).unwrap_or(0)].to_owned();
    }
    for i in (0..pb.len()).rev() {
        if pb[i] == b'/' {
            if !matched_slash {
                start = i + 1;
                break;
            }
        } else if end == -1 {
            matched_slash = false;
            end = isize::try_from(i + 1).unwrap_or(-1);
        }
    }
    if end == -1 {
        String::new()
    } else {
        path[start..usize::try_from(end).unwrap_or(0)].to_owned()
    }
}

fn path_scan(path: &str, start: usize) -> (usize, isize, isize) {
    let pb = path.as_bytes();
    let mut start_part = 0usize;
    let mut start_dot: isize = -1;
    let mut end: isize = -1;
    let mut matched_slash = true;
    let mut pre_dot_state = 0i32;
    for i in (start..pb.len()).rev() {
        let code = pb[i];
        if code == b'/' {
            if !matched_slash {
                start_part = i + 1;
                break;
            }
            continue;
        }
        if end == -1 {
            matched_slash = false;
            end = isize::try_from(i + 1).unwrap_or(-1);
        }
        if code == b'.' {
            if start_dot == -1 {
                start_dot = isize::try_from(i).unwrap_or(-1);
            } else if pre_dot_state != 1 {
                pre_dot_state = 1;
            }
        } else if start_dot != -1 {
            pre_dot_state = -1;
        }
    }
    let sp = isize::try_from(start_part).unwrap_or(0);
    if start_dot == -1
        || end == -1
        || pre_dot_state == 0
        || (pre_dot_state == 1 && start_dot == end - 1 && start_dot == sp + 1)
    {
        start_dot = -1;
    }
    (start_part, start_dot, end)
}

fn path_extname(path: &str) -> String {
    let (_, dot, end) = path_scan(path, 0);
    if dot == -1 {
        String::new()
    } else {
        path[usize::try_from(dot).unwrap_or(0)..usize::try_from(end).unwrap_or(0)].to_owned()
    }
}

fn path_parse(path: &str) -> (String, String, String, String, String) {
    let (mut root, mut dir, mut base, mut ext, mut name) = (
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    );
    if !path.is_empty() {
        let absolute = path.starts_with('/');
        let start = if absolute {
            root = "/".into();
            1
        } else {
            0
        };
        let (start_part, dot, end) = path_scan(path, start);
        let from = if start_part == 0 && absolute {
            1
        } else {
            start_part
        };
        if dot == -1 {
            if end != -1 {
                let s = &path[from..usize::try_from(end).unwrap_or(0)];
                s.clone_into(&mut base);
                s.clone_into(&mut name);
            }
        } else {
            let d = usize::try_from(dot).unwrap_or(0);
            let e = usize::try_from(end).unwrap_or(0);
            path[from..d].clone_into(&mut name);
            path[from..e].clone_into(&mut base);
            path[d..e].clone_into(&mut ext);
        }
        if start_part > 0 {
            path[..start_part - 1].clone_into(&mut dir);
        } else if absolute {
            dir = "/".into();
        }
    }
    (root, dir, base, ext, name)
}

fn path_format(root: &str, dir: &str, base: &str, name: &str, ext: &str) -> String {
    let dir = if dir.is_empty() { root } else { dir };
    let owned_base;
    let base = if base.is_empty() {
        owned_base = format!("{name}{ext}");
        &owned_base
    } else {
        base
    };
    if dir.is_empty() {
        base.to_owned()
    } else if dir == root {
        format!("{dir}{base}")
    } else {
        format!("{dir}/{base}")
    }
}

fn path_resolve_within(root: &str, input: &str) -> Option<String> {
    let (r, _, _, _, _) = path_parse(input);
    if !r.is_empty() {
        return None;
    }
    let base = path_resolve(&[root.to_owned()]);
    let candidate = path_resolve(&[base.clone(), input.to_owned()]);
    let rel = path_relative(&base, &candidate);
    if rel == ".." || rel.starts_with("../") || rel.starts_with('/') {
        None
    } else {
        Some(candidate)
    }
}
