//! The editor features the playground asks of a session: what each name is,
//! the types the code leaves out, the signature of the call at the cursor and
//! the rewrites offered for a range. Offsets cross the boundary as UTF-16
//! units, like every other position of the module.

use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

use flick_core::ide::{Ide, TOKEN_TYPES};

use crate::{Session, byte_offset, utf16_offset};

#[cfg(test)]
#[path = "field_tests.rs"]
mod field_tests;
#[cfg(test)]
#[path = "ide_tests.rs"]
mod ide_tests;

#[wasm_bindgen]
impl Session {
    /// `{ "types": [String], "tokens": [{ "from", "to", "type" }] }`: the
    /// names the checker resolved, by their index in `types`.
    #[must_use]
    pub fn semantic_tokens(&self) -> String {
        let Some(file) = self.file() else {
            return json!({ "types": TOKEN_TYPES, "tokens": [] }).to_string();
        };
        let ide = Ide {
            analysis: &self.analysis,
            sources: &self.sources,
        };
        let tokens: Vec<Value> = ide
            .semantic_tokens(file)
            .into_iter()
            .map(|(start, end, kind)| {
                json!({
                    "from": utf16_offset(&self.text, start),
                    "to": utf16_offset(&self.text, end),
                    "type": TOKEN_TYPES.get(kind as usize).copied().unwrap_or("variable"),
                })
            })
            .collect();
        json!({ "types": TOKEN_TYPES, "tokens": tokens }).to_string()
    }

    /// `{ "label", "params": [{ "from", "to" }], "active" }` for the call at
    /// the UTF-16 offset `at`; `null` when there is none.
    #[must_use]
    pub fn signature_help(&self, at: u32) -> String {
        let Some(file) = self.file() else {
            return Value::Null.to_string();
        };
        let ide = Ide {
            analysis: &self.analysis,
            sources: &self.sources,
        };
        ide.signature_help(file, byte_offset(&self.text, at))
            .map_or(Value::Null, |help| {
                // the parameters are ranges of `label`, which the tooltip
                // highlights as it is written, not of the module
                let params: Vec<Value> = help
                    .params
                    .into_iter()
                    .map(|(start, end)| json!({ "from": start, "to": end }))
                    .collect();
                json!({ "label": help.label, "params": params, "active": help.active })
            })
            .to_string()
    }

    /// A JSON array of `{ "title", "kind", "edits": [{ "from", "to", "text" }] }`:
    /// the rewrites offered for the UTF-16 range `from..to`.
    #[must_use]
    pub fn code_actions(&self, from: u32, to: u32) -> String {
        let Some(file) = self.file() else {
            return Value::Array(Vec::new()).to_string();
        };
        let ide = Ide {
            analysis: &self.analysis,
            sources: &self.sources,
        };
        let start = byte_offset(&self.text, from);
        let end = byte_offset(&self.text, to).max(start);
        let actions: Vec<Value> = ide
            .actions(file, start, end)
            .into_iter()
            .map(|action| {
                let edits: Vec<Value> = action
                    .edits
                    .into_iter()
                    .map(|edit| {
                        json!({
                            "from": utf16_offset(&self.text, edit.span.start),
                            "to": utf16_offset(&self.text, edit.span.end),
                            "text": edit.replacement,
                        })
                    })
                    .collect();
                json!({ "title": action.title, "kind": action.kind, "edits": edits })
            })
            .collect();
        Value::Array(actions).to_string()
    }
}
