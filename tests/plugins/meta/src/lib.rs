//! External plugin fixture exercising `flick:plugin@0.6.0` declaration metadata,
//! plugin-declared `heads`, `watch`, and custom `@attributes`, and two realistic
//! external derives:
//! - `Meta.Codec`: configurable-field encode/decode with `@tag`, `@meta.prefix`,
//!   `@rename`, `@skip`, `@default` (plus `deriving Meta.Codec { ... }`), exact
//!   field/attribute-span diagnostics, and refusal to leak opaque/private types.
//! - `Meta.CliForm`: CLI/form help and argument parser driven by `///` doc
//!   comments on types, variants, and record fields, preserving declaration
//!   order via `source_index` and reading variant `@alias` and field `@rename`
//!   attributes.

mod cli_form;
mod codec;
mod codec_emit;

use flick_plugin::{
    Arena, AttributeSpec, AttributeTarget, Block, Builder, CallSite, Compiler, Constant,
    DeriveTarget, Diagnostic, Expr, HeadSpec, Plugin, PluginMetadata, TokenKind, flick,
};

pub struct Meta;

#[cfg(feature = "guest")]
flick_plugin::export!(Meta);

impl Plugin for Meta {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            heads: vec![
                HeadSpec {
                    head: "route".into(),
                    module: "Meta".into(),
                    declarations: vec!["fn".into()],
                    ..HeadSpec::default()
                },
                HeadSpec {
                    head: "widget".into(),
                    module: "Meta".into(),
                    named: true,
                    ..HeadSpec::default()
                },
                HeadSpec {
                    head: "ref_of".into(),
                    module: "Meta".into(),
                    bare: true,
                    ..HeadSpec::default()
                },
            ],
            watch: vec!["Meta.guarded".into()],
            derives: vec!["Meta.Codec".into(), "Meta.CliForm".into()],
            own_codes: Vec::new(),
            attributes: vec![
                AttributeSpec {
                    name: "tag".into(),
                    targets: vec![AttributeTarget::Declaration],
                    doc: "Prefixes encoded wire values with a type tag.".into(),
                },
                AttributeSpec {
                    name: "meta.prefix".into(),
                    targets: vec![AttributeTarget::Declaration],
                    doc: "Namespaced prefix for encoded wire values.".into(),
                },
                AttributeSpec {
                    name: "rename".into(),
                    targets: vec![AttributeTarget::RecordField],
                    doc: "Overrides the wire key or CLI flag name of a record field.".into(),
                },
                AttributeSpec {
                    name: "skip".into(),
                    targets: vec![AttributeTarget::RecordField],
                    doc: "Omits a record field from encoded wire output.".into(),
                },
                AttributeSpec {
                    name: "default".into(),
                    targets: vec![AttributeTarget::RecordField],
                    doc: "Provides a fallback value when a record field is omitted.".into(),
                },
                AttributeSpec {
                    name: "alias".into(),
                    targets: vec![AttributeTarget::Variant],
                    doc: "Adds a short command alias for a CLI subcommand variant.".into(),
                },
            ],
        }
    }

    fn collect(&self, blocks: &[Block], _: &mut dyn Compiler) -> Vec<u8> {
        let count = u32::try_from(blocks.len()).unwrap_or(u32::MAX);
        count.to_le_bytes().to_vec()
    }

    fn expand(
        &self,
        block: &Block,
        _: &[u8],
        compiler: &mut dyn Compiler,
    ) -> Result<Arena, Vec<Diagnostic>> {
        let mut b = Builder::new(block.whole());
        match block.head.as_str() {
            "route" => {
                // `route fn <name> = <expr>`
                let mut c = block.cursor();
                c.eat("fn").ok_or_else(|| {
                    vec![
                        Diagnostic::error("expected `fn` after `route`", block.whole())
                            .with_code("M010"),
                    ]
                })?;
                let name = c.ident().ok_or_else(|| {
                    vec![Diagnostic::error("expected route name", block.whole()).with_code("M010")]
                })?;
                c.eat("=").ok_or_else(|| {
                    vec![Diagnostic::error("expected `=`", block.whole()).with_code("M010")]
                })?;
                let body = c.expr(compiler).map_err(|d| vec![d])?;
                let decl = flick!(b @ name.span, let #(block.text_of(name.span)) = #body);
                Ok(b.finish(decl))
            }
            "widget" => {
                // `widget <name> = <expr>`
                let mut c = block.cursor();
                let name = c.ident().ok_or_else(|| {
                    vec![Diagnostic::error("expected widget name", block.whole()).with_code("M011")]
                })?;
                c.eat("=").ok_or_else(|| {
                    vec![Diagnostic::error("expected `=`", block.whole()).with_code("M011")]
                })?;
                let body = c.expr(compiler).map_err(|d| vec![d])?;
                let decl = flick!(b @ name.span, let #(block.text_of(name.span)) = #body);
                Ok(b.finish(decl))
            }
            "ref_of" => {
                // `ref_of App.Target`
                let name = block.text.trim();
                if name.is_empty()
                    || !block.tokens.iter().all(|t| {
                        matches!(t.kind, TokenKind::Ident | TokenKind::Con | TokenKind::Punct)
                    })
                {
                    return Err(vec![
                        Diagnostic::error("expected a name after `ref_of`", block.whole())
                            .with_code("M012"),
                    ]);
                }
                let text = b.expr(Expr::Text(name.to_owned()));
                Ok(b.finish(text))
            }
            other => Err(vec![
                Diagnostic::error(format!("unknown head `{other}`"), block.whole())
                    .with_code("M099"),
            ]),
        }
    }

    fn check_calls(&self, calls: &[CallSite], _: &[u8]) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        for call in calls {
            if call.module == "Meta"
                && call.name == "guarded"
                && let Some(arg) = call.arguments.first()
                && matches!(arg.constant.as_ref(), Some(Constant::Text(s)) if s == "forbidden")
            {
                out.push(
                    Diagnostic::error("`Meta.guarded` refuses \"forbidden\"", arg.span)
                        .with_code("M003"),
                );
            }
        }
        out
    }

    fn derive(
        &self,
        target: &DeriveTarget,
        class: &str,
        args: Option<&Block>,
        module: &[DeriveTarget],
        _: &[u8],
        _: &mut dyn Compiler,
    ) -> Result<Arena, Vec<Diagnostic>> {
        match class {
            "Meta.Codec" => codec::derive(target, args, module),
            "Meta.CliForm" => cli_form::derive(target, args, module),
            _ => Err(vec![
                Diagnostic::error(
                    format!("`{class}` has no derive in `meta`"),
                    target.decl.whole(),
                )
                .with_code("M099"),
            ]),
        }
    }
}
