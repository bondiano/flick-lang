//! Parsing and validation of `[build]` and `[target.<name>]` in `flick.toml`.

use std::collections::BTreeMap;
use std::{fmt, ops};

use serde::{Deserialize, Deserializer, de};
use toml::Spanned;

use super::{
    BackendEnvironment, BackendPackage, Build, Profile, TargetDependency, TargetOption, TargetValue,
};
use crate::codegen::Labels;
use crate::diagnostics::{Diagnostic, codes};
use crate::source::Span;
use crate::types;

pub(super) struct RawTargetTable {
    dependencies: Option<Spanned<RawDepsField>>,
    options: BTreeMap<Spanned<String>, Spanned<toml::Value>>,
}

enum RawDepsField {
    Table(BTreeMap<Spanned<String>, Spanned<toml::Value>>),
    NotATable,
}

impl<'de> Deserialize<'de> for RawTargetTable {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = RawTargetTable;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a target options table")
            }
            fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut dependencies = None;
                let mut options = BTreeMap::new();
                while let Some(key) = map.next_key::<Spanned<String>>()? {
                    if key.get_ref() == "dependencies" {
                        dependencies = Some(map.next_value::<Spanned<RawDepsField>>()?);
                    } else {
                        let value = map.next_value::<Spanned<toml::Value>>()?;
                        options.insert(key, value);
                    }
                }
                Ok(RawTargetTable {
                    dependencies,
                    options,
                })
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}

impl<'de> Deserialize<'de> for RawDepsField {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = RawDepsField;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a table of package versions")
            }
            fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut entries = BTreeMap::new();
                while let Some(key) = map.next_key::<Spanned<String>>()? {
                    if key.get_ref().starts_with("$__toml_private_") {
                        let _ = map.next_value::<de::IgnoredAny>()?;
                        return Ok(RawDepsField::NotATable);
                    }
                    let value = map.next_value::<Spanned<toml::Value>>()?;
                    entries.insert(key, value);
                }
                Ok(RawDepsField::Table(entries))
            }
            fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
                Ok(RawDepsField::NotATable)
            }
            fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
                Ok(RawDepsField::NotATable)
            }
            fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
                Ok(RawDepsField::NotATable)
            }
            fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
                Ok(RawDepsField::NotATable)
            }
            fn visit_str<E: de::Error>(self, _: &str) -> Result<Self::Value, E> {
                Ok(RawDepsField::NotATable)
            }
            fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                while seq.next_element::<de::IgnoredAny>()?.is_some() {}
                Ok(RawDepsField::NotATable)
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

pub(super) type RawTargets = BTreeMap<Spanned<String>, Spanned<RawTargetTable>>;

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", default)]
pub(super) struct RawBuild {
    out_dir: Option<String>,
    profile: Option<Profile>,
    target: Option<Spanned<String>>,
    backend: Option<Spanned<BackendPackage>>,
    environment: Option<Spanned<BackendEnvironment>>,
    solver_steps: Option<u32>,
    solver_depth: Option<u32>,
    labels: Option<Labels>,
    // Removed backend-specific keys: accepted here only to emit FLICK-P057
    // with exact spans and replacement hints.
    emit: Option<Spanned<toml::Value>>,
    emit_dts: Option<Spanned<toml::Value>>,
    promise_facade: Option<Spanned<toml::Value>>,
    schema_aot: Option<Spanned<toml::Value>>,
    effect: Option<Spanned<toml::Value>>,
    ui: Option<Spanned<toml::Value>>,
}

fn reject_removed_build_keys(
    raw: &RawBuild,
    span: &dyn Fn(ops::Range<usize>) -> Span,
) -> Result<(), Diagnostic> {
    if let Some(val) = &raw.emit {
        let hint = match val.get_ref().as_str() {
            Some("js") => {
                "use `[build] target = \"js\"` (or omit it; `\"js\"` is the default)".to_owned()
            }
            Some("ts") => {
                "use `[build] target = \"js\"` and `[target.js] typescript = true`".to_owned()
            }
            Some("go") => "use `[build] target = \"go\"`".to_owned(),
            Some(other) => format!("use `[build] target = \"{other}\"`"),
            None => "use `[build] target = \"<name>\"`".to_owned(),
        };
        return Err(
            Diagnostic::error(codes::P057, "`[build] emit` was removed", span(val.span()))
                .with_fix(|| (hint, Vec::new())),
        );
    }
    for (val, old_key, new_key) in [
        (&raw.emit_dts, "emit-dts", "declarations"),
        (&raw.promise_facade, "promise-facade", "promise-facade"),
        (&raw.schema_aot, "schema-aot", "schema-aot"),
    ] {
        if let Some(val) = val {
            let hint = match val.get_ref().as_bool() {
                Some(b) => format!("move to `[target.js] {new_key} = {b}`"),
                None => format!("move to `[target.js] {new_key} = true`"),
            };
            return Err(Diagnostic::error(
                codes::P057,
                format!("`[build] {old_key}` was removed"),
                span(val.span()),
            )
            .with_fix(|| (hint, Vec::new())));
        }
    }
    if let Some(val) = &raw.effect {
        let hint = match val.get_ref().as_str() {
            Some(v) => format!("move to `[target.js] runtime = \"{v}\"`"),
            None => "move to `[target.js] runtime = \"rt\"` or `\"effect\"`".to_owned(),
        };
        return Err(Diagnostic::error(
            codes::P057,
            "`[build] effect` was removed",
            span(val.span()),
        )
        .with_fix(|| (hint, Vec::new())));
    }
    if let Some(val) = &raw.ui {
        let hint = match val.get_ref().as_str() {
            Some(v) => format!("move to `[target.js] ui = \"{v}\"`"),
            None => "move to `[target.js] ui = \"<module>\"`".to_owned(),
        };
        return Err(
            Diagnostic::error(codes::P057, "`[build] ui` was removed", span(val.span()))
                .with_fix(|| (hint, Vec::new())),
        );
    }
    Ok(())
}

