//! Converts inspected Go exports, signatures, and type shapes into Flick
//! signatures, metadata lines, and opaque handle references.

use std::collections::BTreeSet;
use std::fmt::Write;

use super::generate::{Binding, Blocked, HandleKey, handle_placeholder};
use super::surface::{
    GoExport, GoExportKind, GoExportPolicy, GoPolicy, GoReceiver, GoSignature, GoTypeShape,
};

pub(super) fn convert_export(
    export: &GoExport,
    export_policy: &GoExportPolicy,
    policy: &GoPolicy,
    canonical_ptrs: &BTreeSet<(String, String)>,
) -> Result<Binding, Blocked> {
    match export.kind {
        GoExportKind::Var => Err(Blocked::unsupported(
            "package variable: mutable globals are not imported automatically",
        )),
        GoExportKind::Const => convert_const(export),
        GoExportKind::Func => {
            let sig = export
                .signature
                .as_ref()
                .ok_or_else(|| Blocked::unsupported("missing function signature"))?;
            convert_signature(sig, None, export_policy, policy, canonical_ptrs)
        }
        GoExportKind::Method => {
            let recv = export
                .receiver
                .as_ref()
                .ok_or_else(|| Blocked::unsupported("missing method receiver"))?;
            let sig = export
                .signature
                .as_ref()
                .ok_or_else(|| Blocked::unsupported("missing method signature"))?;
            convert_signature(sig, Some(recv), export_policy, policy, canonical_ptrs)
        }
    }
}

fn convert_const(export: &GoExport) -> Result<Binding, Blocked> {
    let ty = export
        .ty
        .as_ref()
        .ok_or_else(|| Blocked::unsupported("constant has no type"))?;
    if ty.kind != "basic" {
        return Err(Blocked::unsupported(format!(
            "constant of Go type `{}` is outside `flick.go-ffi`",
            ty.text
        )));
    }
    let basic = ty.basic.as_deref().unwrap_or("");
    let flick_ty = match basic {
        "bool" | "untyped_bool" => "Bool",
        "string" | "untyped_string" => "String",
        "float64" | "untyped_float" => "Float",
        "int32" | "rune" => "Int",
        "untyped_int" | "untyped_rune" => match export.int32_ok {
            Some(true) => "Int",
            Some(false) => {
                return Err(Blocked::unsupported(
                    "integer constant overflows 32-bit `Int` (`int32`)",
                ));
            }
            None => {
                return Err(Blocked::unsupported(
                    "untyped integer constant range could not be verified for `int32`",
                ));
            }
        },
        other => {
            return Err(Blocked::unsupported(format!(
                "constant of Go type `{other}` is outside `flick.go-ffi`"
            )));
        }
    };
    Ok(Binding {
        signature: flick_ty.to_owned(),
        mode: "pure",
        metadata: Vec::new(),
        handles: BTreeSet::new(),
        uses_default_go_error: false,
    })
}

fn convert_signature(
    sig: &GoSignature,
    recv: Option<&GoReceiver>,
    export_policy: &GoExportPolicy,
    policy: &GoPolicy,
    canonical_ptrs: &BTreeSet<(String, String)>,
) -> Result<Binding, Blocked> {
    if let Some(r) = recv
        && r.generic
    {
        return Err(Blocked::unsupported(format!(
            "method on generic Go type `{}` is not supported",
            r.name
        )));
    }
    if !sig.type_params.is_empty() {
        return Err(Blocked::unsupported(format!(
            "generic Go function with type parameter(s) `[{}]` is not supported",
            sig.type_params.join(", ")
        )));
    }

    let mut handles = BTreeSet::new();
    let (param_types, is_async) = convert_params(sig, recv, policy, canonical_ptrs, &mut handles)?;
    let (result_type, has_error, is_nullable) =
        convert_results(&sig.results, export_policy, policy, &mut handles)?;

    let pure = export_policy.mode.as_deref() == Some("pure");
    if pure && (is_async || has_error) {
        return Err(Blocked::unsupported(
            "`mode = \"pure\"` cannot be used on a function with `context.Context` or trailing `error`",
        ));
    }

    let mut metadata = vec![format!("arity {}", param_types.len())];
    if recv.is_some() {
        metadata.push("this".to_owned());
    }
    if sig.variadic {
        metadata.push("variadic".to_owned());
    }

    let mut uses_default_go_error = false;
    let error_type = if has_error {
        let catch_fn = export_policy.catch.as_deref().unwrap_or("from_go_error");
        let err_ty = export_policy.error.as_deref().unwrap_or("GoError");
        if err_ty == "GoError" || catch_fn == "from_go_error" {
            uses_default_go_error = true;
        }
        metadata.push(format!("catch {catch_fn}"));
        Some(err_ty)
    } else {
        None
    };

    if is_async {
        metadata.push("cancel context".to_owned());
    }
    if is_nullable {
        metadata.push("nullable".to_owned());
    }

    let mode = if is_async {
        "async"
    } else if pure {
        "pure"
    } else {
        "sync"
    };

    let signature = format_signature(param_types, mode, error_type, &result_type);
    Ok(Binding {
        signature,
        mode,
        metadata,
        handles,
        uses_default_go_error,
    })
}

