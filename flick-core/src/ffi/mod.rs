//! `flick ffi`: bindings generated from the `.d.ts` of an installed package
//! and their review. The Node helper `tools/dts-importer` reads the
//! declarations with TypeScript and describes each value export as a
//! [`Surface`]; [`import::generate`] turns it into `Generated.fl` and a
//! coverage report, and [`check`] compares reviewed bindings with the
//! declarations they were reviewed against. Rules: `docs/src/content/docs/reference/ffi.md`.

pub mod check;
mod convert;
mod enums;
pub mod import;

#[cfg(test)]
mod enum_tests;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use serde::Deserialize;

/// The value exports of a package, as the helper prints them.
#[derive(Debug, Clone, Deserialize)]
pub struct Surface {
    pub package: String,
    pub version: Option<String>,
    /// The declaration file, from the package name: `date-fns/index.d.ts`.
    pub declarations: String,
    pub exports: Vec<ExportShape>,
    /// Simple names of object types that stand for more than one TypeScript
    /// declaration: one Flick type would confuse them.
    #[serde(default)]
    pub ambiguous: Vec<String>,
}

impl Surface {
    pub fn parse(json: &str) -> Result<Surface, String> {
        serde_json::from_str(json).map_err(|e| format!("the `.d.ts` reader printed bad JSON: {e}"))
    }

    /// `pkg@1.2.3`, or `pkg` without a known version.
    pub fn provenance(&self) -> String {
        match &self.version {
            Some(v) => format!("{}@{v}", self.package),
            None => self.package.clone(),
        }
    }

