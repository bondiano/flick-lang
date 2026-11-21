//! Audited `Crypto` pure IR + metered digest/PBKDF2 intrinsic adapter (Stage 4D).

use flick_ir::memory::CryptoIntrinsic;
use flick_ir::{Arith, BinOp, Foreign, ForeignMode, Num};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::{Code, Env, Expr, Intrinsic, Local, Lower, LowerError, Span, Stmt, free};

impl Lower<'_> {
    pub(super) fn std_crypto_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 || value.mode != ForeignMode::Effect || !has_standard_shape(value) {
            return None;
        }
        if value.binding.from.as_deref()? != "flick:Crypto" {
            return None;
        }
        let bind = value.binding.name.as_str();
        let stages = value.stages.len();
        if bind == "flickDigest" && stages == 2 {
            return Some(self.intrinsic(
                Intrinsic::Crypto(CryptoIntrinsic::Digest),
                false,
                vec![],
                block,
                span,
            ));
        }
        let b = ExprBuilder::new(span, "$cr");
        let expr = crypto_template(&b, bind, stages)?;
        Some(self.lower_crypto_expr(&expr, block, span))
    }

    fn lower_crypto_expr(
        &mut self,
        expr: &Expr,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        let used = free(expr);
        let mut env = Env::new();
        if used.contains("$cr.pbkdf2") {
            let local = self.intrinsic(
                Intrinsic::Crypto(CryptoIntrinsic::Pbkdf2),
                false,
                vec![],
                block,
                span,
            )?;
            env.insert("$cr.pbkdf2".into(), local);
        }
        self.expr(expr, &env, block)
    }
}

fn rand_f(b: &ExprBuilder) -> Expr {
    b.app(b.global("Host", "raw_next"), b.unit())
}

fn rand_byte(b: &ExprBuilder) -> Expr {
    b.app(
        b.global("Int", "truncate"),
        b.app(
            b.global("Float", "floor"),
            b.bin(
                BinOp::Arith(Arith::Mul, Num::Float),
                rand_f(b),
                b.float(256.0),
            ),
        ),
    )
}

fn crypto_fail(b: &ExprBuilder, msg: Expr) -> Expr {
    b.app(
        b.global("Fail", "fail"),
        b.app(
            b.at(super::ExprKind::Con(ExprBuilder::con(
                "Crypto",
                "CryptoError",
                "CryptoError",
                1,
            ))),
            msg,
        ),
    )
}

fn hex_byte(b: &ExprBuilder, byte_expr: Expr) -> Expr {
    let digit = |v: Expr| {
        b.app3(
            b.global("String", "slice"),
            v.clone(),
            b.add_i(v, b.int(1)),
            b.str_("0123456789abcdef"),
        )
    };
    b.block(
        vec![
            b.let_(
                "hb",
                b.app2(b.global("Int", "bit_and"), byte_expr, b.int(255)),
            ),
            b.let_(
                "hi",
                b.bin(BinOp::Arith(Arith::Div, Num::Int), b.loc("hb"), b.int(16)),
            ),
            b.let_(
                "lo",
                b.bin(BinOp::Arith(Arith::Rem, Num::Int), b.loc("hb"), b.int(16)),
            ),
        ],
        b.bin(BinOp::Concat, digit(b.loc("hi")), digit(b.loc("lo"))),
    )
}

fn format_uuid_expr(b: &ExprBuilder, bytes: Vec<Expr>) -> Expr {
    let mut stmts = Vec::new();
    let mut parts = Vec::new();
    for (i, expr) in bytes.into_iter().enumerate() {
        let name = format!("b{i}");
        stmts.push(b.let_(&name, expr));
        if matches!(i, 4 | 6 | 8 | 10) {
            parts.push(b.str_("-"));
        }
        parts.push(hex_byte(b, b.loc(&name)));
    }
    b.block(
        stmts,
        b.app2(b.global("String", "join"), b.str_(""), b.list(parts)),
    )
}

