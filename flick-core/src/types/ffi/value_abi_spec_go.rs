//! Official `flick/go` versioned value-ABI specification (`flick.go-ffi@0.1.0`).
//! Must stay in sync with `backends/go/src/component/value_abi.rs`.

use super::{
    AbiConversion, AbiDirection, ValueAbiEntrySpec, ValueAbiId, ValueAbiRefusalSpec, ValueAbiSpec,
};

const GO_ENTRIES: &[(&str, &str, &str, AbiConversion, AbiDirection, &str)] = &[
    (
        "unit",
        "Unit",
        "()",
        AbiConversion::None,
        AbiDirection::Both,
        "No Go parameter with `arity 0`, or no Go return value (bare `error` with `catch`); a successful Flick result is `()`",
    ),
    (
        "bool",
        "Bool",
        "bool",
        AbiConversion::None,
        AbiDirection::Both,
        "Go `bool` primitive",
    ),
    (
        "int",
        "Int",
        "int32",
        AbiConversion::None,
        AbiDirection::Both,
        "Go signed 32-bit integer `int32`",
    ),
    (
        "float",
        "Float",
        "float64",
        AbiConversion::None,
        AbiDirection::Both,
        "Go IEEE 754 binary64 `float64`",
    ),
    (
        "string",
        "String",
        "string",
        AbiConversion::None,
        AbiDirection::Both,
        "Go `string` value",
    ),
    (
        "bytes",
        "Bytes",
        "[]byte",
        AbiConversion::Copy,
        AbiDirection::Both,
        "Freshly copied Go `[]byte` slice; host mutations never alias immutable Flick `Bytes`",
    ),
    (
        "list",
        "List a",
        "[]T",
        AbiConversion::Copy,
        AbiDirection::Both,
        "Freshly copied Go slice `[]T` with recursive element conversion; host mutations never alias Flick `List`",
    ),
    (
        "option",
        "Option a",
        "*T or (T, bool)",
        AbiConversion::Copy,
        AbiDirection::Both,
        "Go pointer `*T` when `nil` unambiguously represents `None` (parameters, slice elements, and `nullable` returns); presence flag `(T, bool)` / `struct{ Val T; Ok bool }` for comma-ok returns, nested `Option`, and opaque payloads; trailing `error` maps via `catch` and `Async` passes a leading `context.Context` cancelled on interruption",
    ),
];

const GO_UNSUPPORTED: &[(&str, &str)] = &[
    (
        "bigint",
        "direct Go bindings do not convert `BigInt`; use a sibling `.go` file or `from inline` with `flickrt.Value`",
    ),
    (
        "record",
        "structural records have no stable Go struct layout; use an opaque handle or a sibling `.go` file with `flickrt.Value`",
    ),
    (
        "adt",
        "user-defined ADTs have no stable Go layout; use an opaque handle or a sibling `.go` file with `flickrt.Value`",
    ),
    (
        "tuple",
        "direct Go bindings do not convert tuples; use a sibling `.go` file or `from inline` with `flickrt.Value`",
    ),
    (
        "function",
        "direct Go bindings do not adapt higher-order callback parameters or function results; use a sibling `.go` file or `from inline` with `flickrt.Value`",
    ),
    (
        "result",
        "direct Go bindings map trailing Go `error` via `catch` rather than `Result e a`; use `catch` or a sibling `.go` file with `flickrt.Value`",
    ),
    (
        "js_value",
        "`JsValue` is specific to `flick/js` and not supported by direct Go bindings",
    ),
    (
        "unknown",
        "direct Go bindings do not convert `Unknown`; use a sibling `.go` file or `from inline` with `flickrt.Value`",
    ),
    ("never", "direct Go bindings do not map uninhabited `Never`"),
];

impl ValueAbiSpec {
    /// The official `flick/go` value-ABI table (`flick.go-ffi@0.1.0`).
    /// Must stay in sync with `backends/go/src/component/value_abi.rs`.
    pub fn go() -> Self {
        Self {
            abi: ValueAbiId {
                name: "flick.go-ffi".to_owned(),
                version: "0.1.0".to_owned(),
            },
            opaque_type: "flickrt.Value".to_owned(),
            catch_error_type: "error".to_owned(),
            catch_lang_item: Some("string".to_owned()),
            entries: GO_ENTRIES
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
            unsupported: GO_UNSUPPORTED
                .iter()
                .map(|&(key, reason)| ValueAbiRefusalSpec {
                    key: key.to_owned(),
                    reason: reason.to_owned(),
                })
                .collect(),
            fallback_reason:
                "type is outside the `flick.go-ffi` value ABI table; use a sibling `.go` file or `from inline` with `flickrt.Value`"
                    .to_owned(),
            admits_copied_data: false,
        }
    }
}
