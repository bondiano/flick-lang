//! A compiled program as a file: its modules, the source files their spans
//! point into, and the version of the IR they are written in.
//!
//! The version follows semver. Before 1.0, any change a reader of an older
//! version could misread bumps the minor version, and a reader takes only
//! its own minor version; from 1.0, its own major version.

use std::collections::BTreeMap;
use std::{error, fmt};

#[cfg(feature = "cbor")]
use cbor4ii::core::dec;
#[cfg(feature = "cbor")]
use cbor4ii::core::error::Never;
#[cfg(feature = "cbor")]
use cbor4ii::serde::{DecodeError, Deserializer};
#[cfg(feature = "cbor")]
use ciborium::de::from_reader_with_recursion_limit;
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize};

use crate::ty::wire::{
    DecodeGuard, EncodeGuard, WireIrType, collect_table, decode_table, resolve_deferred_modules,
};
use crate::{FileId, LangItem, Module};

/// The version of the IR this crate reads and writes.
pub const VERSION: &str = "0.15.0";

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub ir_version: String,
    /// The compiler that wrote it.
    pub compiler: String,
    /// The source files the spans point into: a span's `file` is its index
    /// here.
    pub files: Vec<String>,
    pub modules: Vec<Module>,
    /// Where the modules declare each lang item, in the order of its role.
    pub lang: Vec<LangItem>,
}

impl Serialize for Program {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let (table, ids) = collect_table(&self.modules);
        let len = if table.is_empty() { 5 } else { 6 };
        let mut state = serializer.serialize_struct("Program", len)?;
        state.serialize_field("ir_version", &self.ir_version)?;
        state.serialize_field("compiler", &self.compiler)?;
        state.serialize_field("files", &self.files)?;
        if !table.is_empty() {
            state.serialize_field("types", &table)?;
        }
        let guard = (!table.is_empty()).then(|| EncodeGuard::enter(ids));
        state.serialize_field("modules", &self.modules)?;
        drop(guard);
        state.serialize_field("lang", &self.lang)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for Program {
    #[allow(clippy::too_many_lines)]
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            IrVersion,
            Compiler,
            Files,
            Types,
            Modules,
            Lang,
        }

        struct ProgramVisitor;

        impl<'de> Visitor<'de> for ProgramVisitor {
            type Value = Program;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("struct Program")
            }

            fn visit_seq<V: SeqAccess<'de>>(self, mut seq: V) -> Result<Program, V::Error> {
                let ir_version = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let compiler = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                let files = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(2, &self))?;
                let modules = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(3, &self))?;
                let lang = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(4, &self))?;
                Ok(Program {
                    ir_version,
                    compiler,
                    files,
                    modules,
                    lang,
                })
            }

            fn visit_map<V: MapAccess<'de>>(self, mut map: V) -> Result<Program, V::Error> {
                let mut ir_version = None;
                let mut compiler = None;
                let mut files = None;
                let mut modules: Option<Vec<Module>> = None;
                let mut lang = None;
                let mut saw_types = false;
                let mut deferred_table: Option<Vec<crate::TypeRef>> = None;
                let decode_guard = DecodeGuard::enter_deferred();

                while let Some(key) = map.next_key()? {
                    match key {
                        Field::IrVersion => {
                            if ir_version.is_some() {
                                return Err(de::Error::duplicate_field("ir_version"));
                            }
                            ir_version = Some(map.next_value()?);
                        }
                        Field::Compiler => {
                            if compiler.is_some() {
                                return Err(de::Error::duplicate_field("compiler"));
                            }
                            compiler = Some(map.next_value()?);
                        }
                        Field::Files => {
                            if files.is_some() {
                                return Err(de::Error::duplicate_field("files"));
                            }
                            files = Some(map.next_value()?);
                        }
                        Field::Types => {
                            if saw_types {
                                return Err(de::Error::duplicate_field("types"));
                            }
                            saw_types = true;
                            let wire_types: Vec<WireIrType> = map.next_value()?;
                            let resolved = decode_table(wire_types)?;
                            if let Some(table) = DecodeGuard::set_ready(resolved) {
                                deferred_table = Some(table);
                            }
                        }
                        Field::Modules => {
                            if modules.is_some() {
                                return Err(de::Error::duplicate_field("modules"));
                            }
                            modules = Some(map.next_value()?);
                        }
                        Field::Lang => {
                            if lang.is_some() {
                                return Err(de::Error::duplicate_field("lang"));
                            }
                            lang = Some(map.next_value()?);
                        }
                    }
                }
                let deferred = DecodeGuard::take_deferred();
                drop(decode_guard);

                let mut modules = modules.ok_or_else(|| de::Error::missing_field("modules"))?;
                if !deferred.is_empty() {
                    let table = deferred_table.unwrap_or_default();
                    resolve_deferred_modules(&mut modules, &deferred, &table)?;
                }

                Ok(Program {
                    ir_version: ir_version.ok_or_else(|| de::Error::missing_field("ir_version"))?,
                    compiler: compiler.ok_or_else(|| de::Error::missing_field("compiler"))?,
                    files: files.ok_or_else(|| de::Error::missing_field("files"))?,
                    modules,
                    lang: lang.ok_or_else(|| de::Error::missing_field("lang"))?,
                })
            }
        }

        const FIELDS: &[&str] = &[
            "ir_version",
            "compiler",
            "files",
            "types",
            "modules",
            "lang",
        ];
        deserializer.deserialize_struct("Program", FIELDS, ProgramVisitor)
    }
}

