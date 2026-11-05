//! `server fn name x = body` below its signature `name : P ->{r; e} S`: a
//! function its callers reach over the network; and `server name`, its
//! implementation. The code is `name$impl`, the body under the signature;
//! `name$procedure`, how its calls go, with the witnesses of what crosses;
//! `name`, the call any code makes; and `name$server`, what
//! `Server.handler` serves. Several parameters travel as one tuple.

use flick_plugin::{
    Arena, Block, Builder, Compiler, Decl, Diagnostic, Expr, NodeId, Pat, Place, Signature, Span,
    Token, TokenKind, Ty as Type,
};

use crate::module::{Form, Local, Module};
use crate::schema::{Gen, call, name};
use crate::ty::{self, Ty};

/// The error of a call a guard turns away, as a signature names it.
const UNAUTHORIZED: [&str; 2] = ["Server.Unauthorized", "Unauthorized"];

/// The code of the block `server …`.
pub fn expand(block: &Block, compiler: &mut dyn Compiler) -> Result<Arena, Vec<Diagnostic>> {
    let mut b = Builder::new(block.whole());
    let root = match block.place {
        Place::Expression => implementation(block, &mut b)?,
        Place::Declaration => declaration(block, compiler, &mut b)?,
    };
    Ok(b.finish(root))
}

fn refuse(block: &Block, message: String) -> Vec<Diagnostic> {
    vec![Diagnostic::error(message, block.whole()).with_code("E010")]
}

/// `server App.Api.place`: `App.Api.place$server`.
fn implementation(block: &Block, b: &mut Builder) -> Result<NodeId, Vec<Diagnostic>> {
    let tokens = &block.tokens;
    let path = tokens.iter().all(|t| {
        matches!(t.kind, TokenKind::Ident | TokenKind::Con) || block.text_of(t.span) == "."
    });
    let last = tokens.last().filter(|t| t.kind == TokenKind::Ident);
    if !path || last.is_none() {
        let message = "`server` names a server fn: `server name` or `server M.name`";
        return Err(refuse(block, message.into()));
    }
    let named: String = tokens.iter().map(|t| block.text_of(t.span)).collect();
    Ok(name(b, &format!("{named}$server")))
}

/// What a server function's signature says crosses the network, each part
/// at its span of the signature.
struct Parts {
    whole: Span,
    params: Vec<(Span, Ty)>,
    errors: Vec<(Span, Ty)>,
    success: (Span, Ty),
    guarded: bool,
}

fn declaration(
    block: &Block,
    compiler: &mut dyn Compiler,
    b: &mut Builder,
) -> Result<NodeId, Vec<Diagnostic>> {
    let fail = |message: String| refuse(block, message);
    let mut c = block.cursor();
    c.eat("fn")
        .ok_or_else(|| fail("`server` declares a function: `server fn name x = …`".into()))?;
    let me = (c.ident())
        .map(|t| block.text_of(t.span).to_owned())
        .ok_or_else(|| fail("expected the name of the server fn".into()))?;
    let mut params = Vec::new();
    while !c.at("=") && !c.is_done() {
        params.push(c.pat(compiler).map_err(|d| vec![d])?);
    }
    c.eat("=")
        .ok_or_else(|| fail(format!("expected `=` after the parameters of `{me}`")))?;
    let body = c.block(compiler).map_err(|d| vec![d])?;
    if !c.is_done() {
        return Err(fail(format!("expected the end of `{me}` after its body")));
    }
    let signature = (block.signature.as_ref())
        .filter(|s| signs(s, &me))
        .ok_or_else(|| {
            fail(format!(
                "server fn `{me}` needs a signature right above it: what the server runs"
            ))
        })?;
    let parts = parts(signature).map_err(|why| fail(format!("server fn `{me}`: {why}")))?;
    let arity = parts.params.len();
    if params.len() != arity {
        // its implementation takes the parameters as one payload
        return Err(fail(format!(
            "server fn `{me}` names each of its {arity} parameters"
        )));
    }
    let witnesses = witnesses(b, &parts, &me).map_err(fail)?;
    let implementation = format!("{me}$impl");
    let procedure = format!("{me}$procedure");
    let ty = b.ty(Type::Signed(parts.whole));
    let signed = b.decl(Decl::Signature {
        name: implementation.clone(),
        ty,
    });
    let patterns = params.iter().map(|&s| b.pat(Pat::Hole(s))).collect();
    let body = b.expr(Expr::Hole(body));
    let run_body = b.decl(Decl::Let {
        name: implementation.clone(),
        params: patterns,
        ty: None,
        body,
        rec: false,
    });
    let module = b.expr(Expr::Text(format!("{}.{me}", block.module)));
    let mut arguments = vec![module];
    arguments.extend(witnesses);
    let made = call(b, "Server.Derive.procedure", arguments);
    let typed = procedure_type(b, &parts);
    let typed = b.decl(Decl::Signature {
        name: procedure.clone(),
        ty: typed,
    });
    let made = let_(b, &procedure, made);
    let calls = calls(b, &procedure, arity);
    let calls = let_(b, &me, calls);
    let calls = b.doc(&block.docs, calls);
    let serve = if parts.guarded {
        "Server.Derive.guarded"
    } else {
        "Server.Derive.function"
    };
    let (p, run) = (name(b, &procedure), runs(b, &implementation, arity));
    let served = call(b, serve, vec![p, run]);
    let served = let_(b, &format!("{me}$server"), served);
    Ok(b.group(vec![signed, run_body, typed, made, calls, served]))
}

