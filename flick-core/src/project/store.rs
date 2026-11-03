//! Where Flick keeps what it computes or fetches outside a project: the
//! user's cache directory, and in it the package store, one directory per
//! fetched package, named by the hash of its files.

use std::env;
use std::path::PathBuf;

/// `flick` in the user's cache directory.
pub fn cache_dir() -> Option<PathBuf> {
    let dir = env::var_os("XDG_CACHE_HOME")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .or_else(|| env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(dir.join("flick"))
}

/// The package store: `FLICK_STORE`, else `pkg` in [`cache_dir`].
pub fn store_dir() -> Option<PathBuf> {
    env::var_os("FLICK_STORE")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .or_else(|| cache_dir().map(|d| d.join("pkg")))
}
