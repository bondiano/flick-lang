//! `deriving Url`: `T$url : Url T`, a path for each constructor of `T`: its
//! name in kebab case (`Home` and `Index` are `/`), its fields the segments
//! after it, the last one an `Option` that may be missing or another route
//! type, the rest of the path, and a final record field the query.

use flick_plugin::{Builder, Decl, Expr, NodeId, Pat, Ty as Type};

use crate::module::{Abi, Form, Local, Module};
use crate::schema::{Fail, Gen, call, name};
use crate::ty::Ty;

/// How a field of a constructor makes a segment.
enum Segment {
    /// Its value: a scalar.
    Value(Ty),
    /// The constructors of a type without fields, by their words.
    Words(Vec<String>),
    /// Another route type: the rest of the path.
    Nested(String),
}

/// A constructor's path.
struct Route {
    constructor: String,
    path: String,
    segments: Vec<Segment>,
    query: Option<Vec<(String, Ty, bool)>>,
    optional: bool,
}

/// `T$url` and its signature.
pub fn declarations(b: &mut Builder, module: &Module, local: &Local) -> Result<NodeId, Fail> {
    let me = local.name.clone();
    let fail = |why: &str| -> Fail { (me.clone(), why.to_owned()) };
    if !local.derives("Schema") {
        return Err(fail("a type that derives `Url` derives `Schema` too"));
    }
    let Form::Adt {
        cases,
        abi: Abi::Tagged,
        ..
    } = &local.form
    else {
        return Err(fail("a route type is a variant type"));
    };
    if !local.params.is_empty() {
        return Err(fail("a generic type has no URL form"));
    }
    let mut routes: Vec<Route> = Vec::new();
    for (con, fields) in cases {
        let route = route(module, &me, con, fields)?;
        if let Some(other) = routes.iter().find(|r| overlaps(r, &route)) {
            return Err((
                format!("{me}.{con}"),
                format!("its URLs are those of `{}`", other.constructor),
            ));
        }
        routes.push(route);
    }
    let mut schemas = Gen::new(module, local);
    let mut paths = Vec::new();
    for r in &routes {
        paths.push(path(b, &mut schemas, &me, r)?);
    }
    let schema = name(b, &format!("{me}$schema"));
    let paths = b.expr(Expr::Items(paths));
    let body = call(b, "Route.codec", vec![schema, paths]);
    let url = b.ty(Type::Name("Url".into()));
    let this = b.ty(Type::Name(me.clone()));
    let ty = b.ty(Type::Apply {
        function: url,
        arguments: vec![this],
    });
    let sig = b.decl(Decl::Signature {
        name: format!("{me}.url"),
        ty,
    });
    let value = b.decl(Decl::Let {
        name: format!("{me}.url"),
        params: Vec::new(),
        ty: None,
        body,
        rec: false,
    });
    Ok(b.group(vec![sig, value]))
}

/// The route of constructor `con` of `me` with `fields`.
fn route(module: &Module, me: &str, con: &str, fields: &[Ty]) -> Result<Route, Fail> {
    let mut fields: Vec<Ty> = fields.iter().map(|f| expand(module, f)).collect();
    let query = match fields.last() {
        Some(Ty::Record { fields: q, .. }) => {
            let at = fields.len() - 1;
            let mut out = Vec::new();
            for (label, ty) in q {
                let (inner, optional) = optional(ty);
                if !scalar(module, inner) {
                    return Err((
                        format!("{me}.{con}[{at}].{label}"),
                        "a query parameter is a segment's type or an `Option` of one".into(),
                    ));
                }
                out.push((label.clone(), inner.clone(), optional));
            }
            fields.pop();
            Some(out)
        }
        _ => None,
    };
    let mut optional_last = false;
    if let Some(last) = fields.last()
        && let (inner, true) = optional(last)
    {
        let inner = inner.clone();
        optional_last = true;
        if let Some(l) = fields.last_mut() {
            *l = inner;
        }
    }
    let count = fields.len();
    let mut segments = Vec::new();
    for (i, field) in fields.iter().enumerate() {
        let segment = segment(module, field).ok_or_else(|| {
            (
                format!("{me}.{con}[{i}]"),
                "a segment is a `String`, a `Float`, a `Bool`, a refined type over them, a type whose constructors have no fields or one that derives `Url`; only the last may be an `Option`".to_owned(),
            )
        })?;
        if matches!(segment, Segment::Nested(_)) && i + 1 != count {
            return Err((
                format!("{me}.{con}[{i}]"),
                "a nested route is the rest of the path: only the last segment may be one".into(),
            ));
        }
        segments.push(segment);
    }
    Ok(Route {
        constructor: con.to_owned(),
        path: if con == "Home" || con == "Index" {
            String::new()
        } else {
            kebab(con)
        },
        segments,
        query,
        optional: optional_last,
    })
}

/// `ty` with the local aliases it names written out.
fn expand(module: &Module, ty: &Ty) -> Ty {
    if let Ty::Con { name, args } = ty
        && let Some(Local {
            form: Form::Alias {
                body,
                opaque: false,
            },
            params,
            ..
        }) = module.local(name)
    {
        let with: Vec<(String, Ty)> = params
            .iter()
            .map(|(p, _)| p.clone())
            .zip(args.iter().cloned())
            .collect();
        return expand(module, &body.substitute(&with));
    }
    ty.clone()
}