/// `Server.Derive.Procedure P {E1, E2} S`, the parts the user's, a payload
/// of several parameters a tuple of them.
fn procedure_type(b: &mut Builder, parts: &Parts) -> NodeId {
    let each = |b: &mut Builder, spans: &[(Span, Ty)]| -> Vec<NodeId> {
        spans.iter().map(|&(s, _)| b.ty(Type::Signed(s))).collect()
    };
    let params = each(b, &parts.params);
    let payload = match &params[..] {
        [one] => *one,
        _ => b.ty(Type::Tuple(params)),
    };
    let errors = each(b, &parts.errors);
    let errors = b.ty(Type::Row(errors));
    let success = b.ty(Type::Signed(parts.success.0));
    let function = b.ty(Type::Name("Server.Derive.Procedure".into()));
    b.ty(Type::Apply {
        function,
        arguments: vec![payload, errors, success],
    })
}

/// Whether `signature` is the one of `me`.
fn signs(signature: &Signature, me: &str) -> bool {
    match &signature.tokens[..] {
        [name, colon, ..] => text(signature, *name) == me && text(signature, *colon) == ":",
        _ => false,
    }
}

fn text(signature: &Signature, token: Token) -> &str {
    let Span { start, end } = token.span;
    signature
        .text
        .get(start as usize..end as usize)
        .unwrap_or("")
}