impl Program {
    /// `modules` of a compilation whose files `path` names, their spans moved
    /// to indices of the program's list of files, and the lang items `lang`
    /// of the compilation.
    pub fn new(
        compiler: &str,
        mut modules: Vec<Module>,
        mut lang: Vec<LangItem>,
        path: &dyn Fn(FileId) -> String,
    ) -> Program {
        let mut index: BTreeMap<FileId, u32> = BTreeMap::new();
        let mut files = Vec::new();
        for module in &mut modules {
            module.each_span_mut(&mut |span| {
                let next = u32::try_from(index.len()).expect("fewer than 2^32 files");
                let at = *index.entry(span.file).or_insert_with(|| {
                    files.push(path(span.file));
                    next
                });
                span.file = FileId::new(at);
            });
        }
        lang.sort_by(|a, b| a.lang.cmp(&b.lang));
        Program {
            ir_version: VERSION.to_owned(),
            compiler: compiler.to_owned(),
            files,
            modules,
            lang,
        }
    }

    #[cfg(feature = "json")]
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("the IR serializes")
    }

    #[cfg(feature = "json")]
    pub fn from_json(text: &str) -> Result<Program, Error> {
        let program: Program =
            serde_json::from_str(text).map_err(|e| Error::Format(e.to_string()))?;
        program.checked()
    }

    #[cfg(feature = "cbor")]
    pub fn to_cbor(&self) -> Vec<u8> {
        let mut out = Vec::new();
        ciborium::into_writer(self, &mut out).expect("the IR serializes");
        out
    }

    #[cfg(feature = "cbor")]
    pub fn from_cbor(bytes: &[u8]) -> Result<Program, Error> {
        // Borrow map keys directly from the input instead of copying them
        // through a streaming decoder. Keep the existing reader for CBOR forms
        // outside the slice decoder's Serde representation.
        let program: Program = match from_slice(bytes) {
            Ok(program) => program,
            Err(_) => from_reader_with_recursion_limit(bytes, DEPTH)
                .map_err(|e| Error::Format(e.to_string()))?,
        };
        program.checked()
    }

    #[cfg(any(feature = "json", feature = "cbor"))]
    fn checked(self) -> Result<Program, Error> {
        if compatible(&self.ir_version) {
            Ok(self)
        } else {
            Err(Error::Version {
                found: self.ir_version,
            })
        }
    }
}

/// Whether a reader of [`VERSION`] reads the IR of version `found`.
#[cfg(any(feature = "json", feature = "cbor"))]
fn compatible(found: &str) -> bool {
    let parts = |v: &str| -> Option<(u64, u64)> {
        let mut it = v.split('.').map(str::parse::<u64>);
        let (major, minor) = (it.next()?.ok()?, it.next()?.ok()?);
        it.next()?.ok()?;
        Some((major, minor))
    };
    match (parts(VERSION), parts(found)) {
        (Some((0, minor)), Some((0, found))) => minor == found,
        (Some((major, _)), Some((found, _))) => major != 0 && major == found,
        _ => false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Not a program in this form.
    Format(String),
    /// A program of a version this reader does not read.
    Version { found: String },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Format(e) => write!(f, "not a program of the IR: {e}"),
            Error::Version { found } => write!(
                f,
                "the IR of version {found} is not readable by a reader of version {VERSION}"
            ),
        }
    }
}

impl error::Error for Error {}

/// How deep the CBOR of a program may nest. The decoders stop at 256
/// levels by default, which a long chain of statements exceeds; a program
/// the checker accepts must reach its backend, so only the stack that
/// decodes it bounds it.
#[cfg(feature = "cbor")]
const DEPTH: usize = 1 << 16;

/// `cbor4ii`'s slice reader, but nesting [`DEPTH`] levels deep.
#[cfg(feature = "cbor")]
struct DeepSlice<'a> {
    buf: &'a [u8],
    left: usize,
}

#[cfg(feature = "cbor")]
impl<'de> dec::Read<'de> for DeepSlice<'de> {
    type Error = Never;

    fn fill<'b>(&'b mut self, want: usize) -> Result<dec::Reference<'de, 'b>, Self::Error> {
        Ok(dec::Reference::Long(&self.buf[..want.min(self.buf.len())]))
    }

    fn advance(&mut self, n: usize) {
        self.buf = &self.buf[n.min(self.buf.len())..];
    }

    fn step_in(&mut self) -> bool {
        self.left.checked_sub(1).is_some_and(|left| {
            self.left = left;
            true
        })
    }

    fn step_out(&mut self) {
        self.left += 1;
    }
}

#[cfg(feature = "cbor")]
fn from_slice<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, DecodeError<Never>> {
    let reader = DeepSlice {
        buf: bytes,
        left: DEPTH,
    };
    T::deserialize(&mut Deserializer::new(reader))
}

#[cfg(all(test, feature = "json"))]
mod tests;
