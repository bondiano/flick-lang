//! Direct Go code generation for qualified Go package globals such as
//! `@native "math.Hypot" when go` or `foreign pure ... from global "strings.HasPrefix"`,
//! plus Go package import deduplication and aliasing.

use flick_ir::{Foreign, Span};
use flick_pretty::Document;

use super::Gen;
use super::foreign_pkg::is_reserved_go_import_ident;

/// A parsed Go qualified package symbol: import path (e.g. `"math"` or `"math/bits"`),
/// default package identifier (`"math"`, `"bits"`), and exported symbol (`"Hypot"`).
pub(super) struct GoGlobal<'a> {
    pub(super) import_path: &'a str,
    pub(super) default_pkg: &'a str,
    pub(super) export: &'a str,
}

/// Parses a Go qualified global target such as `"math.Hypot"` or `"math/bits.OnesCount32"`.
/// Returns `None` for non-Go or unqualified names (such as JS globals like `"Math.hypot"`
/// where the package starts with uppercase or has invalid Go export syntax).
pub(super) fn parse_go_global(raw: &str) -> Option<GoGlobal<'_>> {
    let (import_path, export) = raw.rsplit_once('.')?;
    if import_path.is_empty()
        || import_path.starts_with('.')
        || import_path.starts_with('/')
        || import_path.ends_with('/')
    {
        return None;
    }
    let mut default_pkg = "";
    for seg in import_path.split('/') {
        if seg.is_empty()
            || !seg
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        {
            return None;
        }
        default_pkg = seg;
    }
    // Go package identifiers are lowercase; avoid mistaking JS globals (`Math.hypot`).
    if !default_pkg
        .bytes()
        .next()
        .is_some_and(|b| b.is_ascii_lowercase())
        || !default_pkg
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return None;
    }
    // Go exported symbols start with an ASCII uppercase letter.
    if !export
        .bytes()
        .next()
        .is_some_and(|b| b.is_ascii_uppercase())
        || !export
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return None;
    }
    Some(GoGlobal {
        import_path,
        default_pkg,
        export,
    })
}

impl<'a> Gen<'a> {
    /// Registers a Go import path for this module, returning the package identifier
    /// to qualify exported symbols with. Uses an explicit import alias when
    /// `force_alias` is true, when `default_pkg` is reserved, or on collision.
    pub(super) fn add_go_import_with_alias(
        &mut self,
        import_path: &str,
        default_pkg: &str,
        force_alias: bool,
    ) -> &'a str {
        if let Some(pkg) = self.go_imports.get(import_path) {
            return self.a.alloc_str(pkg);
        }
        let used =
            self.go_import_idents.contains(default_pkg) || is_reserved_go_import_ident(default_pkg);
        let (alias, ident) = if used {
            let mut n = 0usize;
            let chosen = loop {
                let candidate = format!("{default_pkg}_{n}");
                if !self.go_import_idents.contains(&candidate)
                    && !is_reserved_go_import_ident(&candidate)
                {
                    break candidate;
                }
                n += 1;
            };
            (Some(chosen.clone()), chosen)
        } else if force_alias {
            (Some(default_pkg.to_owned()), default_pkg.to_owned())
        } else {
            (None, default_pkg.to_owned())
        };
        self.go_import_idents.insert(ident.clone());
        self.go_imports
            .insert(import_path.to_owned(), ident.clone());
        self.go_extra_imports.push((alias, import_path.to_owned()));
        self.a.alloc_str(&ident)
    }

    /// Emits a Go wrapper for a qualified Go package global such as `"math.Hypot"`.
    pub(super) fn global_foreign(
        &mut self,
        f: &Foreign,
        raw_name: &str,
        span: Span,
    ) -> Option<Document<'a>> {
        let parsed = parse_go_global(raw_name)?;
        self.emit_direct_go(
            f,
            parsed.import_path,
            parsed.default_pkg,
            false,
            parsed.export,
            span,
        )
    }
}
