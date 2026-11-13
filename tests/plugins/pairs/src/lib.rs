//! A plugin for the compiler's tests. `pairs { k = e; k : T = e }` stands
//! for the record `{ k = e; k = (e : T); blocks = n }`, where `n` is the
//! number of `pairs` blocks of the group, which `collect` counts; each
//! argument of the head, as in `pairs "a" (1 + 2) { … }`, adds a field
//! `arg0`, `arg1`, … of its value. The heads of the other forms of blocks
//! are in `forms`. `bad {}`
//! stands for `(1.0 : String)`, an error in generated code; `coded { X001 }`
//! refuses with the code its body names. `spin {}` never
//! ends and `hog {}` takes all memory: the limits of a plugin's calls.
//! The calls the package watches are in `check_calls`: `tally` counts from
//! one and `boom` fails the plugin.
//! `deriving Pairs.Arbitrary` is in `derive`, the heads that reach
//! outside their blocks are in `reach`, and those that tell other modules
//! about a name in `published`.

mod derive;
mod forms;
mod published;
mod reach;

use std::hint::black_box;
use std::path::Path;

use flick_plugin::{
    Arena, Artifact, Block, Builder, CallSite, Compiler, Constant, DeriveTarget, Diagnostic, Edit,
    Fact, NodeId, Plugin, Published, Refinement, Span, Token, TokenKind, flick,
};

pub struct Pairs;

/// What `reach` read and the notes of `published`, in a state.
fn parts(state: &[u8]) -> (&[u8], &[u8]) {
    let len = (state.get(4..8))
        .and_then(|s| s.try_into().ok())
        .map_or(0, u32::from_le_bytes);
    let len = usize::try_from(len).unwrap_or(usize::MAX);
    let rest = state.get(8..).unwrap_or_default();
    rest.split_at(len.min(rest.len()))
}

#[cfg(feature = "guest")]
flick_plugin::export!(Pairs);

impl Plugin for Pairs {
    /// The IR with each string `"hello"` as `"HELLO"`; a string `"break the
    /// IR"` becomes a local nothing binds, a module with a string `"move
    /// the package"` goes to another package, and one with `"hide the
    /// exports"` exports nothing.
    fn transform(&self, program: Vec<u8>) -> Result<Vec<u8>, Vec<Diagnostic>> {
        let mut program = flick_ir::Program::from_cbor(&program)
            .map_err(|e| vec![Diagnostic::error(e.to_string(), Span::default())])?;
        for module in &mut program.modules {
            let mut said = Said::default();
            for item in &mut module.items {
                match &mut item.kind {
                    flick_ir::ItemKind::Value { body, .. } => shout(body, &mut said),
                    flick_ir::ItemKind::Instance { methods, .. } => {
                        for (_, body) in methods {
                            shout(body, &mut said);
                        }
                    }
                    _ => {}
                }
            }
            if said.moved {
                module.package = Some("elsewhere".into());
            }
            if said.hidden {
                module.exports = flick_ir::Exports::default();
            }
        }
        Ok(program.to_cbor())
    }

    /// Each `.js` with a line `// pairs` before it, its `.map` a line later.
    fn emit(&self, artifacts: Vec<Artifact>) -> Result<Vec<Artifact>, Vec<Diagnostic>> {
        Ok((artifacts.into_iter())
            .map(|mut a| {
                let extension = Path::new(&a.path).extension();
                if extension.is_some_and(|e| e == "js") {
                    a.contents = format!("// pairs\n{}", a.contents);
                } else if extension.is_some_and(|e| e == "map") {
                    a.contents = a
                        .contents
                        .replacen("\"mappings\":\"", "\"mappings\":\";", 1);
                }
                a
            })
            .collect())
    }

    /// A `.pairs` file: a line `name = "text"` is `let name : String =
    /// "text"`, at that line; any other line that is not blank is `X004`.
    fn load(&self, _: &str, bytes: &[u8]) -> Result<Arena, Vec<Diagnostic>> {
        let text = String::from_utf8_lossy(bytes);
        let mut b = Builder::new(Span { start: 0, end: 0 });
        let mut decls = Vec::new();
        let mut start = 0u32;
        for line in text.split_inclusive('\n') {
            let at = Span {
                start,
                end: start + u32::try_from(line.trim_end().len()).unwrap_or(0),
            };
            start += u32::try_from(line.len()).unwrap_or(0);
            if line.trim().is_empty() {
                continue;
            }
            let parsed = (line.split_once('=')).and_then(|(name, value)| {
                let value = value.trim().strip_prefix('"')?.strip_suffix('"')?;
                Some((name.trim(), value))
            });
            let Some((name, value)) = parsed else {
                let message = "expected `name = \"text\"`";
                return Err(vec![Diagnostic::error(message, at).with_code("X004")]);
            };
            let ty = b.at(at).ty(flick_plugin::Ty::Name("String".into()));
            let body = b.at(at).expr(flick_plugin::Expr::Text(value.into()));
            decls.push(b.at(at).decl(flick_plugin::Decl::Let {
                name: name.into(),
                params: Vec::new(),
                ty: Some(ty),
                body,
                rec: false,
            }));
        }
        let root = b.group(decls);
        Ok(b.finish(root))
    }

