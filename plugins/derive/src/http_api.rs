//! `deriving HttpApi`: a record whose fields are endpoints is a group of an
//! API. `T$endpoints` is the record of its endpoints, each built with
//! `HttpApi.Derive.at` from the `HttpApi` functions a program would call,
//! its codecs `wire_codec`s the compiler chooses; `T$api` is the group of
//! them or, with `Authed` and `MaybeAuthed` endpoints, a function of the
//! security scheme, the service and its two checks to the group. The types
//! of both are written from the user's, so the checker finds what does not
//! fit: a missing `Problem` instance, codec or path parameter.

use flick_plugin::{Block, Builder, Decl, DeriveTarget, Expr, Field, NodeId, Pat, Span, Ty};

use crate::endpoint::{self, Endpoint, Guard, PARTS, Path, Refusal, not_a_record};
use crate::schema::{call, name};

/// `T$endpoints` and `T$api`, with their signatures.
pub fn declarations(b: &mut Builder, target: &DeriveTarget) -> Result<NodeId, Refusal> {
    let me = target
        .name
        .rsplit('.')
        .next()
        .unwrap_or(&target.name)
        .to_owned();
    let whole = target.decl.whole();
    let fail = |why: &str| -> Refusal { (whole, format!("`{me}`: {why}")) };
    if !target.local {
        return Err(fail("`HttpApi` is derived where the record is declared"));
    }
    if !target.params.is_empty() {
        return Err(fail("a record of endpoints has no parameters"));
    }
    let record = target.alias.ok_or_else(|| (whole, not_a_record(&me)))?;
    let endpoints = endpoint::read(&target.decl, &me, record)?;
    let guarded = endpoints.iter().filter(|e| e.guard != Guard::None).count();
    if guarded != 0 && guarded != endpoints.len() {
        return Err(fail(
            "every endpoint of a group with `Authed` endpoints is `Authed` or `MaybeAuthed`: put the others in a group of their own",
        ));
    }
    let g = Gen {
        me: &me,
        block: &target.decl,
    };
    let mut decls = g.endpoints(b, &endpoints);
    decls.extend(if guarded == 0 {
        g.group(b, &endpoints)
    } else {
        g.secured(b, &endpoints)
    });
    Ok(b.group(decls))
}

struct Gen<'a> {
    me: &'a str,
    block: &'a Block,
}

