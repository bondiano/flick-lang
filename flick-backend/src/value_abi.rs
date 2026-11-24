//! Handshake validation for backend-declared versioned value-ABI tables.

use std::collections::BTreeSet;

use flick_ir::Lang;
use semver::Version;

use crate::{Descriptor, ValueAbi, ValueAbiEntry, ValueAbiRefusal};

const STRUCTURAL_ENTRY_KEYS: &[&str] = &["tuple", "function"];
const FORBIDDEN_ENTRY_KEYS: &[&str] = &["record", "adt"];

fn is_abi_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    first.is_ascii_lowercase()
        && !name.ends_with('.')
        && !name.ends_with('-')
        && !name.contains("..")
        && bytes.all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'_')
        })
}

fn is_valid_entry_key(key: &str) -> bool {
    STRUCTURAL_ENTRY_KEYS.contains(&key) || Lang::from_name(key).is_some()
}

fn is_valid_refusal_key(key: &str) -> bool {
    is_valid_entry_key(key) || FORBIDDEN_ENTRY_KEYS.contains(&key)
}

/// Validate the optional versioned value-ABI table a backend publishes in
/// `describe`. Records and ADTs have no stable layout and are forbidden in
/// `entries` (they may appear in `unsupported` with a backend refusal reason).
pub fn validate_value_abi(descriptor: &Descriptor) -> Result<(), String> {
    let Some(table) = &descriptor.value_abi else {
        return Ok(());
    };
    validate_header(table)?;
    let seen_entries = validate_entries(&table.abi.name, &table.entries)?;
    validate_refusals(&table.abi.name, &table.unsupported, &seen_entries)
}

fn validate_header(table: &ValueAbi) -> Result<(), String> {
    if !is_abi_name(&table.abi.name) {
        return Err(format!(
            "invalid value ABI name `{}`: use lowercase ASCII letters, digits, `.`, `-`, or `_`",
            table.abi.name
        ));
    }
    Version::parse(&table.abi.version).map_err(|err| {
        format!(
            "invalid value ABI version `{}` for `{}`: {err}",
            table.abi.version, table.abi.name
        )
    })?;
    if table.opaque_type.trim().is_empty() {
        return Err(format!(
            "value ABI `{}` must declare a non-empty `opaque-type`",
            table.abi.name
        ));
    }
    if table.catch_error_type.trim().is_empty() {
        return Err(format!(
            "value ABI `{}` must declare a non-empty `catch-error-type`",
            table.abi.name
        ));
    }
    if let Some(catch_item) = &table.catch_lang_item
        && Lang::from_name(catch_item).is_none()
    {
        return Err(format!(
            "value ABI `{}` declares unknown `catch-lang-item` `{catch_item}`",
            table.abi.name
        ));
    }
    if table.fallback_reason.trim().is_empty() {
        return Err(format!(
            "value ABI `{}` must declare a non-empty `fallback-reason`",
            table.abi.name
        ));
    }
    Ok(())
}

fn validate_entries<'a>(
    abi_name: &str,
    entries: &'a [ValueAbiEntry],
) -> Result<BTreeSet<&'a str>, String> {
    let mut seen_entries = BTreeSet::new();
    for entry in entries {
        if FORBIDDEN_ENTRY_KEYS.contains(&entry.key.as_str()) {
            return Err(format!(
                "value ABI `{abi_name}` cannot list `{}` in `entries`: records and ADTs have no stable layout",
                entry.key
            ));
        }
        if !is_valid_entry_key(&entry.key) {
            return Err(format!(
                "value ABI `{abi_name}` has unknown entry key `{}`: expected a `flick_ir::Lang` item or `tuple`/`function`",
                entry.key
            ));
        }
        if !seen_entries.insert(entry.key.as_str()) {
            return Err(format!(
                "value ABI `{abi_name}` lists duplicate entry key `{}`",
                entry.key
            ));
        }
        if entry.flick_type.trim().is_empty() {
            return Err(format!(
                "value ABI `{abi_name}` entry `{}` must declare a non-empty `flick-type`",
                entry.key
            ));
        }
        if entry.host_type.trim().is_empty() {
            return Err(format!(
                "value ABI `{abi_name}` entry `{}` must declare a non-empty `host-type`",
                entry.key
            ));
        }
    }
    Ok(seen_entries)
}

fn validate_refusals(
    abi_name: &str,
    unsupported: &[ValueAbiRefusal],
    seen_entries: &BTreeSet<&str>,
) -> Result<(), String> {
    let mut seen_refusals = BTreeSet::new();
    for refusal in unsupported {
        if !is_valid_refusal_key(&refusal.key) {
            return Err(format!(
                "value ABI `{abi_name}` has unknown refusal key `{}`",
                refusal.key
            ));
        }
        if seen_entries.contains(refusal.key.as_str()) {
            return Err(format!(
                "value ABI `{abi_name}` lists `{}` in both `entries` and `unsupported`",
                refusal.key
            ));
        }
        if !seen_refusals.insert(refusal.key.as_str()) {
            return Err(format!(
                "value ABI `{abi_name}` lists duplicate refusal key `{}`",
                refusal.key
            ));
        }
        if refusal.reason.trim().is_empty() {
            return Err(format!(
                "value ABI `{abi_name}` refusal for `{}` must declare a non-empty `reason`",
                refusal.key
            ));
        }
    }
    Ok(())
}
