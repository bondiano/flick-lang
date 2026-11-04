//! Plugins: blocks `head { … }` whose code a plugin gives back. The code
//! comes as an arena of core syntax ([`flick_plugin::Arena`]); it is
//! rendered as text after the module's own, parsed and spliced in place of
//! the block, the user's code inside keeping its own place in the file.

pub mod render;
pub mod source;
pub mod splice;

#[cfg(test)]
mod tests;