impl Gen<'_> {
    /// `T$endpoints : { l : HttpApi.Endpoint "l" (Handler …) }` and its value.
    fn endpoints(&self, b: &mut Builder, endpoints: &[Endpoint]) -> Vec<NodeId> {
        let types = (endpoints.iter())
            .map(|e| {
                let handler = handler(b, e, None);
                let label = b.ty(Ty::Text(e.label.clone()));
                let value = applied(b, "HttpApi.Endpoint", vec![label, handler]);
                Field {
                    label: e.label.clone(),
                    value,
                }
            })
            .collect();
        let ty = b.ty(Ty::Record(types));
        let whole = self.block.whole();
        let fields = (endpoints.iter())
            .map(|e| {
                // what does not fit in its code is on its field
                b.at(e.at);
                let value = endpoint(b, e);
                b.at(whole);
                Field {
                    label: e.label.clone(),
                    value,
                }
            })
            .collect();
        let body = b.expr(Expr::Record(fields));
        self.value(b, "endpoints", ty, Vec::new(), body)
    }

    /// `T$api : HttpApi.Group "t" r { … }`, adding each endpoint.
    fn group(&self, b: &mut Builder, endpoints: &[Endpoint]) -> Vec<NodeId> {
        let ty = self.group_type(b, endpoints, false);
        let body = self.added(b, endpoints, |_, _, endpoint| endpoint);
        self.value(b, "api", ty, Vec::new(), body)
    }

    /// `T$api security tag check maybe`, the group whose endpoints the
    /// checks guard.
    fn secured(&self, b: &mut Builder, endpoints: &[Endpoint]) -> Vec<NodeId> {
        let first = &endpoints[0];
        let headers = part(b, first, "headers");
        // the errors every guarded endpoint declares are what the check may fail with
        let shown = |e: &Endpoint| -> Vec<String> {
            let text = |s: Span| self.block.text_of(s).split_whitespace().collect::<String>();
            e.errors.iter().map(|&s| text(s)).collect()
        };
        let all: Vec<Vec<String>> = endpoints.iter().map(shown).collect();
        let checked: Vec<NodeId> = (first.errors.iter().zip(&all[0]))
            .filter(|(_, t)| all.iter().all(|row| row.contains(t)))
            .map(|(&s, _)| b.ty(Ty::Hole(s)))
            .collect();
        let checked = b.ty(Ty::Row(checked));
        let group = self.group_type(b, endpoints, true);
        let (a, r) = (b.ty(Ty::Name("a".into())), b.ty(Ty::Name("r".into())));
        let ty = applied(
            b,
            "HttpApi.Derive.Secured",
            vec![a, r, headers, checked, group],
        );
        let guarded = self.added(b, endpoints, |b, e, endpoint| {
            let (with, check) = match e.guard {
                Guard::Optional => ("HttpApi.Derive.with_optional_header_auth", "maybe"),
                _ => ("HttpApi.Derive.with_header_auth", "check"),
            };
            // its own function, whose errors are some of the endpoint's
            let request = b.pat(Pat::Var("request".into()));
            let passed = name(b, "request");
            let body = call(b, check, vec![passed]);
            let check = b.expr(Expr::Lambda {
                params: vec![request],
                body,
            });
            let (security, tag) = (name(b, "security"), name(b, "tag"));
            call(b, with, vec![security, tag, check, endpoint])
        });
        let tag = name(b, "tag");
        let body = call(b, "HttpApi.authenticated", vec![tag, guarded]);
        let params = ["security", "tag", "check", "maybe"]
            .map(|p| b.pat(Pat::Var(p.into())))
            .to_vec();
        self.value(b, "api", ty, params, body)
    }

    /// `HttpApi.Group "t" r { l : Handler … }`, its handlers guarded ones.
    fn group_type(&self, b: &mut Builder, endpoints: &[Endpoint], guarded: bool) -> NodeId {
        let fields = (endpoints.iter())
            .map(|e| Field {
                label: e.label.clone(),
                value: handler(b, e, guarded.then_some("a")),
            })
            .collect();
        let handlers = b.ty(Ty::Record(fields));
        let label = b.ty(Ty::Text(snake(self.me)));
        let r = b.ty(Ty::Name("r".into()));
        applied(b, "HttpApi.Group", vec![label, r, handlers])
    }

    /// `HttpApi.add (T$endpoints.lN) (… (HttpApi.group "t"))`, each endpoint
    /// as `wrap` makes it.
    fn added(
        &self,
        b: &mut Builder,
        endpoints: &[Endpoint],
        wrap: impl Fn(&mut Builder, &Endpoint, NodeId) -> NodeId,
    ) -> NodeId {
        let group = label(b, &snake(self.me));
        let mut made = call(b, "HttpApi.group", vec![group]);
        for e in endpoints {
            let record = name(b, &format!("{}.endpoints", self.me));
            let endpoint = b.expr(Expr::Get {
                record,
                label: e.label.clone(),
            });
            let endpoint = wrap(b, e, endpoint);
            made = call(b, "HttpApi.add", vec![endpoint, made]);
        }
        made
    }

    /// `T.member : ty` and `let T.member params = body`.
    fn value(
        &self,
        b: &mut Builder,
        member: &str,
        ty: NodeId,
        params: Vec<NodeId>,
        body: NodeId,
    ) -> Vec<NodeId> {
        let name = format!("{}.{member}", self.me);
        let signature = b.decl(Decl::Signature {
            name: name.clone(),
            ty,
        });
        let value = b.decl(Decl::Let {
            name,
            params,
            ty: None,
            body,
            rec: false,
        });
        vec![signature, value]
    }
}