fn convert_params(
    sig: &GoSignature,
    recv: Option<&GoReceiver>,
    policy: &GoPolicy,
    canonical_ptrs: &BTreeSet<(String, String)>,
    handles: &mut BTreeSet<HandleKey>,
) -> Result<(Vec<String>, bool), Blocked> {
    let mut param_types = Vec::new();
    if let Some(r) = recv {
        let use_ptr = !r.is_interface
            && (r.ptr || canonical_ptrs.contains(&(r.package.clone(), r.name.clone())));
        let key = HandleKey {
            pkg: r.package.clone(),
            name: r.name.clone(),
            ptr: use_ptr,
        };
        let recv_ty = policy
            .of_type(&key.go_form())
            .or_else(|| policy.of_type(&key.name))
            .and_then(|p| p.convert.clone())
            .unwrap_or_else(|| {
                let ph = handle_placeholder(&key);
                handles.insert(key);
                ph
            });
        param_types.push(recv_ty);
    }

    let mut params_slice = sig.params.as_slice();
    let is_async = if let Some(first) = params_slice.first()
        && first.ty.is_context
    {
        params_slice = &params_slice[1..];
        true
    } else {
        false
    };

    for (i, p) in params_slice.iter().enumerate() {
        let is_variadic_param = sig.variadic && i + 1 == params_slice.len();
        if p.ty.is_context {
            return Err(Blocked::unsupported(
                "`context.Context` is only supported as the leading parameter",
            ));
        }
        let converted = if is_variadic_param {
            convert_variadic_param(&p.ty, policy, handles)?
        } else {
            convert_param_type(&p.ty, policy, handles)?
        };
        param_types.push(converted);
    }
    Ok((param_types, is_async))
}

fn convert_results(
    results: &[GoTypeShape],
    export_policy: &GoExportPolicy,
    policy: &GoPolicy,
    handles: &mut BTreeSet<HandleKey>,
) -> Result<(String, bool, bool), Blocked> {
    let mut results_slice = results;
    let has_error = if let Some(last) = results_slice.last()
        && last.is_error
    {
        results_slice = &results_slice[..results_slice.len() - 1];
        true
    } else {
        false
    };
    if results_slice.iter().any(|r| r.is_error) {
        return Err(Blocked::unsupported(
            "`error` is only supported as the trailing result",
        ));
    }

    let mut is_nullable = false;
    let result_type = match results_slice {
        [] => "Unit".to_owned(),
        [only] => convert_result_type(only, export_policy, policy, handles, &mut is_nullable)?,
        [val, ok] if ok.kind == "basic" && ok.basic.as_deref() == Some("bool") => {
            let inner = convert_value_type(val, policy, handles)?;
            format!("Option {}", paren_app(&inner))
        }
        many => {
            return Err(Blocked::unsupported(format!(
                "returns {} values; only 0, 1, `(T, bool)`, and trailing `error` are supported",
                many.len()
            )));
        }
    };
    Ok((result_type, has_error, is_nullable))
}

fn convert_variadic_param(
    ty: &GoTypeShape,
    policy: &GoPolicy,
    handles: &mut BTreeSet<HandleKey>,
) -> Result<String, Blocked> {
    if ty.kind != "slice" {
        return Err(Blocked::unsupported(format!(
            "unexpected variadic parameter type `{}`",
            ty.text
        )));
    }
    let elem = ty
        .elem
        .as_deref()
        .ok_or_else(|| Blocked::unsupported("variadic slice has no element type"))?;
    if elem.kind == "empty_interface" {
        return Err(Blocked::unsupported(
            "variadic `any` parameter is not supported",
        ));
    }
    let inner = convert_value_type(elem, policy, handles)?;
    Ok(format!("List {}", paren_app(&inner)))
}