fn clamped_millis_expr(b: &ExprBuilder) -> Expr {
    b.block(
        vec![b.let_(
            "t",
            b.app(
                b.global("Float", "trunc"),
                b.app(b.global("Clock", "current_time_millis"), b.unit()),
            ),
        )],
        b.if_(
            b.bin(BinOp::Lt, b.loc("t"), b.float(0.0)),
            b.float(0.0),
            b.if_(
                b.bin(BinOp::Gt, b.loc("t"), b.float(281_474_976_710_655.0)),
                b.float(281_474_976_710_655.0),
                b.loc("t"),
            ),
        ),
    )
}

fn pbkdf2_template(b: &ExprBuilder) -> Expr {
    let call = b.app(
        b.app4(
            b.loc("pbkdf2"),
            b.loc("alg"),
            b.loc("iters"),
            b.loc("len"),
            b.loc("salt"),
        ),
        b.loc("pwd"),
    );
    b.lam(
        &["alg", "iters", "len", "salt", "pwd"],
        b.match_(
            call,
            vec![
                arm(b.ok_pat("bs"), b.loc("bs")),
                arm(b.err_pat("msg"), crypto_fail(b, b.loc("msg"))),
            ],
        ),
    )
}

fn random_bytes_template(b: &ExprBuilder) -> Expr {
    let body = b.while_(
        b.bin(BinOp::Lt, b.var("i"), b.loc("n")),
        b.block(
            vec![b.assign("i", b.add_i(b.var("i"), b.int(1)))],
            b.yield_(rand_byte(b)),
        ),
    );
    b.lam(
        &["n"],
        b.if_(
            b.bin(BinOp::Lt, b.loc("n"), b.int(0)),
            crypto_fail(
                b,
                b.str_("Crypto.randomBytes: size must be a non-negative safe integer"),
            ),
            b.block(vec![b.var_decl("i", b.int(0))], b.gen_strict(body)),
        ),
    )
}

fn random_shuffle_template(b: &ExprBuilder) -> Expr {
    let step = b.block(
        vec![
            b.let_("len", b.app(b.global("List.Core", "length"), b.var("rem"))),
            b.let_(
                "idx",
                b.app(
                    b.global("Int", "truncate"),
                    b.app(
                        b.global("Float", "floor"),
                        b.bin(
                            BinOp::Arith(Arith::Mul, Num::Float),
                            rand_f(b),
                            b.app(b.global("Int", "to_float"), b.loc("len")),
                        ),
                    ),
                ),
            ),
            b.let_(
                "chosen",
                b.app2(
                    b.global("List.Core", "take"),
                    b.int(1),
                    b.app2(b.global("List.Core", "drop"), b.loc("idx"), b.var("rem")),
                ),
            ),
            b.assign(
                "out",
                b.app2(
                    b.global("List.Core", "append"),
                    b.var("out"),
                    b.loc("chosen"),
                ),
            ),
            b.assign(
                "rem",
                b.app2(
                    b.global("List.Core", "append"),
                    b.app2(b.global("List.Core", "take"), b.loc("idx"), b.var("rem")),
                    b.app2(
                        b.global("List.Core", "drop"),
                        b.add_i(b.loc("idx"), b.int(1)),
                        b.var("rem"),
                    ),
                ),
            ),
        ],
        b.unit(),
    );
    b.lam(
        &["xs"],
        b.block(
            vec![
                b.var_decl("rem", b.loc("xs")),
                b.var_decl("out", b.list(vec![])),
                Stmt::Expr(b.while_(
                    b.bin(
                        BinOp::Gt,
                        b.app(b.global("List.Core", "length"), b.var("rem")),
                        b.int(0),
                    ),
                    step,
                )),
            ],
            b.var("out"),
        ),
    )
}

