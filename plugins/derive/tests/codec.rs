//! The code `codec` blocks stand for, and what is wrong with the ones that
//! stand for none.

use flick_core::plugin::testing::expand;
use flick_plugin::{Arena, Block, Compiler, Diagnostic, Plugin};
use flick_plugin_derive::codec;

struct Codecs;

impl Plugin for Codecs {
    fn collect(&self, _: &[Block]) -> Vec<u8> {
        Vec::new()
    }

    fn expand(
        &self,
        block: &Block,
        _: &[u8],
        compiler: &mut dyn Compiler,
    ) -> Result<Arena, Vec<Diagnostic>> {
        codec::expand(block, compiler)
    }
}

/// What the one block of a module with `value` stands for.
fn one(value: &str) -> Result<String, Vec<String>> {
    let source = format!(
        "module App.Main exposing (..)\n\nimport Codec exposing (plugin codec)\n\nlet c = {value}\n"
    );
    let mut found = expand(&Codecs, &source);
    assert_eq!(found.len(), 1, "{found:?}");
    found.remove(0)
}

#[test]
fn a_block_gathers_its_fields_in_the_written_order() {
    let code = one(
        "codec Profile.schema {\n  email = field \"email\" String.json;\n  nickname = optional_field \"nick\" String.json;\n}",
    );
    assert_eq!(
        code.as_deref(),
        Ok("Codec.Derive.record
  \"Profile\"
  (Profile.schema : (Schema _))
  (Codec.Derive.field
    \"nickname\"
    ((optional_field \"nick\" String.json) : (FieldCodec _ _))
    (Codec.Derive.field
      \"email\"
      ((field \"email\" String.json) : (FieldCodec _ _))
      Codec.Derive.fields))")
    );
}

#[test]
fn a_field_that_needs_services_makes_a_codec_of_them() {
    let code = one(
        "codec Profile.schema {\n  email = field \"email\" String.json |> on_missing_effect (fun () -> Directory.lookup ());\n}",
    )
    .expect("code");
    assert!(code.starts_with("Codec.Derive.record_effect\n"), "{code}");
}

#[test]
fn a_label_or_a_wire_name_given_twice_is_refused() {
    let errors = one(
        "codec Profile.schema {\n  email = field \"email\" String.json;\n  email = field \"mail\" String.json;\n  nickname = optional_field \"email\" String.json;\n}",
    )
    .expect_err("errors");
    assert_eq!(
        errors,
        [
            "D003 `email = field \"mail\" String.json`: `email` is given twice",
            "D005 `optional_field \"email\" String.json`: the wire name \"email\" is already a field's",
        ]
    );
}
