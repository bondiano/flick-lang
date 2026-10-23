//! The build cache and the artifact manifest. A cache key hashes every
//! input a build depends on, each labeled, so that two different sets of
//! inputs never share a key by concatenation. A cache miss only costs time:
//! the cached artifacts are exactly what the build would write.

use serde::{Deserialize, Serialize};

/// Directory of the cache, the run generations and the manifest.
pub const DIR: &str = ".flick";

/// The artifacts a build wrote, relative to its output directory, so that
/// `flick clean` removes them and nothing else.
pub const MANIFEST: &str = ".flick/artifacts.json";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Output directories, relative to the project root, each with its files.
    pub outputs: Vec<(String, Vec<String>)>,
}

impl Manifest {
    /// Records the files of one output directory, replacing what an earlier
    /// build recorded for it.
    pub fn record(&mut self, out_dir: &str, files: Vec<String>) {
        self.outputs.retain(|(dir, _)| dir != out_dir);
        self.outputs.push((out_dir.to_owned(), files));
        self.outputs.sort();
    }
}

/// Inputs of a cache key, each with a label naming what it is.
#[derive(Debug, Default)]
pub struct Key {
    parts: Vec<(String, Vec<u8>)>,
}

impl Key {
    pub fn add(&mut self, label: impl Into<String>, bytes: impl Into<Vec<u8>>) {
        self.parts.push((label.into(), bytes.into()));
    }

    /// 128 bits of two FNV-1a hashes over the sorted, length-prefixed parts.
    pub fn finish(mut self) -> String {
        self.parts.sort();
        let mut a = Fnv(0xcbf2_9ce4_8422_2325);
        let mut b = Fnv(0x6c62_272e_07bb_0142);
        for (label, bytes) in &self.parts {
            for chunk in [label.as_bytes(), bytes.as_slice()] {
                let len = (chunk.len() as u64).to_le_bytes();
                for h in [&mut a, &mut b] {
                    h.write(&len);
                    h.write(chunk);
                }
            }
        }
        format!("{:016x}{:016x}", a.0, b.0)
    }
}

struct Fnv(u64);

impl Fnv {
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(parts: &[(&str, &str)]) -> String {
        let mut k = Key::default();
        for (label, bytes) in parts {
            k.add(*label, bytes.as_bytes());
        }
        k.finish()
    }

    #[test]
    fn a_key_follows_every_input_and_not_their_order() {
        let base = key(&[("config", "a"), ("profile", "debug")]);
        assert_eq!(base, key(&[("profile", "debug"), ("config", "a")]));
        assert_ne!(base, key(&[("config", "a"), ("profile", "release")]));
        assert_ne!(base, key(&[("config", "ab"), ("profile", "debug")]));
        // moving a byte between parts changes the key
        assert_ne!(
            key(&[("x", "ab"), ("y", "")]),
            key(&[("x", "a"), ("y", "b")])
        );
    }

    #[test]
    fn the_manifest_keeps_one_entry_per_output() {
        let mut m = Manifest::default();
        m.record("dist", vec!["a.js".into()]);
        m.record("dist", vec!["b.js".into()]);
        assert_eq!(
            m.outputs,
            vec![("dist".to_owned(), vec!["b.js".to_owned()])]
        );
    }
}