/// The parameters, the errors and the success of a signature
/// `name : A -> B ->{r; e} S`, or why it is no server fn's.
fn parts(signature: &Signature) -> Result<Parts, &'static str> {
    let sig = Block {
        head: String::new(),
        text: signature.text.clone(),
        body: 0,
        place: Place::Declaration,
        tokens: signature.tokens.clone(),
        docs: String::new(),
        signature: None,
        module: String::new(),
    };
    let tokens = &signature.tokens[2.min(signature.tokens.len())..];
    let at = |i: usize| tokens.get(i).map_or("", |t| text(signature, *t));
    if at(0) == "forall" || (0..tokens.len()).any(|i| at(i) == "=>") {
        return Err("a server fn is not generic: its types cross the network");
    }
    // the pieces between the arrows outside brackets, and each arrow's row
    let mut pieces: Vec<(usize, usize)> = Vec::new();
    let mut rows: Vec<Option<(usize, usize)>> = Vec::new();
    let (mut depth, mut start, mut i) = (0usize, 0usize, 0usize);
    while i < tokens.len() {
        match at(i) {
            "(" | "{" | "[" => depth += 1,
            ")" | "}" | "]" => depth = depth.saturating_sub(1),
            "->" if depth == 0 => {
                pieces.push((start, i));
                let row = (at(i + 1) == "{").then(|| closing(tokens, signature, i + 1));
                rows.push(row.map(|end| (i + 2, end)));
                i = row.map_or(i + 1, |end| end + 1);
                start = i;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    let Some((&last, before)) = rows.split_last() else {
        return Err("a server fn is a function: `P ->{r; e} S`");
    };
    if before.iter().any(Option::is_some) {
        return Err("only the last arrow of a server fn runs: `A -> B ->{r; e} S`");
    }
    let Some((row_start, row_end)) = last else {
        return Err("a server fn runs on the server: its arrow is `->{r; e}`");
    };
    let span =
        |(from, to): (usize, usize)| match (tokens.get(from), tokens.get(to.max(from + 1) - 1)) {
            (Some(first), Some(last)) if from < to => Some(Span {
                start: first.span.start,
                end: last.span.end,
            }),
            _ => None,
        };
    let read = |range: (usize, usize)| -> Result<(Span, Ty), &'static str> {
        let span = span(range).ok_or("a server fn is a function: `P ->{r; e} S`")?;
        Ok((
            span,
            ty::read(&sig, span).map_err(|_| "its types are not read")?,
        ))
    };
    let params = pieces.iter().map(|&p| read(p)).collect::<Result<_, _>>()?;
    let success = read((start, tokens.len()))?;
    let whole = span((0, tokens.len())).ok_or("a server fn is a function: `P ->{r; e} S`")?;
    // `{s; e1, e2}`: the services, then the errors
    let semicolon = (row_start..row_end).find(|&k| at(k) == ";");
    let mut errors = Vec::new();
    let mut guarded = false;
    if let Some(semicolon) = semicolon {
        let mut from = semicolon + 1;
        let mut depth = 0usize;
        for k in semicolon + 1..=row_end {
            let piece = at(k);
            match piece {
                "(" | "{" | "[" => depth += 1,
                ")" | "}" | "]" if k < row_end => depth = depth.saturating_sub(1),
                "|" if depth == 0 => {
                    return Err(
                        "a server fn lists its errors: each crosses the network with its codec",
                    );
                }
                _ => {}
            }
            if (piece == "," && depth == 0) || k == row_end {
                if from < k {
                    let error = span((from, k)).map(|s| sig.text_of(s).to_owned());
                    guarded |= error.as_deref().is_some_and(|e| UNAUTHORIZED.contains(&e));
                    errors.push(read((from, k))?);
                }
                from = k + 1;
            }
        }
    }
    Ok(Parts {
        whole,
        params,
        errors,
        success,
        guarded,
    })
}

/// The index of the bracket that closes the one at `open`.
fn closing(tokens: &[Token], signature: &Signature, open: usize) -> usize {
    let mut depth = 0usize;
    for (i, t) in tokens.iter().enumerate().skip(open) {
        match text(signature, *t) {
            "(" | "{" | "[" => depth += 1,
            ")" | "}" | "]" => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
    }
    tokens.len()
}

/// The witnesses of the payload, the success and the errors, as
/// `Server.Derive.procedure` takes them.
fn witnesses(b: &mut Builder, parts: &Parts, me: &str) -> Result<Vec<NodeId>, String> {
    // shortcut: `expand` has no summary of the module, so a local alias
    // without `deriving Schema` is `T.schema` the checker does not find;
    // give `expand` the summary a derive gets when a program needs one.
    let module = Module { types: Vec::new() };
    let local = Local {
        name: String::new(),
        params: Vec::new(),
        form: Form::Abstract,
        deriving: Vec::new(),
    };
    let mut schemas = Gen::new(&module, &local);
    let mut send = |b: &mut Builder, what: &str, ty: &Ty| {
        schemas
            .schema(b, ty, what)
            .map_err(|(_, why)| format!("server fn `{me}` sends its {what} as JSON, but {why}"))
    };
    let payload = match &parts.params[..] {
        [(_, one)] => one.clone(),
        all => Ty::Tuple(all.iter().map(|(_, t)| t.clone()).collect()),
    };
    let payload = send(b, "payload", &payload)?;
    let success = send(b, "result", &parts.success.1)?;
    let mut errors = name(b, "Server.Derive.no_errors");
    for (_, error) in parts.errors.iter().rev() {
        let witness = send(b, &format!("error `{error}`"), error)?;
        errors = call(b, "Server.Derive.with_error", vec![witness, errors]);
    }
    Ok(vec![payload, success, errors])
}

/// `name$impl`, or for several parameters `fun (p0, p1) -> name$impl p0 p1`.
fn runs(b: &mut Builder, implementation: &str, arity: usize) -> NodeId {
    if arity == 1 {
        return name(b, implementation);
    }
    let names: Vec<String> = (0..arity).map(|i| format!("p{i}")).collect();
    let vars = names.iter().map(|n| b.pat(Pat::Var(n.clone()))).collect();
    let tuple = b.pat(Pat::Tuple(vars));
    let args = names.iter().map(|n| name(b, n)).collect();
    let body = call(b, implementation, args);
    b.expr(Expr::Lambda {
        params: vec![tuple],
        body,
    })
}

/// `Server.Derive.call name$procedure`, or for several parameters
/// `fun p0 p1 -> Server.Derive.call name$procedure (p0, p1)`.
fn calls(b: &mut Builder, procedure: &str, arity: usize) -> NodeId {
    let made = name(b, procedure);
    if arity == 1 {
        return call(b, "Server.Derive.call", vec![made]);
    }
    let names: Vec<String> = (0..arity).map(|i| format!("p{i}")).collect();
    let params = names.iter().map(|n| b.pat(Pat::Var(n.clone()))).collect();
    let items = names.iter().map(|n| name(b, n)).collect();
    let payload = b.expr(Expr::Tuple(items));
    let body = call(b, "Server.Derive.call", vec![made, payload]);
    b.expr(Expr::Lambda { params, body })
}

fn let_(b: &mut Builder, named: &str, body: NodeId) -> NodeId {
    b.decl(Decl::Let {
        name: named.to_owned(),
        params: Vec::new(),
        ty: None,
        body,
        rec: false,
    })
}
