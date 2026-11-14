//! Plain TypeScript projection consumed by the JavaScript emitter.

use std::collections::HashMap;

/// The TypeScript of a module beyond its JavaScript.
#[derive(Clone)]
pub struct Typed {
    pub declarations: Vec<String>,
    /// The type of each exported value and constructor that has one, by its
    /// JS name; the others stay `any`.
    pub annotations: HashMap<String, String>,
    /// The imports of the types the declarations and annotations name.
    pub imports: String,
    /// The modules those imports name.
    pub modules: Vec<String>,
}