    pub fn find(&self, export: &str) -> Option<&ExportShape> {
        self.exports.iter().find(|e| e.name == export)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExportShape {
    /// The JS export name; `default` for a default export.
    pub name: String,
    /// The declared name: `delay` for `export default function delay`.
    pub local: String,
    pub kind: ExportKind,
    #[serde(default)]
    pub signatures: Vec<Signature>,
    #[serde(rename = "type")]
    pub ty: Option<Shape>,
    /// `sha256:…` of the export's shape: what a review vouches for.
    pub fingerprint: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportKind {
    Function,
    Class,
    Value,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Signature {
    pub params: Vec<Param>,
    /// Declares a `this` parameter.
    pub this: bool,
    pub type_params: Vec<String>,
    pub result: Shape,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Param {
    pub name: String,
    pub optional: bool,
    pub rest: bool,
    #[serde(rename = "type")]
    pub ty: Shape,
}

/// A TypeScript type, reduced by the TypeScript checker.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "k", rename_all = "lowercase")]
pub enum Shape {
    Any,
    Unknown,
    Boolean,
    String,
    Number,
    Bigint,
    Void,
    Undefined,
    Null,
    Never,
    Literal {
        text: String,
    },
    /// A type parameter; `bound` is the text of its constraint.
    Param {
        name: String,
        bound: Option<String>,
    },
    /// `alias` names the `type` alias that declares it.
    Union {
        types: Vec<Shape>,
        #[serde(default)]
        alias: Option<String>,
    },
    Tuple {
        elements: Vec<Shape>,
    },
    Array {
        element: Box<Shape>,
    },
    Promise {
        #[serde(rename = "type")]
        ty: Box<Shape>,
    },
    Function {
        signatures: Vec<Signature>,
    },
    /// An instance of a class, or a built-in object such as `Date`.
    Handle {
        name: String,
    },
    /// A plain object shape; `name` of an interface or alias.
    Object {
        name: Option<String>,
        text: String,
    },
    /// Anything else, with its TypeScript text.
    Other {
        text: String,
    },
}

impl Shape {
    /// The TypeScript spelling, for reports.
    pub fn text(&self) -> String {
        match self {
            Shape::Any => "any".into(),
            Shape::Unknown => "unknown".into(),
            Shape::Boolean => "boolean".into(),
            Shape::String => "string".into(),
            Shape::Number => "number".into(),
            Shape::Bigint => "bigint".into(),
            Shape::Void => "void".into(),
            Shape::Undefined => "undefined".into(),
            Shape::Null => "null".into(),
            Shape::Never => "never".into(),
            Shape::Literal { text } | Shape::Object { text, .. } | Shape::Other { text } => {
                text.clone()
            }
            Shape::Param { name, .. } | Shape::Handle { name } => name.clone(),
            Shape::Union { types, .. } => join(types, " | "),
            Shape::Tuple { elements } => format!("[{}]", join(elements, ", ")),
            Shape::Array { element } => format!("{}[]", element.text()),
            Shape::Promise { ty } => format!("Promise<{}>", ty.text()),
            Shape::Function { .. } => "function".into(),
        }
    }
}

fn join(shapes: &[Shape], separator: &str) -> String {
    shapes
        .iter()
        .map(Shape::text)
        .collect::<Vec<_>>()
        .join(separator)
}

/// `policy.toml` beside the generated module: the contracts a `.d.ts` does
/// not state, which the user gives per export.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    /// `any` and `unknown` are refused instead of becoming `JsValue`.
    #[serde(default)]
    pub strict: bool,
    #[serde(default)]
    pub exports: BTreeMap<String, ExportPolicy>,
    /// The constructor names of an enum, by its literals:
    /// `[enums.Method] "GET" = "Get"`.
    #[serde(default)]
    pub enums: BTreeMap<String, BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportPolicy {
    /// `"pure"`: the call is trusted to be pure.
    pub mode: Option<String>,
    /// `"JsError"`: a throw or rejection becomes `JsError`.
    pub catch: Option<String>,
    /// `"none"`: an `undefined` or `null` result is `None`.
    pub undefined: Option<String>,
    /// `"signal"`: an async call gets an `AbortSignal`, aborted on interruption.
    pub cancel: Option<String>,
    /// `"call"` or `"owner"`: how long JS keeps a callback.
    pub retention: Option<String>,
}

impl Policy {
    #[cfg(feature = "native-project")]
    pub fn parse(text: &str) -> Result<Policy, String> {
        let policy: Policy = toml::from_str(text).map_err(|e| e.message().to_owned())?;
        for (name, export) in &policy.exports {
            let checks = [
                ("mode", &export.mode, &["pure"][..]),
                ("catch", &export.catch, &["JsError"]),
                ("undefined", &export.undefined, &["none"]),
                ("cancel", &export.cancel, &["signal"]),
                ("retention", &export.retention, &["call", "owner"]),
            ];
            for (key, value, allowed) in checks {
                if let Some(value) = value
                    && !allowed.contains(&value.as_str())
                {
                    let allowed: Vec<String> = allowed.iter().map(|a| format!("\"{a}\"")).collect();
                    return Err(format!(
                        "`{key}` of `{name}` is \"{value}\"; expected {}",
                        allowed.join(" or ")
                    ));
                }
            }
        }
        for (name, constructors) in &policy.enums {
            if let Some(c) = constructors.values().find(|c| !convert::is_type_name(c)) {
                return Err(format!(
                    "`{c}` of `[enums.{name}]` is no constructor name: it starts with a capital letter"
                ));
            }
        }
        Ok(policy)
    }

    pub fn of(&self, export: &str) -> ExportPolicy {
        self.exports.get(export).cloned().unwrap_or_default()
    }
}

/// `date-fns` → `DateFns`, `@acme/lookup-lib` → `LookupLib`: the module of
/// a package's bindings.
pub fn module_part(package: &str) -> String {
    let last = package.rsplit('/').next().unwrap_or(package);
    let name: String = last
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            chars
                .next()
                .map(|c| c.to_ascii_uppercase())
                .into_iter()
                .chain(chars)
                .collect::<String>()
        })
        .collect();
    match name.chars().next() {
        Some(c) if c.is_ascii_alphabetic() => name,
        _ => format!("Js{name}"),
    }
}