fn uuid_v4_template(b: &ExprBuilder) -> Expr {
    let mut bytes = Vec::with_capacity(16);
    for i in 0..16 {
        let raw = rand_byte(b);
        let adjusted = match i {
            6 => b.app2(
                b.global("Int", "bit_or"),
                b.app2(b.global("Int", "bit_and"), raw, b.int(0x0f)),
                b.int(0x40),
            ),
            8 => b.app2(
                b.global("Int", "bit_or"),
                b.app2(b.global("Int", "bit_and"), raw, b.int(0x3f)),
                b.int(0x80),
            ),
            _ => raw,
        };
        bytes.push(adjusted);
    }
    b.lam(&["_u"], format_uuid_expr(b, bytes))
}

fn uuid_v7_template(b: &ExprBuilder) -> Expr {
    let time_byte = |div: f64| {
        b.app(
            b.global("Int", "truncate"),
            b.bin(
                BinOp::Arith(Arith::Rem, Num::Float),
                b.app(
                    b.global("Float", "floor"),
                    b.bin(
                        BinOp::Arith(Arith::Div, Num::Float),
                        b.loc("ms"),
                        b.float(div),
                    ),
                ),
                b.float(256.0),
            ),
        )
    };
    let divs = [
        1_099_511_627_776.0,
        4_294_967_296.0,
        16_777_216.0,
        65_536.0,
        256.0,
        1.0,
    ];
    let mut bytes: Vec<Expr> = divs.into_iter().map(time_byte).collect();
    for i in 6..16 {
        let raw = rand_byte(b);
        let adjusted = match i {
            6 => b.app2(
                b.global("Int", "bit_or"),
                b.app2(b.global("Int", "bit_and"), raw, b.int(0x0f)),
                b.int(0x70),
            ),
            8 => b.app2(
                b.global("Int", "bit_or"),
                b.app2(b.global("Int", "bit_and"), raw, b.int(0x3f)),
                b.int(0x80),
            ),
            _ => raw,
        };
        bytes.push(adjusted);
    }
    b.lam(
        &["_u"],
        b.block(
            vec![b.let_("ms", clamped_millis_expr(b))],
            format_uuid_expr(b, bytes),
        ),
    )
}

const ULID_ALPHA: &str = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

fn ulid_char_at(b: &ExprBuilder, idx: Expr) -> Expr {
    b.app3(
        b.global("String", "slice"),
        idx.clone(),
        b.add_i(idx, b.int(1)),
        b.str_(ULID_ALPHA),
    )
}

fn ulid_time_loop(b: &ExprBuilder) -> Expr {
    b.while_(
        b.bin(BinOp::Lt, b.var("ti"), b.int(10)),
        b.block(
            vec![
                b.let_(
                    "d",
                    b.app(
                        b.global("Int", "truncate"),
                        b.bin(
                            BinOp::Arith(Arith::Rem, Num::Float),
                            b.var("cur"),
                            b.float(32.0),
                        ),
                    ),
                ),
                b.assign(
                    "tchars",
                    b.app2(
                        b.global("List.Core", "append"),
                        b.list(vec![ulid_char_at(b, b.loc("d"))]),
                        b.var("tchars"),
                    ),
                ),
                b.assign(
                    "cur",
                    b.app(
                        b.global("Float", "floor"),
                        b.bin(
                            BinOp::Arith(Arith::Div, Num::Float),
                            b.var("cur"),
                            b.float(32.0),
                        ),
                    ),
                ),
                b.assign("ti", b.add_i(b.var("ti"), b.int(1))),
            ],
            b.unit(),
        ),
    )
}

fn ulid_rand_loop(b: &ExprBuilder) -> Expr {
    b.while_(
        b.bin(BinOp::Lt, b.var("ri"), b.int(16)),
        b.block(
            vec![
                b.let_(
                    "d",
                    b.app(
                        b.global("Int", "truncate"),
                        b.app(
                            b.global("Float", "floor"),
                            b.bin(
                                BinOp::Arith(Arith::Mul, Num::Float),
                                rand_f(b),
                                b.float(32.0),
                            ),
                        ),
                    ),
                ),
                b.assign(
                    "rchars",
                    b.app2(
                        b.global("List.Core", "append"),
                        b.var("rchars"),
                        b.list(vec![ulid_char_at(b, b.loc("d"))]),
                    ),
                ),
                b.assign("ri", b.add_i(b.var("ri"), b.int(1))),
            ],
            b.unit(),
        ),
    )
}

