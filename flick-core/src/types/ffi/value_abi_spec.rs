//! Versioned backend value-ABI specification mirrored from `flick:backend/types`.

#[cfg(feature = "native-project")]
use serde::{Deserialize, Serialize};

/// How a value crosses a foreign boundary under a backend's value ABI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "native-project", derive(Serialize, Deserialize))]
pub enum AbiConversion {
    None,
    Copy,
    View,
}

impl AbiConversion {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Copy => "copy",
            Self::View => "view",
        }
    }
}

/// Direction(s) in which a value-ABI entry is valid at a foreign boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "native-project", derive(Serialize, Deserialize))]
pub enum AbiDirection {
    /// Passing a value from Flick into host code (parameters, callback results).
    In,
    /// Receiving a value from host code into Flick (call results, callback parameters).
    Out,
    /// Valid in both directions.
    Both,
}

impl AbiDirection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::In => "in",
            Self::Out => "out",
            Self::Both => "both",
        }
    }

    pub const fn allows(self, needed: Self) -> bool {
        matches!(
            (self, needed),
            (Self::Both, _) | (Self::In, Self::In) | (Self::Out, Self::Out)
        )
    }

    #[must_use]
    pub const fn flip(self) -> Self {
        match self {
            Self::In => Self::Out,
            Self::Out => Self::In,
            Self::Both => Self::Both,
        }
    }
}

/// Versioned ABI identifier (`name@version`), matching `foreign-binding.abi`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "native-project", derive(Serialize, Deserialize))]
pub struct ValueAbiId {
    pub name: String,
    pub version: String,
}

/// One entry in a backend's versioned value-ABI table.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "native-project", derive(Serialize, Deserialize))]
pub struct ValueAbiEntrySpec {
    pub key: String,
    pub flick_type: String,
    pub host_type: String,
    pub conversion: AbiConversion,
    pub direction: AbiDirection,
    pub notes: String,
}

/// Explicit backend refusal reason for a type key outside `entries`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "native-project", derive(Serialize, Deserialize))]
pub struct ValueAbiRefusalSpec {
    pub key: String,
    pub reason: String,
}

/// Versioned value-ABI table published by a backend descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "native-project", derive(Serialize, Deserialize))]
pub struct ValueAbiSpec {
    pub abi: ValueAbiId,
    pub opaque_type: String,
    pub catch_error_type: String,
    pub catch_lang_item: Option<String>,
    pub entries: Vec<ValueAbiEntrySpec>,
    pub unsupported: Vec<ValueAbiRefusalSpec>,
    pub fallback_reason: String,
    pub admits_copied_data: bool,
}