/// `Handler p q h b r {E…} s`, or `Guarded p q h b a r {E…} s`.
fn handler(b: &mut Builder, e: &Endpoint, guard: Option<&str>) -> NodeId {
    let mut args: Vec<NodeId> = PARTS.iter().map(|p| part(b, e, p)).collect();
    let alias = match guard {
        Some(a) => {
            args.push(b.ty(Ty::Name(a.into())));
            "HttpApi.Derive.Guarded"
        }
        None => "HttpApi.Derive.Handler",
    };
    args.push(b.ty(Ty::Name("r".into())));
    let errors = e.errors.iter().map(|&s| b.ty(Ty::Hole(s))).collect();
    args.push(b.ty(Ty::Row(errors)));
    args.push(match e.success {
        Some((s, _)) => b.ty(Ty::Hole(s)),
        None => b.ty(Ty::Name("Unit".into())),
    });
    applied(b, alias, args)
}

/// The type of a part of the request, `Unit` when it has none.
fn part(b: &mut Builder, e: &Endpoint, name: &str) -> NodeId {
    match e.part(name) {
        Some(s) => b.ty(Ty::Hole(s)),
        None => b.ty(Ty::Name("Unit".into())),
    }
}

/// `HttpApi.Derive.at M path label (fun endpoint -> with_errors … (… endpoint))`.
fn endpoint(b: &mut Builder, e: &Endpoint) -> NodeId {
    let mut made = name(b, "endpoint");
    for &(p, span) in &e.parts {
        let codec = wire_codec(b, span);
        made = call(b, &format!("HttpApi.with_{p}"), vec![codec, made]);
    }
    if let Some((span, created)) = e.success {
        let codec = wire_codec(b, span);
        let with = if created { "created" } else { "ok" };
        made = call(b, &format!("HttpApi.Derive.{with}"), vec![codec, made]);
    }
    for &error in &e.errors {
        // each case with its own type, which names its `Problem` instance
        let errors = name(b, "HttpApi.errors");
        let case = call(b, "HttpApi.Derive.problem_case", vec![errors]);
        let error = b.ty(Ty::Hole(error));
        let row = b.ty(Ty::Row(vec![error]));
        let ty = applied(b, "HttpApi.ErrorCases", vec![row]);
        let case = b.expr(Expr::Annot { expr: case, ty });
        made = call(b, "HttpApi.with_errors", vec![case, made]);
    }
    let param = b.pat(Pat::Var("endpoint".into()));
    let parts = b.expr(Expr::Lambda {
        params: vec![param],
        body: made,
    });
    let method = name(b, &format!("HttpApi.{}", e.method));
    let path = match &e.path {
        Path::Text(text) => b.ty(Ty::Text(text.clone())),
        Path::Route(span) => b.ty(Ty::Hole(*span)),
    };
    let path = labelled(b, path);
    let label = label(b, &e.label);
    call(b, "HttpApi.Derive.at", vec![method, path, label, parts])
}

/// `(HttpApi.Derive.wire_codec : PureCodec Json T)`.
fn wire_codec(b: &mut Builder, of: Span) -> NodeId {
    let expr = name(b, "HttpApi.Derive.wire_codec");
    let json = b.ty(Ty::Name("Json".into()));
    let of = b.ty(Ty::Hole(of));
    let ty = applied(b, "PureCodec", vec![json, of]);
    b.expr(Expr::Annot { expr, ty })
}

/// `(Record.label : Record.Label "text")`.
fn label(b: &mut Builder, text: &str) -> NodeId {
    let symbol = b.ty(Ty::Text(text.to_owned()));
    labelled(b, symbol)
}

fn labelled(b: &mut Builder, symbol: NodeId) -> NodeId {
    let expr = name(b, "Record.label");
    let ty = applied(b, "Record.Label", vec![symbol]);
    b.expr(Expr::Annot { expr, ty })
}

fn applied(b: &mut Builder, f: &str, arguments: Vec<NodeId>) -> NodeId {
    let function = b.ty(Ty::Name(f.to_owned()));
    b.ty(Ty::Apply {
        function,
        arguments,
    })
}

/// `UpdateUser` → `update_user`, the name of its group.
fn snake(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}
