use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use crate::{Input, InputError};

pub fn sha256(bytes: &[u8]) -> String {
    let bytes = Sha256::digest(bytes);
    bytes.iter().fold(String::with_capacity(64), |mut out, b| {
        use std::fmt::Write;
        write!(out, "{b:02x}").expect("writing to a string");
        out
    })
}

/// The only side-input operation available to a backend.
pub trait Inputs {
    fn read(&mut self, id: &str) -> Result<Vec<u8>, InputError>;
}

/// A manifest checked against the actual content, equally usable by native
/// backends and by the component host. Every declared input must be available;
/// otherwise availability could change the output without changing its key.
/// Host paths never reach the guest.
pub struct DeclaredInputs {
    contents: BTreeMap<String, Vec<u8>>,
}

impl DeclaredInputs {
    pub fn new(
        manifest: &[Input],
        mut read: impl FnMut(&str) -> Option<Vec<u8>>,
    ) -> Result<Self, String> {
        let mut contents = BTreeMap::new();
        for input in manifest {
            if input.id.is_empty() || contents.contains_key(&input.id) {
                return Err(format!("invalid or duplicate input id `{}`", input.id));
            }
            let bytes = read(&input.id)
                .ok_or_else(|| format!("declared input `{}` is unavailable", input.id))?;
            if u64::try_from(bytes.len()).ok() != Some(input.size) || sha256(&bytes) != input.sha256
            {
                return Err(format!("input digest or size mismatch for `{}`", input.id));
            }
            contents.insert(input.id.clone(), bytes);
        }
        Ok(Self { contents })
    }
}

impl Inputs for DeclaredInputs {
    fn read(&mut self, id: &str) -> Result<Vec<u8>, InputError> {
        match self.contents.get(id) {
            Some(bytes) => Ok(bytes.clone()),
            None => Err(InputError::Undeclared),
        }
    }
}