fn ulid_template(b: &ExprBuilder) -> Expr {
    b.lam(
        &["_u"],
        b.block(
            vec![
                b.var_decl("cur", clamped_millis_expr(b)),
                b.var_decl("ti", b.int(0)),
                b.var_decl("tchars", b.list(vec![])),
                Stmt::Expr(ulid_time_loop(b)),
                b.var_decl("ri", b.int(0)),
                b.var_decl("rchars", b.list(vec![])),
                Stmt::Expr(ulid_rand_loop(b)),
            ],
            b.app2(
                b.global("String", "join"),
                b.str_(""),
                b.app2(
                    b.global("List.Core", "append"),
                    b.var("tchars"),
                    b.var("rchars"),
                ),
            ),
        ),
    )
}

fn crypto_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flickPbkdf2", 5) => Some(pbkdf2_template(b)),
        ("flickRandomBytes", 1) => Some(random_bytes_template(b)),
        ("flickRandom", 1) => Some(b.lam(&["_u"], rand_f(b))),
        ("flickRandomBoolean", 1) => {
            Some(b.lam(&["_u"], b.bin(BinOp::Gt, rand_f(b), b.float(0.5))))
        }
        ("flickRandomInt", 1) => Some(b.lam(
            &["_u"],
            b.app(
                b.global("Int", "truncate"),
                b.bin(
                    BinOp::Arith(Arith::Sub, Num::Float),
                    b.app(
                        b.global("Float", "floor"),
                        b.bin(
                            BinOp::Arith(Arith::Mul, Num::Float),
                            rand_f(b),
                            b.float(4_294_967_296.0),
                        ),
                    ),
                    b.float(2_147_483_648.0),
                ),
            ),
        )),
        ("flickRandomBetween", 2) => Some(b.lam(
            &["lo", "hi"],
            b.bin(
                BinOp::Arith(Arith::Add, Num::Float),
                b.loc("lo"),
                b.bin(
                    BinOp::Arith(Arith::Mul, Num::Float),
                    rand_f(b),
                    b.bin(
                        BinOp::Arith(Arith::Sub, Num::Float),
                        b.loc("hi"),
                        b.loc("lo"),
                    ),
                ),
            ),
        )),
        ("flickRandomIntBetween", 2) => Some(b.lam(
            &["lo", "hi"],
            b.block(
                vec![
                    b.let_("lf", b.app(b.global("Int", "to_float"), b.loc("lo"))),
                    b.let_("hf", b.app(b.global("Int", "to_float"), b.loc("hi"))),
                    b.let_(
                        "span",
                        b.bin(
                            BinOp::Arith(Arith::Add, Num::Float),
                            b.bin(
                                BinOp::Arith(Arith::Sub, Num::Float),
                                b.loc("hf"),
                                b.loc("lf"),
                            ),
                            b.float(1.0),
                        ),
                    ),
                ],
                b.app(
                    b.global("Int", "truncate"),
                    b.bin(
                        BinOp::Arith(Arith::Add, Num::Float),
                        b.app(
                            b.global("Float", "floor"),
                            b.bin(
                                BinOp::Arith(Arith::Mul, Num::Float),
                                rand_f(b),
                                b.loc("span"),
                            ),
                        ),
                        b.loc("lf"),
                    ),
                ),
            ),
        )),
        ("flickRandomShuffle", 1) => Some(random_shuffle_template(b)),
        ("flickRandomUuidV4", 1) => Some(uuid_v4_template(b)),
        ("flickRandomUuidV7", 1) => Some(uuid_v7_template(b)),
        ("flickRandomUlid", 1) => Some(ulid_template(b)),
        _ => None,
    }
}
