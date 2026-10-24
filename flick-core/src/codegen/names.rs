//! JS names of what the IR refers to, and constructor ids.

use crate::types::{Constructor, PRELUDE};

/// Words a Flick name may spell that JS reserves, and the names generated
/// code relies on.
const RESERVED: &[&str] = &[
    "arguments",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "eval",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "implements",
    "import",
    "in",
    "instanceof",
    "interface",
    "let",
    "new",
    "null",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "static",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "undefined",
    "var",
    "void",
    "while",
    "with",
    "yield",
    "NaN",
    "Infinity",
    "Effect",
    "Object",
    "Array",
    "Error",
    "Symbol",
    "Promise",
    "globalThis",
];

/// A Flick value, local or constructor name as a JS identifier.
pub fn ident(name: &str) -> String {
    if RESERVED.contains(&name) {
        format!("{name}$")
    } else {
        name.to_owned()
    }
}

/// The namespace a module's exports are imported under.
pub fn module_alias(module: &str) -> String {
    format!("$m_{}", module.replace('.', "_"))
}

/// The dictionary of an instance, by the head it is named by: `$i_Show_Number`
/// when the head is names separated by single spaces, which the name maps
/// back to; any other head (`Show (List a)`) also gets a hash, so two heads
/// never share a dictionary.
pub fn instance(name: &str) -> String {
    let words = name.split(' ');
    if words
        .clone()
        .all(|w| !w.is_empty() && w.chars().all(|c| c.is_ascii_alphanumeric()))
    {
        return format!("$i_{}", words.collect::<Vec<_>>().join("_"));
    }
    let readable: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    format!("$i_{readable}_{:08x}", fnv(name))
}

pub fn service(name: &str) -> String {
    format!("$s_{name}")
}

/// The `_tag` of a constructor: its name, and a hash of its qualified id —
/// package, module, type and name — so the tag keeps the whole identity at
/// a fraction of its length: `Circle#3a0c5e1b9d27`.
pub fn tag(c: &Constructor, package: Option<&str>) -> String {
    format!("{}#{:012x}", c.name, fnv64(&qualified_id(c, package)) >> 16)
}

/// `@acme/shapes/App.Shapes.Shape.Circle`: what a tag stands for.
pub fn qualified_id(c: &Constructor, package: Option<&str>) -> String {
    let id = format!("{}.{}.{}", c.module, c.ty, c.name);
    match package {
        Some(p) => format!("{p}/{id}"),
        None => id,
    }
}

fn fnv64(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Constructors with the native Effect representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Native {
    Some,
    None,
    Ok,
    Err,
}

pub fn native(c: &Constructor) -> Option<Native> {
    if c.module != PRELUDE {
        return None;
    }
    match (c.ty.as_str(), c.name.as_str()) {
        ("Option", "Some") => Some(Native::Some),
        ("Option", "None") => Some(Native::None),
        ("Result", "Ok") => Some(Native::Ok),
        ("Result", "Err") => Some(Native::Err),
        _ => None,
    }
}

fn fnv(text: &str) -> u32 {
    text.bytes().fold(0x811c_9dc5, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    })
}

/// A string literal in JS: JSON with the marker characters of the emitter
/// and the line separators escaped.
pub fn string(text: &str) -> String {
    let json = serde_json::to_string(text).expect("a string serializes");
    json.replace('\u{E000}', "\\uE000")
        .replace('\u{E001}', "\\uE001")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

/// Whether `label` can follow a `.` in JS.
pub fn is_property(label: &str) -> bool {
    label
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && label
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}