    /// The number of `pairs` blocks, the length of what `reach` reads and
    /// it, then the notes of `published`.
    fn collect(&self, blocks: &[Block], compiler: &mut dyn Compiler) -> Vec<u8> {
        let pairs = blocks.iter().filter(|b| b.head == "pairs").count();
        let count = u32::try_from(pairs).unwrap_or(u32::MAX).to_le_bytes();
        let reached = reach::collect(blocks, compiler);
        let len = u32::try_from(reached.len())
            .unwrap_or(u32::MAX)
            .to_le_bytes();
        [
            count.as_slice(),
            &len,
            &reached,
            &published::collect(blocks),
        ]
        .concat()
    }

    fn publish(&self, state: &[u8]) -> Vec<Published> {
        published::publish(parts(state).1)
    }

    fn expand(
        &self,
        block: &Block,
        state: &[u8],
        compiler: &mut dyn Compiler,
    ) -> Result<Arena, Vec<Diagnostic>> {
        if let Some(made) = forms::expand(block, compiler) {
            return made;
        }
        if let Some(made) = reach::expand(block, parts(state).0, compiler) {
            return made;
        }
        if let Some(made) = published::expand(block, compiler) {
            return made;
        }
        let whole = block.whole();
        let mut b = Builder::new(whole);
        match block.head.as_str() {
            "spin" => loop {
                black_box(block);
            },
            "hog" => {
                let mut held = Vec::new();
                loop {
                    held.push(vec![1u8; 1 << 20]);
                    black_box(&held);
                }
            }
            _ => {}
        }
        if block.head == "coded" {
            let code = block.text.trim();
            return Err(vec![Diagnostic::error("refused", whole).with_code(code)]);
        }
        if block.head == "bad" {
            let wrong = flick!(b, (1.0 : String));
            return Ok(b.finish(wrong));
        }
        let mut fields = arguments(block, compiler, &mut b)?;
        let mut c = block.body();
        while let Some(name) = c.next() {
            if name.kind != TokenKind::Ident {
                return Err(vec![refuse(name, "expected a name")]);
            }
            let ty = match c.eat(":") {
                Some(_) => Some(c.ty(compiler).map_err(|d| vec![d])?),
                None => None,
            };
            if c.eat("=").is_none() {
                return Err(vec![refuse(c.peek().unwrap_or(name), "expected `=`")]);
            }
            let value = c.expr(compiler).map_err(|d| vec![d])?;
            c.eat(";");
            // a pair with a type stands for its name to its value
            let node = if let Some(ty) = ty {
                let pair = Span {
                    start: name.span.start,
                    end: value.end,
                };
                flick!(b @ pair, (#value : #ty))
            } else {
                flick!(b @ value, #value)
            };
            fields.push((c.text(name).to_owned(), node));
        }
        let count = (state.get(..4))
            .and_then(|s| s.try_into().ok())
            .map_or(0, u32::from_le_bytes);
        let record = flick!(b @ whole, { #(..fields); blocks = #(f64::from(count)) });
        Ok(b.finish(record))
    }

    /// What `typed { e }` checks: the type of its `_`, as `X002`; and what
    /// `linted { e }` warns of, `X003`, with a fix that writes `e` as `(e)`.
    fn check(&self, block: &Block, _: &[u8], facts: &[Fact]) -> Vec<Diagnostic> {
        if block.head == "linted" {
            return (facts.iter())
                .map(|f| {
                    let text = &block.text[f.span.start as usize..f.span.end as usize];
                    let edit = Edit {
                        span: f.span,
                        replacement: format!("({text})"),
                    };
                    Diagnostic::warning("write it in parentheses", f.span)
                        .with_code("X003")
                        .with_fix("add the parentheses", vec![edit], true)
                })
                .collect();
        }
        if block.head != "typed" {
            return Vec::new();
        }
        (facts.iter())
            .map(|f| {
                Diagnostic::error(format!("`_` is `{}`", f.ty.shown), f.span).with_code("X002")
            })
            .collect()
    }

    /// What the calls the package watches must hold: `tally` counts from
    /// one, as `X005` at its argument. A call of `boom` fails the plugin.
    /// A value a watched tag reaches is `X006` where a function names it,
    /// and a function whose signature names a watched service is `X007`.
    fn check_calls(&self, calls: &[CallSite], _: &[u8]) -> Vec<Diagnostic> {
        let mut found = Vec::new();
        for call in calls {
            assert!(call.name != "boom", "no calls");
            if let Some(tagged) = &call.tagged {
                let around: Vec<String> = (tagged.inside.iter())
                    .map(|f| {
                        format!(
                            "{{{}}}{}",
                            f.services.join(", "),
                            if f.open { ".." } else { "" }
                        )
                    })
                    .collect();
                let message = format!(
                    "`{}` reaches `{}` of `@{}` inside {}",
                    tagged.named,
                    tagged.reaches,
                    tagged.tag,
                    around.join(" in ")
                );
                found.push(Diagnostic::error(message, call.span).with_code("X006"));
                continue;
            }
            if let Some(binding) = &call.binding
                && let Some(gives) = &binding.gives
            {
                let name = binding.name.as_deref().unwrap_or_default();
                let message = format!(
                    "`{name}` asks for `{}.{}` to give `{}`",
                    call.module, call.name, gives.shown
                );
                found.push(Diagnostic::error(message, call.span).with_code("X007"));
                continue;
            }
            // a constant given to a function another declaration reads
            if let Some(Refinement::Predicate(expression)) = &call.refinement
                && call.name.ends_with("$admit")
            {
                let message = format!(
                    "`{}` is read by an expression of {} nodes",
                    call.name.replace('$', "."),
                    expression.nodes.len()
                );
                found.push(Diagnostic::error(message, call.span).with_code("X009"));
                continue;
            }
            // where a watched value is named: the declaration around it
            if call.name == "keyed"
                && let Some(binding) = &call.binding
            {
                let message = format!(
                    "`keyed` in `{}`, given {}{}",
                    binding.name.as_deref().unwrap_or("_"),
                    binding.applied,
                    if binding.in_function {
                        ", in a function"
                    } else {
                        ""
                    }
                );
                found.push(Diagnostic::error(message, call.span).with_code("X008"));
                continue;
            }
            // A bare `tally` is a function value, not a zero-argument call:
            // it has no argument to count from.
            let Some(argument) = call.arguments.first().filter(|_| call.name == "tally") else {
                continue;
            };
            if !matches!(argument.constant, Some(Constant::Number(n)) if n > 0.0) {
                let refusal = Diagnostic::error("`tally` counts from one", argument.span);
                found.push(refusal.with_code("X005"));
            }
        }
        found
    }

    fn derive(
        &self,
        target: &DeriveTarget,
        class: &str,
        _: Option<&Block>,
        _: &[DeriveTarget],
        _: &[u8],
        _: &mut dyn Compiler,
    ) -> Result<Arena, Vec<Diagnostic>> {
        derive::derive(target, class)
    }
}

/// A field `arg<i>` for each argument of the head: a string, or `( … )`.
fn arguments(
    block: &Block,
    compiler: &mut dyn Compiler,
    b: &mut Builder,
) -> Result<Vec<(String, NodeId)>, Vec<Diagnostic>> {
    let mut fields = Vec::new();
    let mut depth = 0u32;
    let mut start = None;
    for t in block.arguments() {
        start = start.or(Some(t.span.start));
        match block.text_of(t.span) {
            "(" => depth += 1,
            ")" => depth -= 1,
            _ => {}
        }
        if depth == 0
            && let Some(from) = start.take()
        {
            let value = compiler
                .parse_expr(from, t.span.end)
                .map_err(|d| vec![d])?
                .span;
            fields.push((format!("arg{}", fields.len()), flick!(b @ value, #value)));
        }
    }
    Ok(fields)
}

fn refuse(token: Token, message: &str) -> Diagnostic {
    Diagnostic::error(message, token.span)
}

/// What the strings of a module ask of its metadata: `"move the package"`
/// and `"hide the exports"`.
#[derive(Default)]
struct Said {
    moved: bool,
    hidden: bool,
}

/// `e` with each string `"hello"` in it as `"HELLO"`, and `"break the IR"`
/// as a local nothing binds; `said` holds what its strings ask.
fn shout(e: &mut flick_ir::Expr, said: &mut Said) {
    if let flick_ir::ExprKind::Lit(flick_ir::Lit::String(s)) = &mut e.kind {
        said.moved |= s == "move the package";
        said.hidden |= s == "hide the exports";
        if s == "hello" {
            *s = "HELLO".into();
        } else if s == "break the IR" {
            e.kind = flick_ir::ExprKind::Local("dangling".into());
        }
    }
    e.each_child_mut(&mut |child| shout(child, said));
}
