//! JSON surface emitted by the embedded `go/types` inspector (`inspect.go`)
//! and the per-package `policy.toml` schema for `go:ffi import` / `go:ffi check`.

use std::collections::BTreeMap;

use serde::Deserialize;

/// Exported surface of one Go package, as `inspect.go` emits it.
#[derive(Debug, Clone, Deserialize)]
pub(super) struct GoSurface {
    pub(super) package: String,
    #[serde(default)]
    pub(super) version: Option<String>,
    pub(super) declarations: String,
    #[serde(default)]
    pub(super) exports: Vec<GoExport>,
}

impl GoSurface {
    pub(super) fn parse(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| format!("the Go inspector printed bad JSON: {e}"))
    }

    /// `pkg@v1.0.0`, or `pkg` for stdlib and unversioned local modules.
    pub(super) fn provenance(&self) -> String {
        match &self.version {
            Some(v) => format!("{}@{v}", self.package),
            None => self.package.clone(),
        }
    }

    pub(super) fn find(&self, export: &str) -> Option<&GoExport> {
        self.exports.iter().find(|e| e.name == export)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct GoExport {
    pub(super) name: String,
    pub(super) kind: GoExportKind,
    #[serde(default)]
    pub(super) receiver: Option<GoReceiver>,
    #[serde(default)]
    pub(super) signature: Option<GoSignature>,
    #[serde(rename = "type", default)]
    pub(super) ty: Option<GoTypeShape>,
    #[serde(default)]
    pub(super) int32_ok: Option<bool>,
    pub(super) fingerprint: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum GoExportKind {
    Func,
    Method,
    Const,
    Var,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(super) struct GoReceiver {
    pub(super) package: String,
    pub(super) name: String,
    #[serde(default)]
    pub(super) ptr: bool,
    #[serde(default)]
    pub(super) is_interface: bool,
    #[serde(default)]
    pub(super) generic: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(super) struct GoSignature {
    #[serde(default)]
    pub(super) type_params: Vec<String>,
    #[serde(default)]
    pub(super) params: Vec<GoParam>,
    #[serde(default)]
    pub(super) results: Vec<GoTypeShape>,
    #[serde(default)]
    pub(super) variadic: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(super) struct GoParam {
    #[serde(default)]
    pub(super) name: String,
    #[serde(rename = "type")]
    pub(super) ty: GoTypeShape,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(super) struct GoTypeShape {
    pub(super) kind: String,
    #[serde(default)]
    pub(super) basic: Option<String>,
    #[serde(default)]
    pub(super) elem: Option<Box<GoTypeShape>>,
    #[serde(default)]
    pub(super) package: Option<String>,
    #[serde(default)]
    pub(super) name: Option<String>,
    #[serde(default)]
    pub(super) exported: bool,
    #[serde(default)]
    pub(super) underlying: Option<String>,
    #[serde(default)]
    pub(super) type_args: Vec<GoTypeShape>,
    #[serde(default)]
    pub(super) is_error: bool,
    #[serde(default)]
    pub(super) is_context: bool,
    #[serde(default)]
    pub(super) text: String,
}

impl GoTypeShape {
    pub(super) fn is_interface(&self) -> bool {
        self.underlying.as_deref() == Some("interface")
    }
}

/// `policy.toml` beside a generated Go FFI module.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GoPolicy {
    #[serde(default)]
    pub(super) exports: BTreeMap<String, GoExportPolicy>,
    #[serde(default)]
    pub(super) types: BTreeMap<String, GoTypePolicy>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GoExportPolicy {
    /// `"pure"`: trust the Go call to be pure.
    pub(super) mode: Option<String>,
    /// `"none"`: a nullable Go pointer or interface return becomes `Option a` with `nullable`.
    pub(super) undefined: Option<String>,
    /// `true`: shorthand for `undefined = "none"`.
    #[serde(default)]
    pub(super) nullable: bool,
    /// Custom `catch` conversion function (`String -> E`), defaulting to `from_go_error`.
    pub(super) catch: Option<String>,
    /// Custom Flick error type name paired with `catch`, defaulting to `GoError`.
    pub(super) error: Option<String>,
}

impl GoExportPolicy {
    pub(super) fn allows_nullable(&self) -> bool {
        self.nullable || self.undefined.as_deref() == Some("none")
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GoTypePolicy {
    /// Force this Go type to remain an opaque handle.
    #[serde(default)]
    pub(super) opaque: bool,
    /// Map this Go type to an existing Flick type (e.g. `"String"`, `"Int"`, `"Bytes"`).
    pub(super) convert: Option<String>,
    /// Override the `@go` type form emitted for this opaque type (e.g. `"*MyType"`).
    pub(super) go_form: Option<String>,
}

impl GoPolicy {
    pub(super) fn parse(text: &str) -> Result<Self, String> {
        let policy: Self = toml::from_str(text).map_err(|e| e.message().to_owned())?;
        for (name, export) in &policy.exports {
            if let Some(mode) = &export.mode
                && mode != "pure"
            {
                return Err(format!(
                    "`mode` of `{name}` is \"{mode}\"; expected \"pure\""
                ));
            }
            if let Some(undef) = &export.undefined
                && undef != "none"
            {
                return Err(format!(
                    "`undefined` of `{name}` is \"{undef}\"; expected \"none\""
                ));
            }
            if let Some(catch) = &export.catch
                && catch.trim().is_empty()
            {
                return Err(format!("`catch` of `{name}` cannot be empty"));
            }
            if let Some(err_ty) = &export.error
                && err_ty.trim().is_empty()
            {
                return Err(format!("`error` of `{name}` cannot be empty"));
            }
            if export.catch.is_some() != export.error.is_some() {
                return Err(format!(
                    "`[exports.{name}]` must set `catch` and `error` together"
                ));
            }
        }
        for (name, ty) in &policy.types {
            if ty.opaque && ty.convert.is_some() {
                return Err(format!(
                    "`[types.{name}]` cannot set both `opaque = true` and `convert`"
                ));
            }
            if let Some(conv) = &ty.convert
                && conv.trim().is_empty()
            {
                return Err(format!("`convert` of `[types.{name}]` cannot be empty"));
            }
        }
        Ok(policy)
    }

    pub(super) fn of_export(&self, export: &str) -> GoExportPolicy {
        self.exports.get(export).cloned().unwrap_or_default()
    }

    pub(super) fn of_type(&self, key: &str) -> Option<&GoTypePolicy> {
        self.types.get(key)
    }
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

/// Converts a Go import path into its Flick module directory segment:
/// `strings` -> `Strings`, `example.com/foo/v2` -> `Foo`, `gopkg.in/yaml.v3` -> `Yaml`.
pub(super) fn go_module_part(package: &str) -> String {
    let segments: Vec<&str> = package.split('/').filter(|s| !s.is_empty()).collect();
    let base = match segments.as_slice() {
        [.., prev, last] if is_major_version_segment(last) => *prev,
        [.., last] => *last,
        [] => package,
    };
    let stripped = match base.rsplit_once('.') {
        Some((prefix, suffix)) if is_major_version_segment(suffix) => prefix,
        _ => base,
    };
    let name: String = stripped
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
        _ => format!("Go{name}"),
    }
}