const JS_ENTRIES: &[(&str, &str, &str, AbiConversion, AbiDirection, &str)] = &[
    (
        "unit",
        "Unit",
        "undefined",
        AbiConversion::None,
        AbiDirection::Both,
        "JS `undefined`; a successful foreign result of type `Unit` is `()` whatever JS returns",
    ),
    (
        "bool",
        "Bool",
        "boolean",
        AbiConversion::None,
        AbiDirection::Both,
        "JS `boolean` primitive",
    ),
    (
        "int",
        "Int",
        "number",
        AbiConversion::None,
        AbiDirection::Both,
        "JS `number` kept in 32-bit signed integer range (`Int32`)",
    ),
    (
        "float",
        "Float",
        "number",
        AbiConversion::None,
        AbiDirection::Both,
        "JS IEEE 754 binary64 `number`",
    ),
    (
        "bigint",
        "BigInt",
        "bigint",
        AbiConversion::None,
        AbiDirection::Both,
        "JS arbitrary-precision `bigint`",
    ),
    (
        "int8",
        "Int8",
        "number",
        AbiConversion::None,
        AbiDirection::Both,
        "JS `number` masked/sign-extended to signed 8-bit integer",
    ),
    (
        "int16",
        "Int16",
        "number",
        AbiConversion::None,
        AbiDirection::Both,
        "JS `number` masked/sign-extended to signed 16-bit integer",
    ),
    (
        "int64",
        "Int64",
        "bigint",
        AbiConversion::None,
        AbiDirection::Both,
        "JS `bigint` wrapped to signed 64-bit integer",
    ),
    (
        "uint8",
        "UInt8 (Byte)",
        "number",
        AbiConversion::None,
        AbiDirection::Both,
        "JS `number` masked to unsigned 8-bit integer",
    ),
    (
        "uint16",
        "UInt16",
        "number",
        AbiConversion::None,
        AbiDirection::Both,
        "JS `number` masked to unsigned 16-bit integer",
    ),
    (
        "uint32",
        "UInt32",
        "number",
        AbiConversion::None,
        AbiDirection::Both,
        "JS `number` masked to unsigned 32-bit integer",
    ),
    (
        "uint64",
        "UInt64",
        "bigint",
        AbiConversion::None,
        AbiDirection::Both,
        "JS `bigint` wrapped to unsigned 64-bit integer",
    ),
    (
        "float32",
        "Float32",
        "number",
        AbiConversion::None,
        AbiDirection::Both,
        "JS `number` rounded to IEEE 754 binary32",
    ),
    (
        "string",
        "String",
        "string",
        AbiConversion::None,
        AbiDirection::Both,
        "JS UTF-16 `string` primitive",
    ),
    (
        "never",
        "Never",
        "never",
        AbiConversion::None,
        AbiDirection::Both,
        "Uninhabited bottom type",
    ),
    (
        "js_value",
        "JsValue",
        "unknown",
        AbiConversion::None,
        AbiDirection::Both,
        "Opaque `JsValue` passed as-is until a checked snapshot",
    ),
    (
        "unknown",
        "Unknown",
        "unknown",
        AbiConversion::Copy,
        AbiDirection::Both,
        "Inert immutable Flick value; deep-copied across the boundary",
    ),
    (
        "tuple",
        "(a, b, …)",
        "readonly [A, B, …]",
        AbiConversion::Copy,
        AbiDirection::Both,
        "Dense JS array of fixed length; deep-copied across the boundary",
    ),
    (
        "list",
        "List a",
        "ReadonlyArray<A>",
        AbiConversion::Copy,
        AbiDirection::Both,
        "Dense JS array with an immutable source API; deep-copied across the boundary",
    ),
    (
        "option",
        "Option a",
        "Option.Option<A>",
        AbiConversion::Copy,
        AbiDirection::Both,
        "Native Effect `Option`; `nullable` reads `null`/`undefined` as `None` and `optional i` passes `None` as `undefined`",
    ),
    (
        "result",
        "Result e a",
        "Result.Result<A, E>",
        AbiConversion::Copy,
        AbiDirection::Both,
        "Native Effect `Result` representation of the pinned Effect version",
    ),
    (
        "eff",
        "Eff r e a",
        "Effect.Effect<A, E, R>",
        AbiConversion::None,
        AbiDirection::Both,
        "Native Effect `Effect` representation of the pinned Effect version",
    ),
    (
        "layer",
        "Layer ri e ro",
        "Layer.Layer<RO, E, RI>",
        AbiConversion::None,
        AbiDirection::Both,
        "Native Effect `Layer` representation of the pinned Effect version",
    ),
    (
        "exit",
        "Exit e a",
        "Exit.Exit<A, E>",
        AbiConversion::None,
        AbiDirection::Both,
        "Native Effect `Exit` representation of the pinned Effect version",
    ),
    (
        "failure",
        "Failure e",
        "Cause.Cause<E>",
        AbiConversion::None,
        AbiDirection::Both,
        "Native Effect `Cause` representation of the pinned Effect version",
    ),
    (
        "func_ref",
        "FuncRef (a -> b)",
        "(a, …) => b",
        AbiConversion::None,
        AbiDirection::Both,
        "The function itself, without captures; adapted to JS arity as a callback is",
    ),
    (
        "function",
        "a -> b, a ->{r; e} b",
        "(a, …) => b",
        AbiConversion::None,
        AbiDirection::Both,
        "Callbacks adapted to JS arity across four families: pure, Effect, Promise, scoped event",
    ),
];

impl ValueAbiSpec {
    /// Looks up a direct entry by lang item or structural constructor key.
    pub fn entry(&self, key: &str) -> Option<&ValueAbiEntrySpec> {
        self.entries.iter().find(|e| e.key == key)
    }

    /// Looks up the explicit backend refusal reason for `key`, if declared.
    pub fn unsupported_reason(&self, key: &str) -> Option<&str> {
        self.unsupported
            .iter()
            .find(|r| r.key == key)
            .map(|r| r.reason.as_str())
    }

    /// Returns the backend's refusal reason for `key`, falling back to `fallback_reason`.
    pub fn refusal_reason(&self, key: &str) -> &str {
        self.unsupported_reason(key)
            .unwrap_or(self.fallback_reason.as_str())
    }

    /// The official `flick/js` value-ABI table (`flick.javascript-ffi@0.1.0`).
    /// Must stay in sync with `backends/js/src/component/value_abi.rs`.
    pub fn js() -> Self {
        Self {
            abi: ValueAbiId {
                name: "flick.javascript-ffi".to_owned(),
                version: "0.1.0".to_owned(),
            },
            opaque_type: "unknown".to_owned(),
            catch_error_type: "unknown".to_owned(),
            catch_lang_item: Some("js_value".to_owned()),
            entries: JS_ENTRIES
                .iter()
                .map(
                    |&(key, flick_type, host_type, conversion, direction, notes)| {
                        ValueAbiEntrySpec {
                            key: key.to_owned(),
                            flick_type: flick_type.to_owned(),
                            host_type: host_type.to_owned(),
                            conversion,
                            direction,
                            notes: notes.to_owned(),
                        }
                    },
                )
                .collect(),
            unsupported: Vec::new(),
            fallback_reason: "type is outside the `flick.javascript-ffi` value ABI table"
                .to_owned(),
            admits_copied_data: true,
        }
    }
}
