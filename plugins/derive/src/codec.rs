//! `codec s { label = field "wire" c; … }`: the codec of the record the
//! schema `s` describes, each field given once with a `FieldCodec`. The code
//! gathers the fields with `Codec.Derive` in the written order; what only
//! the types show, the record's fields and the type each codec decodes, the
//! `_` of that code ask the checker, and `check` reads.

use flick_plugin::{
    Arena, Block, Builder, Compiler, Diagnostic, Expr, Fact, Hole, HoleKind, NodeId, Piece, Span,
    Token, TokenKind, Ty as Type, string_value,
};

use crate::schema::{call, name};

/// The fields whose codec makes a block a `Codec` of services, not pure.
const EFFECTFUL: [&str; 2] = ["field_effect", "on_missing_effect"];

/// The fields whose first argument is the wire name.
const WIRED: [&str; 3] = ["field", "optional_field", "field_effect"];

/// A field of a block: its label, where it stands, and its codec.
struct Field {
    label: String,
    at: Span,
    value: Span,
}

/// The code of the block `codec s { … }`.
pub fn expand(block: &Block, compiler: &mut dyn Compiler) -> Result<Arena, Vec<Diagnostic>> {
    let whole = block.whole();
    let refuse =
        |message: &str, at: Span, code: &str| vec![Diagnostic::error(message, at).with_code(code)];
    let schema = schema_span(block).ok_or_else(|| {
        refuse(
            "a `codec` block names the schema of its record: `codec T.schema { … }`",
            whole,
            "D007",
        )
    })?;
    let hole = compiler
        .parse_expr(schema.start, schema.end)
        .map_err(|d| vec![d])?;
    if hole.span != schema {
        return Err(refuse(
            "expected the schema of a record before `{`",
            schema,
            "D007",
        ));
    }
    let fields = fields(block, compiler)?;
    let mut wrong = Vec::new();
    let mut given: Vec<&str> = Vec::new();
    let mut wires: Vec<String> = Vec::new();
    for f in &fields {
        if given.contains(&f.label.as_str()) {
            let message = format!("`{}` is given twice", f.label);
            wrong.push(Diagnostic::error(message, f.at).with_code("D003"));
        }
        given.push(&f.label);
        if let Some(wire) = wire_name(block, f.value) {
            if wires.contains(&wire) {
                let message = format!("the wire name \"{wire}\" is already a field's");
                wrong.push(Diagnostic::error(message, f.value).with_code("D005"));
            }
            wires.push(wire);
        }
    }
    if !wrong.is_empty() {
        return Err(wrong);
    }
    let mut b = Builder::new(whole);
    let effectful = (block.tokens.iter())
        .any(|t| t.kind == TokenKind::Ident && EFFECTFUL.contains(&block.text_of(t.span)));
    let mut rest = name(&mut b, "Codec.Derive.fields");
    for f in &fields {
        let row = b.at(whole).ty(Type::Wildcard);
        let decoded = b.at(f.at).ty(Type::Wildcard);
        let ty = applied(b.at(whole), "FieldCodec", vec![row, decoded]);
        let value = b.expr(Expr::Hole(f.value));
        let codec = b.expr(Expr::Annot { expr: value, ty });
        let label = b.expr(Expr::Text(f.label.clone()));
        rest = call(&mut b, "Codec.Derive.field", vec![label, codec, rest]);
    }
    let record = b.at(schema).ty(Type::Wildcard);
    let ty = applied(b.at(whole), "Schema", vec![record]);
    let value = b.expr(Expr::Hole(hole.span));
    let schema = b.expr(Expr::Annot { expr: value, ty });
    let named = b.expr(Expr::Text(type_name(block)));
    let made = if effectful {
        "Codec.Derive.record_effect"
    } else {
        "Codec.Derive.record"
    };
    let root = call(&mut b, made, vec![named, schema, rest]);
    Ok(b.finish(root))
}

/// What only the types of the block's code show: a schema of no record
/// (`D007`), a field the record lacks (`D004`), fields the block misses
/// (`D002`), and a codec of another type than its field's (`D012`).
pub fn check(block: &Block, facts: &[Fact]) -> Vec<Diagnostic> {
    let whole = block.whole();
    let Some(schema) = schema_span(block) else {
        return Vec::new();
    };
    let Some(record) = facts.iter().find(|f| f.span == schema) else {
        return Vec::new();
    };
    let Some(fields) = &record.fields else {
        if record.ty.exact.is_none() {
            return Vec::new();
        }
        let message = format!(
            "a `codec` block describes a record, but this is a schema of `{}`",
            record.ty.shown
        );
        return vec![Diagnostic::error(message, schema).with_code("D007")];
    };
    let mut out = Vec::new();
    let mut given: Vec<String> = Vec::new();
    for fact in facts.iter().filter(|f| f.span != schema && f.span != whole) {
        let Some(label) = label_at(block, fact.span) else {
            continue;
        };
        given.push(label.to_owned());
        let Some(field) = fields.iter().find(|f| f.label == label) else {
            let message = format!("the record has no field `{label}`");
            out.push(Diagnostic::error(message, fact.span).with_code("D004"));
            continue;
        };
        // shortcut: a codec of a type still unknown is not compared; compare
        // by unifying when the checker gives plugins that
        if let (Some(wanted), Some(decoded)) = (&field.ty.exact, &fact.ty.exact)
            && wanted != decoded
        {
            let message = format!(
                "`{label}` is a `{}`, but its codec decodes `{}`",
                field.ty.shown, fact.ty.shown
            );
            out.push(Diagnostic::error(message, fact.span).with_code("D012"));
        }
    }
    let missing: Vec<String> = (fields.iter())
        .filter(|f| !given.contains(&f.label))
        .map(|f| format!("`{}`", f.label))
        .collect();
    if !missing.is_empty() {
        let message = format!("the codec misses the field {}", missing.join(", "));
        out.push(Diagnostic::error(message, whole).with_code("D002"));
    }
    out
}

