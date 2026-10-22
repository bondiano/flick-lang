//! JS interface to the Flick core for the in-browser demo on the site:
//! compile-output preview and the REPL. Built for `wasm32-unknown-unknown`
//! via wasm-bindgen; the surface grows as the core learns real phases.

use wasm_bindgen::prelude::*;

/// Flick language version this build targets, e.g. `"0.7"`.
#[wasm_bindgen]
pub fn language_version() -> String {
    flick_core::LANGUAGE_VERSION.to_owned()
}
