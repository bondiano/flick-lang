//! Package target parsing, reserved identifier checks, and wrapper signature
//! classification for direct Go package bindings (`foreign f from "pkg" "Sym"`).

use flick_backend_support::Known;
use flick_ir::{Binding as IrBinding, Foreign, ForeignMode, IrType};

use super::foreign::is_relative;
use super::foreign_global::parse_go_global;
use super::foreign_pkg_conv::GoAbiType;
use super::go_type::{GoType, TypeDecls};
use super::shape::Types;

/// Go reserved keywords and wrapper parameter names that an import alias must not shadow.
const RESERVED_IDENTS: &[&str] = &[
    "flickrt",
    "fb",
    "ctx",
    "break",
    "case",
    "chan",
    "const",
    "continue",
    "default",
    "defer",
    "else",
    "fallthrough",
    "for",
    "func",
    "go",
    "goto",
    "if",
    "import",
    "interface",
    "map",
    "package",
    "range",
    "return",
    "select",
    "struct",
    "switch",
    "type",
    "var",
];

/// A parsed direct Go package binding target (`from "pkg" "Export"`).
pub(super) struct GoPackageTarget<'a> {
    pub(super) import_path: &'a str,
    pub(super) default_pkg: String,
    /// True when the import path needs an explicit Go import alias (e.g. `/v2`,
    /// `.v3`, hyphenated directory name, or Go keyword segment).
    pub(super) force_alias: bool,
    pub(super) export: &'a str,
}

fn is_major_version_segment(seg: &str) -> bool {
    let Some(digits) = seg.strip_prefix('v') else {
        return false;
    };
    !digits.is_empty()
        && !digits.starts_with('0')
        && digits.bytes().all(|b| b.is_ascii_digit())
        && digits.parse::<u32>().is_ok_and(|n| n >= 2)
}

fn is_valid_go_ident(s: &str) -> bool {
    let mut bytes = s.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Whether `ident` is a Go keyword or collides with generated wrapper locals (`fb`, `ctx`, `aN`, `dN`, `tN`).
pub(super) fn is_reserved_go_import_ident(ident: &str) -> bool {
    if RESERVED_IDENTS.contains(&ident) {
        return true;
    }
    let bytes = ident.as_bytes();
    bytes.len() >= 2
        && matches!(bytes[0], b'a' | b'd' | b't' | b'p' | b'r')
        && bytes[1..].iter().all(u8::is_ascii_digit)
}

/// Parses a direct Go package binding `(from, export)` such as `("math", "Hypot")`
/// or `("example.com/foo/bar/v2", "Run")`. Returns `None` for relative paths,
/// `flick:` runtime specifiers, or invalid Go import/export syntax.
pub(super) fn parse_go_package<'a>(from: &'a str, export: &'a str) -> Option<GoPackageTarget<'a>> {
    if from.is_empty()
        || is_relative(from)
        || from.starts_with("flick:")
        || from.starts_with('.')
        || from.starts_with('/')
        || from.ends_with('/')
    {
        return None;
    }
    let segments: Vec<&str> = from.split('/').collect();
    if segments.iter().any(|seg| {
        seg.is_empty()
            || *seg == "."
            || *seg == ".."
            || !seg
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'~' | b'+'))
    }) {
        return None;
    }
    // Exported Go symbol (single `Hypot` or dotted `StdEncoding.EncodeToString`).
    if export.is_empty()
        || !export.split('.').all(|seg| {
            seg.bytes().next().is_some_and(|b| b.is_ascii_uppercase())
                && seg.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
    {
        return None;
    }
    let last = *segments.last()?;
    let base = if segments.len() >= 2 && is_major_version_segment(last) {
        segments[segments.len() - 2]
    } else {
        last
    };
    let stripped = match base.rsplit_once('.') {
        Some((prefix, suffix)) if is_major_version_segment(suffix) => prefix,
        _ => base,
    };
    let mut sanitized: String = stripped
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if !is_valid_go_ident(&sanitized) || sanitized.starts_with(|c: char| c.is_ascii_digit()) {
        sanitized = format!("pkg_{sanitized}");
    }
    let force_alias = sanitized != last || is_reserved_go_import_ident(&sanitized);
    Some(GoPackageTarget {
        import_path: from,
        default_pkg: sanitized,
        force_alias,
        export,
    })
}

fn is_direct_go_binding(binding: &IrBinding) -> bool {
    match binding {
        IrBinding::Module { from, name } => parse_go_package(from, name).is_some(),
        IrBinding::Global { name } => parse_go_global(name).is_some(),
        IrBinding::Inline(_) => false,
    }
}

/// Decomposes a foreign `IrType` into `(param_types, result_type)` for `params` stages.
pub(super) fn decompose_sig(ty: &IrType, params: usize) -> Option<(Vec<&IrType>, &IrType)> {
    let mut cur = ty;
    while let IrType::Forall { body, .. } = cur {
        cur = body;
    }
    let mut out: Vec<&IrType> = Vec::with_capacity(params);
    for _ in 0..params {
        match cur {
            IrType::Fun { param, result, .. } => {
                out.push(param);
                cur = result;
            }
            _ => return None,
        }
    }
    Some((out, cur))
}

/// Computes the typed Go wrapper signature (`Types`) for a direct Go package
/// binding or qualified Go global, matching `shape::shape` and `emit_direct_go`.
pub(super) fn direct_go_types(
    lang: &Known,
    decls: &TypeDecls,
    f: &Foreign,
    params: usize,
) -> Types {
    if !is_direct_go_binding(&f.binding) || f.dicts > 0 {
        return Types::default();
    }
    let Some(ty) = f.ty.as_deref() else {
        return Types::default();
    };
    let Some((param_tys, result_ty)) = decompose_sig(ty, params) else {
        return Types::default();
    };
    // Validate that all parameter and result types are in the Go ABI table before
    // choosing typed wrapper signatures (keeps fallback behavior consistent).
    if f.arity == 0 {
        if GoAbiType::classify(lang, decls, result_ty, 0).is_none() {
            return Types::default();
        }
        let result_go = (f.mode != ForeignMode::Async && f.catch.is_none())
            .then(|| GoType::of(lang, decls, Some(result_ty)))
            .flatten();
        return Types::of(result_go, &[None]);
    }
    if f.arity != params {
        return Types::default();
    }
    let first_is_typed_opaque = param_tys.first().is_some_and(|p| {
        matches!(
            GoAbiType::classify(lang, decls, p, 0),
            Some(GoAbiType::Opaque(Some(_)))
        )
    });
    if f.adapter.has_word("this") && !first_is_typed_opaque {
        return Types::default();
    }
    for p in &param_tys {
        if GoAbiType::classify(lang, decls, p, 0).is_none() {
            return Types::default();
        }
    }
    if GoAbiType::classify(lang, decls, result_ty, 0).is_none() {
        return Types::default();
    }
    let param_go: Vec<Option<GoType>> = param_tys
        .iter()
        .map(|p| GoType::of(lang, decls, Some(p)))
        .collect();
    let result_go = (f.mode != ForeignMode::Async && f.catch.is_none())
        .then(|| GoType::of(lang, decls, Some(result_ty)))
        .flatten();
    Types::of(result_go, &param_go)
}
