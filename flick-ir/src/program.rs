//! A compiled program as a file: its modules, the source files their spans
//! point into, and the version of the IR they are written in.
//!
//! The version follows semver. Before 1.0, any change a reader of an older
//! version could misread bumps the minor version, and a reader takes only
//! its own minor version; from 1.0, its own major version.

use std::collections::BTreeMap;
use std::{error, fmt};

use serde::{Deserialize, Serialize};

use crate::{FileId, LangItem, Module};

/// The version of the IR this crate reads and writes.
pub const VERSION: &str = "0.11.0";

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
        let program: Program =
            ciborium::from_reader(bytes).map_err(|e| Error::Format(e.to_string()))?;
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

#[cfg(all(test, feature = "json"))]
mod tests {
    use std::collections::BTreeMap;

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

    #[test]
    fn a_reader_takes_only_its_own_minor_version_before_1_0() {
        assert!(compatible(VERSION));
        assert!(!compatible("0.5.0"));
        assert!(!compatible("0.3.0"));
        assert!(!compatible("1.0.0"));
        assert!(!compatible("0.1"));
        assert!(!compatible("0.10.0"));
        let mut p = program(1.0);
        p.ir_version = "0.12.0".to_owned();
        let found = "0.12.0".to_owned();
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
}