fn convert_param_type(
    ty: &GoTypeShape,
    policy: &GoPolicy,
    handles: &mut BTreeSet<HandleKey>,
) -> Result<String, Blocked> {
    if ty.kind == "pointer" {
        let elem = ty
            .elem
            .as_deref()
            .ok_or_else(|| Blocked::unsupported("pointer has no element type"))?;
        if let Some(opt_inner) = try_unambiguous_ptr_elem(elem, policy, handles)? {
            return Ok(format!("Option {}", paren_app(&opt_inner)));
        }
    }
    convert_value_type(ty, policy, handles)
}

fn convert_result_type(
    ty: &GoTypeShape,
    export_policy: &GoExportPolicy,
    policy: &GoPolicy,
    handles: &mut BTreeSet<HandleKey>,
    is_nullable: &mut bool,
) -> Result<String, Blocked> {
    if ty.kind == "pointer" {
        let elem = ty
            .elem
            .as_deref()
            .ok_or_else(|| Blocked::unsupported("pointer has no element type"))?;
        if let Some(opt_inner) = try_unambiguous_ptr_elem(elem, policy, handles)? {
            if export_policy.allows_nullable() {
                *is_nullable = true;
                return Ok(format!("Option {}", paren_app(&opt_inner)));
            }
            return Err(Blocked::policy(format!(
                "returns pointer `{}` which may be nil: set `nullable = true` (or `undefined = \"none\"`) in `policy.toml`",
                ty.text
            )));
        }
        // Pointer to a named struct/type becomes an opaque handle, or `Option Handle`
        // with `nullable` when the policy opts in.
        let handle = convert_value_type(ty, policy, handles)?;
        if export_policy.allows_nullable() {
            *is_nullable = true;
            return Ok(format!("Option {}", paren_app(&handle)));
        }
        return Ok(handle);
    }
    let converted = convert_value_type(ty, policy, handles)?;
    if ty.kind == "named" && ty.is_interface() && export_policy.allows_nullable() {
        *is_nullable = true;
        return Ok(format!("Option {}", paren_app(&converted)));
    }
    Ok(converted)
}

/// If `elem` is a non-opaque `flick.go-ffi` ABI type where `*T` unambiguously
/// represents `Option T` (`bool`, `int32`/`rune`, `float64`, `string`, `[]byte`, `[]U`),
/// converts it and returns `Some(flick_ty)`.
fn try_unambiguous_ptr_elem(
    elem: &GoTypeShape,
    policy: &GoPolicy,
    handles: &mut BTreeSet<HandleKey>,
) -> Result<Option<String>, Blocked> {
    match elem.kind.as_str() {
        "basic" => match elem.basic.as_deref() {
            Some("bool" | "int32" | "rune" | "float64" | "string") => {
                convert_value_type(elem, policy, handles).map(Some)
            }
            _ => Ok(None),
        },
        "slice" => convert_value_type(elem, policy, handles).map(Some),
        _ => Ok(None),
    }
}

