//! Direct Go package function bindings (`foreign hypot from "math" "Hypot"`)
//! with automatic Go imports and generated `flick.go-ffi@0.1.0` conversions.

use flick_ir::{Foreign, ForeignMode, Span};
use flick_pretty::{Document, docvec};

use super::foreign::parameters;
use super::foreign_pkg_conv::{GoAbiType, comma_ok_to_flick, flick_to_go, go_to_flick};
pub(super) use super::foreign_pkg_spec::{
    decompose_sig, direct_go_types, is_reserved_go_import_ident, parse_go_package,
};
use super::go_type::{GoType, OptPayload, OptRepr, TypeDecls};
use super::layout::{block, call, closure, signature, single, statement, typed_signature};
use super::{Gen, names};

enum ReturnShape<'t> {
    /// Go function returns nothing (or bare `error` with `catch`).
    Void,
    /// Go function returns 1 value `r0` (or `(r0, error)` with `catch`).
    Single(&'t GoAbiType),
    /// Go function returns 1 nullable opaque handle `r0` (or `(r0, error)` with `catch`).
    NullableOpaque,
    /// Go function returns `(r0, bool)` (or `(r0, bool, error)` with `catch`).
    CommaOk(&'t GoAbiType),
}

impl<'a> Gen<'a> {
    /// Emits a direct Go package binding (`foreign f from "pkg" "Sym"`).
    pub(super) fn package_foreign(
        &mut self,
        f: &Foreign,
        from: &str,
        export: &str,
        span: Span,
    ) -> Option<Document<'a>> {
        let Some(parsed) = parse_go_package(from, export) else {
            self.errors.push((
                format!(
                    "`{}.{}` (foreign `{export}` from `{from}`) is not a valid Go package import and exported symbol",
                    self.module, f.name
                ),
                Some(span),
            ));
            return None;
        };
        self.emit_direct_go(
            f,
            parsed.import_path,
            &parsed.default_pkg,
            parsed.force_alias,
            parsed.export,
            span,
        )
    }

    /// Emits a direct Go call with automatic import registration and `flick.go-ffi`
    /// parameter/result conversions (shared by package bindings and Go globals).
    pub(super) fn emit_direct_go(
        &mut self,
        f: &Foreign,
        import_path: &str,
        default_pkg: &str,
        force_alias: bool,
        export: &str,
        span: Span,
    ) -> Option<Document<'a>> {
        let a = self.a;
        let empty = TypeDecls::new();
        let decls = self.decls.unwrap_or(&empty);
        let taken = parameters(f);

        let pkg = self.add_go_import_with_alias(import_path, default_pkg, force_alias);
        let target = a.alloc_str(&format!("{pkg}.{export}"));

        // Zero-parameter constant (`foreign pure pi from "math" "Pi"`).
        if f.dicts == 0 && taken == 0 {
            return Some(self.emit_direct_constant(f, decls, target));
        }
        let (param_abis, result_abi) =
            self.classify_direct_sig(f, decls, import_path, export, taken, span)?;
        let return_shape = self.classify_return_shape(f, &result_abi, span)?;

        let types = direct_go_types(self.input.lang, decls, f, taken);
        let heads: Vec<String> = (0..f.dicts)
            .map(|i| format!("d{i}"))
            .chain((0..taken).map(|i| format!("a{i}")))
            .collect();
        let head_refs: Vec<&str> = heads.iter().map(String::as_str).collect();
        let params_sig = if types.any() {
            typed_signature(&head_refs, f.dicts, &types)
        } else {
            signature(&head_refs)
        };
        let result_go_type = types.result();
        let result_sig = result_go_type.map_or_else(|| "flickrt.Value".to_owned(), GoType::go_type);

        let converted_args: Vec<Document<'a>> = param_abis
            .iter()
            .enumerate()
            .map(|(i, abi)| {
                let param_name = a.text(a.alloc_str(&heads[f.dicts + i]));
                flick_to_go(a, abi, types.param(i), param_name)
            })
            .collect();

        let body = self.direct_go_body(f, target, &converted_args, &return_shape, result_go_type);
        let name = names::func(self.module, &f.name);
        Some(docvec![
            a,
            "func ",
            name,
            "(",
            params_sig,
            ") ",
            a.alloc_str(&result_sig),
            " ",
            body
        ])
    }

    fn emit_direct_constant(
        &self,
        f: &Foreign,
        decls: &TypeDecls,
        target: &'a str,
    ) -> Document<'a> {
        let a = self.a;
        let var = names::var(self.module, &f.name);
        let rhs = match f
            .ty
            .as_deref()
            .and_then(|ty| GoAbiType::classify(self.input.lang, decls, ty, 0))
        {
            Some(GoAbiType::Int) => docvec![a, "int32(", target, ")"],
            Some(GoAbiType::Float) => docvec![a, "float64(", target, ")"],
            Some(GoAbiType::Bool) => docvec![a, "bool(", target, ")"],
            Some(GoAbiType::String) => docvec![a, "string(", target, ")"],
            Some(abi) => go_to_flick(a, &abi, None, a.text(target)),
            None => a.text(target),
        };
        docvec![a, "var ", var, " flickrt.Value = ", rhs]
    }

    fn classify_direct_sig(
        &mut self,
        f: &Foreign,
        decls: &TypeDecls,
        import_path: &str,
        export: &str,
        taken: usize,
        span: Span,
    ) -> Option<(Vec<GoAbiType>, GoAbiType)> {
        let Some(ty) = f.ty.as_deref() else {
            self.errors.push((
                format!(
                    "`{}.{}` needs IR type information for direct Go binding `{import_path}.{export}`",
                    self.module, f.name
                ),
                Some(span),
            ));
            return None;
        };
        let Some((param_tys, result_ty)) = decompose_sig(ty, taken) else {
            self.errors.push((
                format!(
                    "`{}.{}` signature arity does not match direct Go binding `{import_path}.{export}`",
                    self.module, f.name
                ),
                Some(span),
            ));
            return None;
        };
        let arg_count = if f.arity == 0 { 0 } else { taken };
        let mut param_abis = Vec::with_capacity(arg_count);
        for &p in param_tys.iter().take(arg_count) {
            let Some(abi) = GoAbiType::classify(self.input.lang, decls, p, 0) else {
                self.errors.push((
                    format!(
                        "`{}.{}` has a parameter type outside `flick.go-ffi@0.1.0`; use a sibling `.go` file or `from inline` with `flickrt.Value`",
                        self.module, f.name
                    ),
                    Some(span),
                ));
                return None;
            };
            param_abis.push(abi);
        }
        let Some(result_abi) = GoAbiType::classify(self.input.lang, decls, result_ty, 0) else {
            self.errors.push((
                format!(
                    "`{}.{}` has a result type outside `flick.go-ffi@0.1.0`; use a sibling `.go` file or `from inline` with `flickrt.Value`",
                    self.module, f.name
                ),
                Some(span),
            ));
            return None;
        };
        Some((param_abis, result_abi))
    }

    fn classify_return_shape<'t>(
        &mut self,
        f: &Foreign,
        result_abi: &'t GoAbiType,
        span: Span,
    ) -> Option<ReturnShape<'t>> {
        match result_abi {
            GoAbiType::Unit => Some(ReturnShape::Void),
            GoAbiType::OptionPtr(inner) => Some(if f.adapter.result.nullable {
                ReturnShape::Single(result_abi)
            } else {
                ReturnShape::CommaOk(inner)
            }),
            GoAbiType::OptionFlag(inner) => {
                if !f.adapter.result.nullable {
                    return Some(ReturnShape::CommaOk(inner));
                }
                if matches!(inner.as_ref(), GoAbiType::Opaque) {
                    Some(ReturnShape::NullableOpaque)
                } else {
                    self.errors.push((
                        format!(
                            "`{}.{}` cannot use `nullable` on nested `Option`: `nil` would collide with `Some None`; omit `nullable` to use Go's `(T, bool)` presence flag",
                            self.module, f.name
                        ),
                        Some(span),
                    ));
                    None
                }
            }
            _ => Some(ReturnShape::Single(result_abi)),
        }
    }

    fn direct_go_body(
        &mut self,
        f: &Foreign,
        target: &'a str,
        args: &[Document<'a>],
        return_shape: &ReturnShape<'_>,
        want_result: Option<&GoType>,
    ) -> Document<'a> {
        let catch_parts = f.catch.as_ref().map(|catch_expr| {
            let saved_ctx = self.context.replace("fb");
            let saved_fb = self.fiber.replace("fb");
            let (stmts, code) = self.block(|g| g.expr(catch_expr));
            self.context = saved_ctx;
            self.fiber = saved_fb;
            (stmts, code.doc)
        });
        let variadic = f.adapter.has_word("variadic");
        if f.mode == ForeignMode::Async {
            self.direct_go_async_body(target, args, variadic, catch_parts, return_shape)
        } else {
            self.direct_go_sync_body(
                target,
                args,
                variadic,
                catch_parts,
                return_shape,
                want_result,
            )
        }
    }

    fn direct_go_sync_body(
        &self,
        target: &'a str,
        args: &[Document<'a>],
        variadic: bool,
        catch_parts: Option<(Vec<Document<'a>>, Document<'a>)>,
        return_shape: &ReturnShape<'_>,
        want_result: Option<&GoType>,
    ) -> Document<'a> {
        let a = self.a;
        let sync_args: Vec<Document<'a>> = args
            .iter()
            .enumerate()
            .map(|(i, &arg)| {
                if variadic && i + 1 == args.len() {
                    docvec![a, arg, "..."]
                } else {
                    arg
                }
            })
            .collect();
        let go_call = call(a, target, sync_args);
        if let Some((mut stmts, catch_doc)) = catch_parts {
            let inner = emit_catch_body(a, go_call, return_shape);
            let run = closure(a, "func() (flickrt.Value, error)", inner);
            let wrapped = call(a, "flickrt.CatchForeign", [a.text("fb"), run, catch_doc]);
            stmts.push(statement(a, docvec![a, "return ", wrapped]));
            return block(a, stmts);
        }
        match return_shape {
            ReturnShape::Void => block(
                a,
                [statement(a, go_call), statement(a, "return flickrt.Unit")],
            ),
            ReturnShape::Single(abi) => {
                let converted = go_to_flick(a, abi, want_result, go_call);
                single(a, docvec![a, "return ", converted])
            }
            ReturnShape::NullableOpaque => {
                let converted = match want_result {
                    Some(GoType::Opt(OptRepr::RefSentinel)) => {
                        call(a, "flickrt.NullableToOptRef", [go_call])
                    }
                    Some(GoType::Opt(OptRepr::Flag(OptPayload::Value))) => {
                        call(a, "flickrt.NullableToOptValue", [go_call])
                    }
                    _ => call(a, "flickrt.FromNullableGo", [go_call]),
                };
                single(a, docvec![a, "return ", converted])
            }
            ReturnShape::CommaOk(inner) => {
                let converted =
                    comma_ok_to_flick(a, inner, want_result, a.text("r0"), a.text("r1"));
                block(
                    a,
                    [
                        statement(a, docvec![a, "r0, r1 := ", go_call]),
                        statement(a, docvec![a, "return ", converted]),
                    ],
                )
            }
        }
    }

    fn direct_go_async_body(
        &self,
        target: &'a str,
        args: &[Document<'a>],
        variadic: bool,
        catch_parts: Option<(Vec<Document<'a>>, Document<'a>)>,
        return_shape: &ReturnShape<'_>,
    ) -> Document<'a> {
        let a = self.a;
        let mut stmts = Vec::new();
        let mut async_args = Vec::with_capacity(args.len() + 1);
        async_args.push(a.text("ctx"));
        for (i, &arg) in args.iter().enumerate() {
            let local = a.alloc_str(&format!("p{i}"));
            stmts.push(statement(a, docvec![a, local, " := ", arg]));
            if variadic && i + 1 == args.len() {
                async_args.push(docvec![a, local, "..."]);
            } else {
                async_args.push(a.text(local));
            }
        }
        let go_call = call(a, target, async_args);
        if let Some((catch_stmts, catch_doc)) = catch_parts {
            stmts.extend(catch_stmts);
            let inner = emit_catch_body(a, go_call, return_shape);
            let run = closure(a, "func(ctx flickrt.Context) (flickrt.Value, error)", inner);
            let wrapped = call(
                a,
                "flickrt.ForeignAsyncCatch",
                [a.text("fb"), run, catch_doc],
            );
            stmts.push(statement(a, docvec![a, "return ", wrapped]));
            return block(a, stmts);
        }
        let inner = match return_shape {
            ReturnShape::Void => vec![go_call, a.text("return flickrt.Unit")],
            ReturnShape::Single(abi) => {
                let converted = go_to_flick(a, abi, None, go_call);
                vec![docvec![a, "return ", converted]]
            }
            ReturnShape::NullableOpaque => {
                vec![docvec![
                    a,
                    "return ",
                    call(a, "flickrt.FromNullableGo", [go_call])
                ]]
            }
            ReturnShape::CommaOk(inner) => {
                let converted = comma_ok_to_flick(a, inner, None, a.text("r0"), a.text("r1"));
                vec![
                    docvec![a, "r0, r1 := ", go_call],
                    docvec![a, "return ", converted],
                ]
            }
        };
        let run = closure(a, "func(ctx flickrt.Context) flickrt.Value", inner);
        let wrapped = call(a, "flickrt.ForeignAsync", [a.text("fb"), run]);
        stmts.push(statement(a, docvec![a, "return ", wrapped]));
        block(a, stmts)
    }
}

