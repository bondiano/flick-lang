//! Synthetic `$rt.Helper.*` canonical FLN codec helpers (`Codec.fln`).

mod adt;
mod decode;
mod encode;

use flick_ir::Constructor;

use self::adt::{decode_fln_adt, decode_fln_enum, decode_fln_enum_tags};
use self::decode::{
    decode_fln_list, decode_fln_option, decode_fln_record, decode_fln_result, decode_fln_tuple,
};
pub(super) use self::encode::encode_fln_helper;
use super::expr_builder::{ExprBuilder, arm};
use super::std_schema_rt::{cat, con_app, rt, schema_err, str_lit};
use super::{Expr, Pat};

fn fln_con(b: &ExprBuilder, ctor: &str, args: Vec<Expr>) -> Expr {
    con_app(b, "Fln", "Value", ctor, args)
}

fn fln_pat(b: &ExprBuilder, ctor: &str, vars: &[&str]) -> Pat {
    let pats = vars
        .iter()
        .map(|v| {
            if *v == "_" {
                Pat::Wild
            } else {
                Pat::Bind(b.name(v))
            }
        })
        .collect();
    Pat::Con(ExprBuilder::con("Fln", "Value", ctor, vars.len()), pats)
}

fn kc(c: &str, n: usize) -> Constructor {
    ExprBuilder::con("Schema", "$Kind", c, n)
}

#[expect(
    clippy::too_many_lines,
    reason = "closed Schema FLN decoder AST template"
)]
pub(super) fn decode_fln_helper(b: &ExprBuilder) -> Expr {
    let leaf = |k: &str| b.app2(rt(b, "Schema.DecodeLeaf"), str_lit(b, k), b.loc("input"));
    b.lam(
        &["sch", "input"],
        b.match_(
            b.app(rt(b, "Helper.schema_kind"), b.loc("sch")),
            vec![
                arm(Pat::Con(kc("$Unit", 0), vec![]), leaf("fln_unit")),
                arm(Pat::Con(kc("$Bool", 0), vec![]), leaf("fln_bool")),
                arm(Pat::Con(kc("$String", 0), vec![]), leaf("fln_string")),
                arm(Pat::Con(kc("$Int", 0), vec![]), leaf("fln_int")),
                arm(Pat::Con(kc("$Int8", 0), vec![]), leaf("fln_int8")),
                arm(Pat::Con(kc("$Int16", 0), vec![]), leaf("fln_int16")),
                arm(Pat::Con(kc("$Int64", 0), vec![]), leaf("fln_int64")),
                arm(Pat::Con(kc("$UInt8", 0), vec![]), leaf("fln_uint8")),
                arm(Pat::Con(kc("$UInt16", 0), vec![]), leaf("fln_uint16")),
                arm(Pat::Con(kc("$UInt32", 0), vec![]), leaf("fln_uint32")),
                arm(Pat::Con(kc("$UInt64", 0), vec![]), leaf("fln_uint64")),
                arm(Pat::Con(kc("$BigInt", 0), vec![]), leaf("fln_bigint")),
                arm(Pat::Con(kc("$Number", 0), vec![]), leaf("fln_number")),
                arm(Pat::Con(kc("$Float32", 0), vec![]), leaf("fln_float32")),
                arm(
                    Pat::Con(kc("$Never", 0), vec![]),
                    b.err(schema_err(b, str_lit(b, "expected nothing (Never)"))),
                ),
                arm(Pat::Con(kc("$Unknown", 0), vec![]), b.ok(b.loc("input"))),
                arm(
                    Pat::Con(kc("$Redacted", 1), vec![Pat::Wild]),
                    b.err(schema_err(
                        b,
                        str_lit(
                            b,
                            "a Redacted value has no FLN form without an explicit policy",
                        ),
                    )),
                ),
                arm(
                    Pat::Con(kc("$Lazy", 2), vec![Pat::Wild, Pat::Bind(b.name("th"))]),
                    b.app2(
                        rt(b, "Helper.decode_fln"),
                        b.app(b.loc("th"), b.unit()),
                        b.loc("input"),
                    ),
                ),
                arm(
                    Pat::Con(
                        kc("$Refined", 3),
                        vec![
                            Pat::Bind(b.name("nm")),
                            Pat::Bind(b.name("bs")),
                            Pat::Bind(b.name("chk")),
                        ],
                    ),
                    b.match_(
                        b.app2(rt(b, "Helper.decode_fln"), b.loc("bs"), b.loc("input")),
                        vec![
                            arm(
                                b.ok_pat("v"),
                                b.if_(
                                    b.app(b.loc("chk"), b.loc("v")),
                                    b.ok(b.loc("v")),
                                    b.err(schema_err(
                                        b,
                                        cat(b, str_lit(b, "not a valid "), b.loc("nm")),
                                    )),
                                ),
                            ),
                            arm(b.err_pat("e"), b.err(b.loc("e"))),
                        ],
                    ),
                ),
                arm(
                    Pat::Con(kc("$List", 1), vec![Pat::Bind(b.name("it"))]),
                    decode_fln_list(b),
                ),
                arm(
                    Pat::Con(kc("$Tuple", 1), vec![Pat::Bind(b.name("its"))]),
                    decode_fln_tuple(b),
                ),
                arm(
                    Pat::Con(kc("$Option", 1), vec![Pat::Bind(b.name("it"))]),
                    decode_fln_option(b),
                ),
                arm(
                    Pat::Con(
                        kc("$Result", 2),
                        vec![Pat::Bind(b.name("es")), Pat::Bind(b.name("os"))],
                    ),
                    decode_fln_result(b),
                ),
                arm(
                    Pat::Con(kc("$Record", 1), vec![Pat::Bind(b.name("flds"))]),
                    decode_fln_record(b),
                ),
                arm(
                    Pat::Con(kc("$Adt", 2), vec![Pat::Wild, Pat::Bind(b.name("cases"))]),
                    decode_fln_adt(b),
                ),
                arm(
                    Pat::Con(kc("$Enum", 1), vec![Pat::Bind(b.name("vals"))]),
                    decode_fln_enum(b),
                ),
                arm(
                    Pat::Con(kc("$EnumTags", 1), vec![Pat::Bind(b.name("pairs"))]),
                    decode_fln_enum_tags(b),
                ),
                arm(
                    Pat::Wild,
                    b.err(schema_err(
                        b,
                        str_lit(b, "schema has no canonical FLN form"),
                    )),
                ),
            ],
        ),
    )
}
