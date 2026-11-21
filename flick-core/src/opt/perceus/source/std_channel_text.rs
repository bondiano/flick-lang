//! Audited `Channel.Transform` buffer/text + `ChannelSchema` pure IR adapters (Stage 4E).

mod buffer;
mod schema;
mod utf8;

use super::expr_builder::ExprBuilder;
use super::std_schema_rt::rt;
use super::{Expr, Span};

pub(super) fn channel_text_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flickBuffer", 2) => Some(b.app(rt(b, "Helper.ch_buffer"), b.bool_(false))),
        ("flickBufferArray", 2) => Some(b.app(rt(b, "Helper.ch_buffer"), b.bool_(true))),
        ("splitLines", 1) => Some(b.lam(&["_u"], rt(b, "Helper.ch_split_lines"))),
        ("flickDecodeText", 1) => Some(b.lam(&["_enc"], rt(b, "Helper.ch_decode_text"))),
        ("flickEncodeText", 1) => Some(b.lam(&["_u"], rt(b, "Helper.ch_encode_text"))),
        _ => None,
    }
}

pub(super) fn channel_schema_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flickDecode", 1) => Some(b.app(rt(b, "Helper.ch_schema_codec"), b.bool_(true))),
        ("flickEncode", 1) => Some(b.app(rt(b, "Helper.ch_schema_codec"), b.bool_(false))),
        ("flickDuplex", 2) => {
            let dec = b.app2(
                rt(b, "Helper.ch_schema_codec"),
                b.bool_(true),
                b.field(b.loc("opts"), "outputSchema"),
            );
            let enc = b.app2(
                rt(b, "Helper.ch_schema_codec"),
                b.bool_(false),
                b.field(b.loc("opts"), "inputSchema"),
            );
            let inner = b.app2(rt(b, "Helper.ch_pipe_to"), b.loc("self"), enc);
            Some(b.lam(
                &["opts", "self"],
                b.app2(rt(b, "Helper.ch_pipe_to"), dec, inner),
            ))
        }
        _ => None,
    }
}

pub(super) fn channel_text_helper(name: &str, span: Span) -> Option<Expr> {
    let b = ExprBuilder::new(span, "$chx");
    match name {
        "ch_buffer" => Some(buffer::ch_buffer_helper(&b)),
        "ch_split_lines" => Some(buffer::ch_split_lines_helper(&b)),
        "ch_utf8_stream_step" => Some(utf8::ch_utf8_stream_step_helper(&b)),
        "ch_decode_text" => Some(utf8::ch_decode_text_helper(&b)),
        "ch_encode_text" => Some(utf8::ch_encode_text_helper(&b)),
        "ch_schema_unreport" => Some(schema::ch_schema_unreport_helper(&b)),
        "ch_schema_codec" => Some(schema::ch_schema_codec_helper(&b)),
        _ => None,
    }
}