fn emit_catch_body<'a>(
    a: &'a flick_pretty::DocumentArena<'a>,
    go_call: Document<'a>,
    return_shape: &ReturnShape<'_>,
) -> Vec<Document<'a>> {
    match return_shape {
        ReturnShape::Void => vec![
            docvec![a, "err := ", go_call],
            a.text("return flickrt.Unit, err"),
        ],
        ReturnShape::Single(abi) => {
            let converted = go_to_flick(a, abi, None, a.text("r0"));
            vec![
                docvec![a, "r0, err := ", go_call],
                a.text("if err != nil { return nil, err }"),
                docvec![a, "return ", converted, ", nil"],
            ]
        }
        ReturnShape::NullableOpaque => vec![
            docvec![a, "r0, err := ", go_call],
            a.text("if err != nil { return nil, err }"),
            docvec![
                a,
                "return ",
                call(a, "flickrt.FromNullableGo", [a.text("r0")]),
                ", nil"
            ],
        ],
        ReturnShape::CommaOk(inner) => {
            let converted = comma_ok_to_flick(a, inner, None, a.text("r0"), a.text("r1"));
            vec![
                docvec![a, "r0, r1, err := ", go_call],
                a.text("if err != nil { return nil, err }"),
                docvec![a, "return ", converted, ", nil"],
            ]
        }
    }
}
