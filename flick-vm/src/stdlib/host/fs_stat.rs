//! `FileSystem.Info` metadata conversion and `OpenFlag` option mapping.

use std::fs::{Metadata, OpenOptions};
use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

use flick_core::ir::Lit;

use crate::tier0::{Data, Error, Heap, Value};

use super::error_map::{opt_none, opt_some};

pub(super) fn open_options_from_flag(
    flag_tag: &str,
    mode: Option<i32>,
    no_follow: bool,
) -> (OpenOptions, bool) {
    let mut opts = OpenOptions::new();
    let mut is_append = false;
    match flag_tag {
        "FileSystem.OpenFlag.Read" => {
            opts.read(true);
        }
        "FileSystem.OpenFlag.ReadWrite" => {
            opts.read(true).write(true);
        }
        "FileSystem.OpenFlag.WriteExclusive" => {
            opts.write(true).create_new(true).truncate(true);
        }
        "FileSystem.OpenFlag.WriteRead" => {
            opts.read(true).write(true).create(true).truncate(true);
        }
        "FileSystem.OpenFlag.WriteReadExclusive" => {
            opts.read(true).write(true).create_new(true).truncate(true);
        }
        "FileSystem.OpenFlag.Append" => {
            opts.append(true).create(true);
            is_append = true;
        }
        "FileSystem.OpenFlag.AppendExclusive" => {
            opts.append(true).create_new(true);
            is_append = true;
        }
        "FileSystem.OpenFlag.AppendRead" => {
            opts.read(true).append(true).create(true);
            is_append = true;
        }
        "FileSystem.OpenFlag.AppendReadExclusive" => {
            opts.read(true).append(true).create_new(true);
            is_append = true;
        }
        _ => {
            opts.write(true).create(true).truncate(true);
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        if let Some(m) = mode
            && m >= 0
        {
            #[expect(clippy::cast_sign_loss, reason = "checked m >= 0")]
            opts.mode(m as u32);
        }
        if no_follow {
            #[cfg(target_os = "linux")]
            const O_NOFOLLOW: i32 = 0x20000;
            #[cfg(target_os = "macos")]
            const O_NOFOLLOW: i32 = 0x0100;
            #[cfg(not(any(target_os = "linux", target_os = "macos")))]
            const O_NOFOLLOW: i32 = 0x20000;
            opts.custom_flags(O_NOFOLLOW);
        }
    }
    #[cfg(not(unix))]
    let _ = (mode, no_follow);
    (opts, is_append)
}

fn system_time_ms(t: io::Result<SystemTime>) -> Option<f64> {
    let time = t.ok()?;
    match time.duration_since(UNIX_EPOCH) {
        Ok(d) =>
        {
            #[expect(clippy::cast_precision_loss, reason = "epoch ms fits f64")]
            Some(d.as_millis() as f64)
        }
        Err(e) =>
        {
            #[expect(clippy::cast_precision_loss, reason = "epoch ms fits f64")]
            Some(-(e.duration().as_millis() as f64))
        }
    }
}

pub(super) fn stat_info_from_metadata(heap: &mut Heap, meta: &Metadata) -> Result<Value, Error> {
    let ft = meta.file_type();
    let type_tag = if ft.is_file() {
        "RegularFile"
    } else if ft.is_dir() {
        "Directory"
    } else if ft.is_symlink() {
        "SymbolicLink"
    } else {
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileTypeExt as _;
            if ft.is_block_device() {
                "BlockDevice"
            } else if ft.is_char_device() {
                "CharacterDevice"
            } else if ft.is_fifo() {
                "Fifo"
            } else if ft.is_socket() {
                "Socket"
            } else {
                "UnknownType"
            }
        }
        #[cfg(not(unix))]
        {
            "UnknownType"
        }
    };

    let atime_v = match system_time_ms(meta.accessed()) {
        Some(ms) => opt_some(heap, Value::Float(ms))?,
        None => opt_none(heap)?,
    };
    let mtime_v = match system_time_ms(meta.modified()) {
        Some(ms) => opt_some(heap, Value::Float(ms))?,
        None => opt_none(heap)?,
    };
    let birth_v = match system_time_ms(meta.created()) {
        Some(ms) => opt_some(heap, Value::Float(ms))?,
        None => opt_none(heap)?,
    };
    let size_v = heap.literal(&Lit::BigInt(meta.len().to_string()))?;
    let type_v = heap.construct(&format!("FileSystem.FileType.{type_tag}"), vec![])?;

    #[cfg(unix)]
    let (blksize_v, blocks_v, dev_v, gid_v, ino_v, mode_i, nlink_v, rdev_v, uid_v) = {
        use std::os::unix::fs::MetadataExt as _;
        let bs = heap.literal(&Lit::BigInt(meta.blksize().to_string()))?;
        let blk = heap.literal(&Lit::BigInt(meta.blocks().to_string()))?;
        let dev = heap.literal(&Lit::BigInt(meta.dev().to_string()))?;
        let ino = heap.literal(&Lit::BigInt(meta.ino().to_string()))?;
        let nlink = heap.literal(&Lit::BigInt(meta.nlink().to_string()))?;
        let rdev = heap.literal(&Lit::BigInt(meta.rdev().to_string()))?;
        #[expect(clippy::cast_possible_wrap, reason = "Unix mode fits i32")]
        let mode_i = meta.mode() as i32;
        (
            opt_some(heap, bs)?,
            opt_some(heap, blk)?,
            dev,
            opt_some(heap, Value::Float(f64::from(meta.gid())))?,
            opt_some(heap, ino)?,
            mode_i,
            opt_some(heap, nlink)?,
            opt_some(heap, rdev)?,
            opt_some(heap, Value::Float(f64::from(meta.uid())))?,
        )
    };
    #[cfg(not(unix))]
    let (blksize_v, blocks_v, dev_v, gid_v, ino_v, mode_i, nlink_v, rdev_v, uid_v) = {
        let mode_i = if meta.is_dir() { 0o040_755 } else { 0o100_644 };
        (
            opt_none(heap)?,
            opt_none(heap)?,
            heap.literal(&Lit::BigInt("0".into()))?,
            opt_none(heap)?,
            opt_none(heap)?,
            mode_i,
            opt_none(heap)?,
            opt_none(heap)?,
            opt_none(heap)?,
        )
    };

    Ok(heap.allocate(Data::Record(vec![
        ("atime".into(), atime_v),
        ("birthtime".into(), birth_v),
        ("blksize".into(), blksize_v),
        ("blocks".into(), blocks_v),
        ("dev".into(), dev_v),
        ("gid".into(), gid_v),
        ("ino".into(), ino_v),
        ("mode".into(), Value::Int(mode_i)),
        ("mtime".into(), mtime_v),
        ("nlink".into(), nlink_v),
        ("rdev".into(), rdev_v),
        ("size".into(), size_v),
        ("type_".into(), type_v),
        ("uid".into(), uid_v),
    ])))
}
