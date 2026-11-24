//! `FuncRef` against value-ABI tables: a backend that lists `func_ref` takes
//! one as the function it refers to; one that does not refuses it, as it
//! has no opaque handle for a function pointer.

use super::{check_with, strict_backend};
use crate::diagnostics::codes;
use crate::types::ffi::ForeignBackendSpec;

const SOURCE: &str = "module Probe exposing (f)\nf : FuncRef (Int -> Int) ->{} Unit\nforeign f from \"./lib.c\" \"f\"\n";

#[test]
fn js_value_abi_admits_a_func_ref() {
    let diags = check_with(SOURCE, &ForeignBackendSpec::js());
    assert!(diags.is_empty(), "{diags:?}");
}

#[test]
fn a_value_abi_without_func_ref_refuses_it() {
    let diags = check_with(SOURCE, &strict_backend());
    let f013 = diags
        .iter()
        .find(|d| d.code == codes::F013)
        .unwrap_or_else(|| panic!("expected FLICK-F013, got {diags:?}"));
    assert!(
        f013.message.contains("FuncRef (Int -> Int)") && f013.message.contains("test.strict-ffi"),
        "{}",
        f013.message
    );
}