fn convert_value_type(
    ty: &GoTypeShape,
    policy: &GoPolicy,
    handles: &mut BTreeSet<HandleKey>,
) -> Result<String, Blocked> {
    match ty.kind.as_str() {
        "basic" => match ty.basic.as_deref().unwrap_or("") {
            "bool" => Ok("Bool".to_owned()),
            "int32" | "rune" => Ok("Int".to_owned()),
            "float64" => Ok("Float".to_owned()),
            "string" => Ok("String".to_owned()),
            other => Err(Blocked::unsupported(format!(
                "Go `{other}` is outside `flick.go-ffi` (Flick `Int` is `int32` and `Float` is `float64`)"
            ))),
        },
        "slice" => {
            let elem = ty
                .elem
                .as_deref()
                .ok_or_else(|| Blocked::unsupported("slice has no element type"))?;
            if elem.kind == "basic" && matches!(elem.basic.as_deref(), Some("byte" | "uint8")) {
                return Ok("Bytes".to_owned());
            }
            let inner = convert_param_type(elem, policy, handles)?;
            Ok(format!("List {}", paren_app(&inner)))
        }
        "pointer" => {
            let elem = ty
                .elem
                .as_deref()
                .ok_or_else(|| Blocked::unsupported("pointer has no element type"))?;
            if elem.kind == "named" {
                return convert_named(elem, true, policy, handles);
            }
            Err(Blocked::unsupported(format!(
                "Go pointer type `{}` is outside `flick.go-ffi`",
                ty.text
            )))
        }
        "named" => convert_named(ty, false, policy, handles),
        "empty_interface" => Err(Blocked::unsupported(
            "uses `any` / `interface{}`, which has no safe Flick type",
        )),
        "chan" => Err(Blocked::unsupported(format!(
            "uses Go channel type `{}`",
            ty.text
        ))),
        "type_param" => Err(Blocked::unsupported(format!(
            "uses generic type parameter `{}`",
            ty.text
        ))),
        "map" => Err(Blocked::unsupported(format!(
            "uses Go map type `{}`",
            ty.text
        ))),
        "array" => Err(Blocked::unsupported(format!(
            "uses fixed-size Go array `{}`",
            ty.text
        ))),
        "func" => Err(Blocked::unsupported(format!(
            "uses Go function type `{}`",
            ty.text
        ))),
        "struct_anon" => Err(Blocked::unsupported("uses unnamed Go struct type")),
        "interface_anon" => Err(Blocked::unsupported("uses unnamed Go interface type")),
        "unsafe_pointer" => Err(Blocked::unsupported("uses `unsafe.Pointer`")),
        _ => Err(Blocked::unsupported(format!(
            "Go type `{}` is not supported",
            ty.text
        ))),
    }
}

fn convert_named(
    ty: &GoTypeShape,
    is_ptr: bool,
    policy: &GoPolicy,
    handles: &mut BTreeSet<HandleKey>,
) -> Result<String, Blocked> {
    if ty.is_error {
        return Err(Blocked::unsupported(
            "`error` is only supported as the trailing result",
        ));
    }
    if ty.is_context {
        return Err(Blocked::unsupported(
            "`context.Context` is only supported as the leading parameter",
        ));
    }
    if !ty.exported {
        return Err(Blocked::unsupported(format!(
            "uses unexported Go type `{}`",
            ty.text
        )));
    }
    if !ty.type_args.is_empty() {
        return Err(Blocked::unsupported(format!(
            "uses generic Go type `{}`",
            ty.text
        )));
    }
    if ty.underlying.as_deref() == Some("chan") {
        return Err(Blocked::unsupported(format!(
            "uses Go channel type `{}`",
            ty.text
        )));
    }
    let name = ty
        .name
        .as_deref()
        .ok_or_else(|| Blocked::unsupported("named Go type has no name"))?;
    let pkg = ty.package.as_deref().unwrap_or("");
    let form_key = if is_ptr {
        format!("*{name}")
    } else {
        name.to_owned()
    };
    if let Some(conv) = policy
        .of_type(&form_key)
        .or_else(|| policy.of_type(name))
        .and_then(|p| p.convert.as_deref())
    {
        return Ok(conv.to_owned());
    }
    if pkg.is_empty() {
        return Err(Blocked::unsupported(format!(
            "built-in Go type `{}` is outside `flick.go-ffi`",
            ty.text
        )));
    }
    let key = HandleKey {
        pkg: pkg.to_owned(),
        name: name.to_owned(),
        ptr: is_ptr,
    };
    let placeholder = handle_placeholder(&key);
    handles.insert(key);
    Ok(placeholder)
}

fn paren_app(ty: &str) -> String {
    if ty.contains(' ') {
        format!("({ty})")
    } else {
        ty.to_owned()
    }
}

fn format_signature(
    mut params: Vec<String>,
    mode: &str,
    error: Option<&str>,
    result: &str,
) -> String {
    if params.is_empty() {
        params.push("Unit".to_owned());
    }
    let last = params.pop().unwrap_or_default();
    let mut out = String::new();
    for p in &params {
        let _ = write!(out, "{p} -> ");
    }
    let arrow = match (mode, error) {
        ("pure", _) => " -> ".to_owned(),
        ("async", None) => " ->{Async} ".to_owned(),
        ("async", Some(e)) => format!(" ->{{Async; {e}}} "),
        (_, None) => " ->{} ".to_owned(),
        (_, Some(e)) => format!(" ->{{; {e}}} "),
    };
    let _ = write!(out, "{last}{arrow}{result}");
    out
}
