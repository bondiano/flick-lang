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
use serde::{Deserialize, Serialize};

use crate::{FileId, LangItem, Module};

/// The version of the IR this crate reads and writes.
pub const VERSION: &str = "0.13.0";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
mod tests {
    use std::collections::BTreeMap;
    use std::{mem, thread};

    use super::{Error, Program, VERSION, compatible};
    use crate::{Expr, ExprKind, FileId, Item, ItemKind, LangItem, Lit, Module, Namespace, Span};

    fn program(x: f64) -> Program {
        let span = Span::new(FileId::new(7), 1, 4);
        let body = Expr::new(ExprKind::Lit(Lit::Float(x)), span);
        let item = Item {
            kind: ItemKind::Value {
                name: "x".to_owned(),
                dicts: Vec::new(),
                body,
            },
            span,
        };
        let module = Module {
            name: "App".to_owned(),
            items: vec![item],
            labels: BTreeMap::default(),
            initialization: crate::Initialization::default(),
            records: BTreeMap::default(),
            package: None,
            exports: crate::Exports::default(),
        };
        let item = |lang: &str, namespace, name: &str| LangItem {
            lang: lang.to_owned(),
            module: "Prelude".to_owned(),
            namespace,
            name: name.to_owned(),
        };
        let lang = vec![
            item("some", Namespace::Constructor, "Just"),
            item("option", Namespace::Type, "Maybe"),
        ];
        Program::new("flick test", vec![module], lang, &|f| {
            format!("file{}.fl", f.index())
        })
    }

    #[test]
    fn the_lang_items_are_in_the_order_of_their_roles() {
        let p = program(1.0);
        let lang: Vec<&str> = p.lang.iter().map(|i| i.lang.as_str()).collect();
        assert_eq!(lang, ["option", "some"]);
    }

    #[test]
    fn spans_point_into_the_list_of_files() {
        let p = program(1.5);
        assert_eq!(p.files, ["file7.fl"]);
        assert_eq!(p.modules[0].items[0].span.file, FileId::new(0));
    }

    #[cfg(feature = "cbor")]
    #[test]
    fn the_slice_reader_accepts_the_writer_float_widths() {
        for value in [0.0, -0.0, 1.5, f64::INFINITY, f64::NAN, 0.1] {
            let expected = program(value);
            let decoded: Program = super::from_slice(&expected.to_cbor()).expect("writer CBOR");
            assert_eq!(decoded, expected);
        }
    }

    /// A program the checker accepts reaches a backend however deep its
    /// expressions nest; the decoders' defaults stop at 256 levels.
    #[cfg(feature = "cbor")]
    #[test]
    fn a_deeply_nested_expression_survives_cbor() {
        // the compiler checks and lowers such a program on a large stack too
        let deep = thread::Builder::new().stack_size(64 << 20);
        deep.spawn(deep_round_trip)
            .expect("a thread")
            .join()
            .expect("deep IR round-trips");
    }

    #[cfg(feature = "cbor")]
    fn deep_round_trip() {
        let mut expected = program(1.0);
        let ItemKind::Value { body, .. } = &mut expected.modules[0].items[0].kind else {
            unreachable!("the value of the program");
        };
        for _ in 0..1_000 {
            let span = body.span;
            let inner = mem::replace(body, Expr::new(ExprKind::Lit(Lit::Unit), span));
            *body = Expr::new(ExprKind::Yield(Box::new(inner)), span);
        }
        let decoded = Program::from_cbor(&expected.to_cbor()).expect("deep IR decodes");
        assert_eq!(decoded, expected);
    }

    #[test]
    fn floats_survive_both_forms() {
        for x in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            -0.0,
            0.1 + 0.2,
            1e300,
        ] {
            let p = program(x);
            assert_eq!(Program::from_json(&p.to_json()), Ok(p.clone()), "{x}");
            #[cfg(feature = "cbor")]
            assert_eq!(Program::from_cbor(&p.to_cbor()), Ok(p), "{x}");
        }
    }

    #[cfg(feature = "cbor")]
    #[test]
    fn cbor_keeps_accepting_tagged_programs_and_trailing_input() {
        let expected = program(1.5);
        let bytes = expected.to_cbor();
        let mut tagged = vec![0xd8, 42];
        tagged.extend_from_slice(&bytes);
        assert_eq!(Program::from_cbor(&tagged), Ok(expected.clone()));
        let mut trailing = bytes;
        trailing.push(0xff);
        assert_eq!(Program::from_cbor(&trailing), Ok(expected));
    }

    #[test]
    fn a_reader_takes_only_its_own_minor_version_before_1_0() {
        assert!(compatible(VERSION));
        assert!(!compatible("0.5.0"));
        assert!(!compatible("0.3.0"));
        assert!(!compatible("1.0.0"));
        assert!(!compatible("0.1"));
        assert!(!compatible("0.10.0"));
        assert!(!compatible("0.11.0"));
        assert!(!compatible("0.12.0"));
        let mut p = program(1.0);
        p.ir_version = "0.14.0".to_owned();
        let found = "0.14.0".to_owned();
        assert_eq!(
            Program::from_json(&p.to_json()),
            Err(Error::Version { found })
        );
    }

    #[test]
    fn lexical_initialization_survives_both_forms() {
        let mut p = program(1.0);
        p.modules[0].initialization = crate::Initialization {
            prefixes: vec![crate::Prefix {
                before: 100,
                bindings: vec!["x".into()],
            }],
        };
        assert_eq!(Program::from_json(&p.to_json()), Ok(p.clone()));
        #[cfg(feature = "cbor")]
        assert_eq!(Program::from_cbor(&p.to_cbor()), Ok(p));
    }

    #[test]
    fn the_exports_of_a_module_survive_both_forms_as_a_reader_is_told() {
        let mut p = program(1.0);
        // a module that exports nothing says nothing about it
        let written: serde_json::Value = serde_json::from_str(&p.to_json()).expect("JSON");
        assert_eq!(written["modules"][0].get("exports"), None);
        assert_eq!(Program::from_json(&p.to_json()), Ok(p.clone()));

        let value = |name: &str, sync| crate::Exported {
            name: name.to_owned(),
            sync,
        };
        p.modules[0].exports = crate::Exports {
            values: vec![value("parse", Some(2)), value("x", None)],
            reexports: vec![crate::Reexport {
                module: "Http.Url".to_owned(),
                namespace: Namespace::Type,
                name: "Url".to_owned(),
            }],
        };
        let written: serde_json::Value = serde_json::from_str(&p.to_json()).expect("JSON");
        assert_eq!(
            written["modules"][0]["exports"],
            serde_json::json!({
                "values": [{ "name": "parse", "sync": 2 }, { "name": "x", "sync": null }],
                "reexports": [{ "module": "Http.Url", "namespace": "type", "name": "Url" }],
            })
        );
        assert_eq!(Program::from_json(&p.to_json()), Ok(p.clone()));
        #[cfg(feature = "cbor")]
        assert_eq!(Program::from_cbor(&p.to_cbor()), Ok(p));
    }
}