/// The layout of the block: the schema and `{`, then a field a line, one
/// level in, each `label = codec;`; a block written on one line stays on
/// one, `{ a = c; b = d }`. None when it does not read.
pub fn format(block: &Block, compiler: &mut dyn Compiler) -> Option<Vec<Piece>> {
    let schema = schema_span(block)?;
    let hole = |span: Span| {
        Piece::Hole(Hole {
            kind: HoleKind::Expr,
            span,
        })
    };
    let fields = fields(block, compiler).ok()?;
    let text = |t: &str| Piece::Text(t.to_owned());
    if !block.text.contains('\n') {
        let mut out = vec![hole(schema), text(" {")];
        for (i, f) in fields.iter().enumerate() {
            let before = if i == 0 { " " } else { "; " };
            out.extend([text(&format!("{before}{} = ", f.label)), hole(f.value)]);
        }
        out.push(text(" "));
        return Some(out);
    }
    let mut out = vec![hole(schema), text(" {"), Piece::Indent];
    for f in &fields {
        out.extend([
            Piece::Line,
            text(&format!("{} = ", f.label)),
            hole(f.value),
            text(";"),
        ]);
    }
    out.extend([Piece::Dedent, Piece::Line]);
    Some(out)
}

/// The schema of the block: its arguments, before `{`.
fn schema_span(block: &Block) -> Option<Span> {
    let args = block.arguments();
    Some(Span {
        start: args.first()?.span.start,
        end: args.last()?.span.end,
    })
}

/// The name of the type whose schema the block names, `Profile` of
/// `Profile.schema` or `schema Profile`, for what its codec reports.
fn type_name(block: &Block) -> String {
    let words: Vec<(&str, TokenKind)> = (block.arguments().iter())
        .map(|t| (block.text_of(t.span), t.kind))
        .collect();
    let named = match &words[..] {
        [.., (ty, TokenKind::Con), (".", _), ("schema", _)]
        | [.., ("schema", _), (ty, TokenKind::Con)]
        | [.., ("schema", _), (ty, TokenKind::Con), (")", _)] => Some(*ty),
        _ => None,
    };
    named.unwrap_or_default().to_owned()
}

/// The fields of the body, `label = codec` each, `;` or a line between.
fn fields(block: &Block, compiler: &mut dyn Compiler) -> Result<Vec<Field>, Vec<Diagnostic>> {
    let mut c = block.body();
    let mut out = Vec::new();
    while let Some(label) = c.ident() {
        let expected = || {
            let message = "expected a field `label = codec`";
            vec![Diagnostic::error(message, label.span)]
        };
        c.eat("=").ok_or_else(expected)?;
        let value = c.expr(compiler).map_err(|d| vec![d])?;
        c.eat(";");
        out.push(Field {
            label: block.text_of(label.span).to_owned(),
            at: Span {
                start: label.span.start,
                end: value.end,
            },
            value,
        });
    }
    if let Some(t) = c.peek() {
        return Err(vec![Diagnostic::error(
            "expected a field `label = codec`",
            t.span,
        )]);
    }
    Ok(out)
}

/// The wire name a field's codec gives: `"wire"` of `field "wire" c`.
fn wire_name(block: &Block, value: Span) -> Option<String> {
    let tokens: Vec<&Token> = (block.tokens.iter())
        .filter(|t| value.start <= t.span.start && t.span.end <= value.end)
        .collect();
    tokens.windows(2).find_map(|w| {
        let head = block.text_of(w[0].span);
        let last = head.rsplit('.').next().unwrap_or(head);
        (w[0].kind == TokenKind::Ident && WIRED.contains(&last) && w[1].kind == TokenKind::String)
            .then(|| string_value(block.text_of(w[1].span)))
    })
}

/// The label of the field at `span`: its first word.
fn label_at(block: &Block, span: Span) -> Option<&str> {
    let first = (block.tokens.iter()).find(|t| t.span.start == span.start)?;
    (first.kind == TokenKind::Ident).then(|| block.text_of(first.span))
}

/// `name args`, a type of the Prelude applied.
fn applied(b: &mut Builder, named: &str, arguments: Vec<NodeId>) -> NodeId {
    let function = b.ty(Type::Name(named.to_owned()));
    b.ty(Type::Apply {
        function,
        arguments,
    })
}