/// The type an `Option` holds, and whether `ty` is one.
fn optional(ty: &Ty) -> (&Ty, bool) {
    match ty {
        Ty::Con { name, args }
            if (name == "Option" || name == "Prelude.Option") && args.len() == 1 =>
        {
            (&args[0], true)
        }
        other => (other, false),
    }
}

/// Whether a field of `ty` is one segment of text.
fn scalar(module: &Module, ty: &Ty) -> bool {
    let Ty::Con { name, args } = ty else {
        return false;
    };
    if !args.is_empty() {
        return false;
    }
    let short = name.strip_prefix("Prelude.").unwrap_or(name);
    if ["String", "Int", "Float", "Bool"].contains(&short) {
        return true;
    }
    match module.local(name) {
        Some(Local {
            form: Form::Refined { base, .. },
            ..
        }) => scalar(module, base),
        Some(Local {
            form: Form::Abstract,
            ..
        }) => true,
        Some(_) => false,
        // shortcut: a type of another module counts as a scalar, its schema checking the text; read its form from its interface once a derive gets one
        None => !["Unit", "Never", "Unknown", "BigInt"].contains(&short),
    }
}

/// The segment a field of `ty` makes, if any.
fn segment(module: &Module, ty: &Ty) -> Option<Segment> {
    if scalar(module, ty) {
        return Some(Segment::Value(ty.clone()));
    }
    let Ty::Con { name, args } = ty else {
        return None;
    };
    let local = module.local(name)?;
    if !args.is_empty() {
        return None;
    }
    if let Form::Adt {
        cases,
        abi: Abi::Tagged,
        ..
    } = &local.form
        && cases.iter().all(|(_, f)| f.is_empty())
    {
        let words: Vec<String> = cases.iter().map(|(c, _)| kebab(c)).collect();
        let distinct = words
            .iter()
            .enumerate()
            .all(|(i, w)| !words[..i].contains(w));
        if distinct {
            return Some(Segment::Words(
                cases.iter().map(|(c, _)| c.clone()).collect(),
            ));
        }
    }
    local.derives("Url").then(|| Segment::Nested(name.clone()))
}

/// Whether two routes can match one URL: the same path and counts of
/// segments that meet.
fn overlaps(a: &Route, b: &Route) -> bool {
    let range = |r: &Route| {
        let n = r.segments.len();
        let open = r.optional || matches!(r.segments.last(), Some(Segment::Nested(_)));
        let nested = matches!(r.segments.last(), Some(Segment::Nested(_)));
        (n - usize::from(open), if nested { usize::MAX } else { n })
    };
    let ((a0, a1), (b0, b1)) = (range(a), range(b));
    a.path == b.path && a0 <= b1 && b0 <= a1
}

/// `Route.path C "path" [segments] query optional`.
fn path(b: &mut Builder, schemas: &mut Gen, me: &str, r: &Route) -> Result<NodeId, Fail> {
    let mut segments = Vec::new();
    for (i, s) in r.segments.iter().enumerate() {
        let at = format!("{me}.{}[{i}]", r.constructor);
        segments.push(match s {
            Segment::Value(ty) => {
                let s = schemas.schema(b, ty, &at)?;
                call(b, "Route.segment", vec![s])
            }
            Segment::Words(cons) => {
                let pairs: Vec<NodeId> = (cons.iter())
                    .map(|c| {
                        let w = b.expr(Expr::Text(kebab(c)));
                        let c = name(b, c);
                        b.expr(Expr::Tuple(vec![w, c]))
                    })
                    .collect();
                let list = b.expr(Expr::Items(pairs));
                call(b, "Route.words", vec![list])
            }
            Segment::Nested(ty) => {
                let url = name(b, &format!("{ty}$url"));
                let unit = b.pat(Pat::Wildcard);
                let thunk = b.expr(Expr::Lambda {
                    params: vec![unit],
                    body: url,
                });
                call(b, "Route.nested", vec![thunk])
            }
        });
    }
    let query = match &r.query {
        None => name(b, "None"),
        Some(fields) => {
            let mut items = Vec::new();
            for (label, ty, optional) in fields {
                let at = format!("{me}.{}.{label}", r.constructor);
                let schema = schemas.schema(b, ty, &at)?;
                let text = b.expr(Expr::Text(label.clone()));
                let missing = b.expr(Expr::Boolean(*optional));
                items.push(call(b, "Route.query", vec![text, schema, missing]));
            }
            let list = b.expr(Expr::Items(items));
            call(b, "Some", vec![list])
        }
    };
    let c = name(b, &r.constructor);
    let p = b.expr(Expr::Text(r.path.clone()));
    let segments = b.expr(Expr::Items(segments));
    let optional = b.expr(Expr::Boolean(r.optional));
    Ok(call(b, "Route.path", vec![c, p, segments, query, optional]))
}

/// `GreenLight` as `green-light`.
pub fn kebab(name: &str) -> String {
    let mut out = String::new();
    let mut previous: Option<char> = None;
    for c in name.chars() {
        if c.is_ascii_uppercase()
            && previous.is_some_and(|p| p.is_ascii_lowercase() || p.is_ascii_digit())
        {
            out.push('-');
        }
        out.push(c.to_ascii_lowercase());
        previous = Some(c);
    }
    out
}