pub(super) fn parse_build(
    raw: Spanned<RawBuild>,
    span: &dyn Fn(ops::Range<usize>) -> Span,
) -> Result<Build, Diagnostic> {
    let raw = raw.into_inner();
    reject_removed_build_keys(&raw, span)?;
    if let Some(target) = &raw.target
        && !is_target_name(target.get_ref())
    {
        return Err(Diagnostic::error(
            codes::P005,
            format!(
                "`{}` is not a target name: lowercase ASCII letters, digits, `-` and `_`, starting with a letter",
                target.get_ref()
            ),
            span(target.span()),
        ));
    }
    if let Some(backend) = &raw.backend {
        backend
            .get_ref()
            .validate()
            .map_err(|why| Diagnostic::error(codes::P005, why, span(backend.span())))?;
    }
    let defaults = Build::default();
    let target_span = raw.target.as_ref().map(|t| span(t.span()));
    let environment_span = raw.environment.as_ref().map(|e| span(e.span()));
    Ok(Build {
        out_dir: raw.out_dir.unwrap_or(defaults.out_dir),
        profile: raw.profile.unwrap_or(defaults.profile),
        target: raw.target.map(Spanned::into_inner),
        target_span,
        backend: raw.backend.map(Spanned::into_inner),
        environment: raw.environment.map(Spanned::into_inner),
        environment_span,
        solver_steps: raw.solver_steps.unwrap_or(types::Limits::default().steps),
        solver_depth: raw.solver_depth.unwrap_or(types::Limits::default().depth),
        labels: raw.labels.unwrap_or(defaults.labels),
    })
}

#[allow(
    clippy::type_complexity,
    reason = "parsed target options plus extracted target dependencies"
)]
pub(super) fn parse_targets(
    raw: RawTargets,
    span: &dyn Fn(ops::Range<usize>) -> Span,
) -> Result<
    (
        BTreeMap<String, BTreeMap<String, TargetOption>>,
        Vec<TargetDependency>,
    ),
    Diagnostic,
> {
    let mut out = BTreeMap::new();
    let mut target_dependencies = Vec::new();
    for (name, table) in raw {
        let name_span = span(name.span());
        let name = name.into_inner();
        if !is_target_name(&name) {
            return Err(Diagnostic::error(
                codes::P005,
                format!(
                    "`[target.{name}]` is not a valid target name: lowercase ASCII letters, digits, `-` and `_`, starting with a letter"
                ),
                name_span,
            ));
        }
        let RawTargetTable {
            dependencies,
            options: raw_options,
        } = table.into_inner();
        if let Some(deps) = dependencies {
            let value_span = span(deps.span());
            let RawDepsField::Table(entries) = deps.into_inner() else {
                return Err(Diagnostic::error(
                    codes::P005,
                    format!("`[target.{name}.dependencies]` must be a table of package versions"),
                    value_span,
                ));
            };
            for (dep_name, dep_value) in entries {
                let dep_span = span(dep_value.span());
                let dep_name = dep_name.into_inner();
                let toml::Value::String(version) = dep_value.into_inner() else {
                    return Err(Diagnostic::error(
                        codes::P005,
                        format!(
                            "`[target.{name}.dependencies]`: `{dep_name}` must be a version string"
                        ),
                        dep_span,
                    ));
                };
                target_dependencies.push(TargetDependency {
                    target: name.clone(),
                    name: dep_name,
                    version,
                    span: dep_span,
                });
            }
        }
        let mut options = BTreeMap::new();
        for (key, value) in raw_options {
            let key_span = span(key.span());
            let value_span = span(value.span());
            let key_name = key.into_inner();
            let target_value = match value.into_inner() {
                toml::Value::Boolean(b) => TargetValue::Bool(b),
                toml::Value::String(s) => TargetValue::String(s),
                toml::Value::Array(items) => {
                    let mut list = Vec::with_capacity(items.len());
                    let mut invalid = None;
                    for item in items {
                        if let toml::Value::String(s) = item {
                            list.push((s, value_span));
                        } else {
                            invalid = Some(item.type_str().to_owned());
                            break;
                        }
                    }
                    match invalid {
                        Some(ty) => TargetValue::Unsupported(format!("array containing {ty}")),
                        None => TargetValue::List(list),
                    }
                }
                other => TargetValue::Unsupported(other.type_str().to_owned()),
            };
            options.insert(
                key_name,
                TargetOption {
                    value: target_value,
                    key_span,
                    value_span,
                },
            );
        }
        out.insert(name, options);
    }
    Ok((out, target_dependencies))
}

fn is_target_name(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_'))
}
