//! Places in source files.

use std::ops;

use serde::{Deserialize, Serialize};

/// A source file of one compilation, by its index in the compiler's list.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct FileId(u32);

impl FileId {
    pub fn new(index: u32) -> Self {
        FileId(index)
    }

    pub fn index(self) -> u32 {
        self.0
    }
}

/// Half-open UTF-8 byte range `start..end` in one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Span {
    pub file: FileId,
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(file: FileId, start: u32, end: u32) -> Self {
        debug_assert!(start <= end);
        Span { file, start, end }
    }

    pub fn range(self) -> ops::Range<usize> {
        self.start as usize..self.end as usize
    }
}
