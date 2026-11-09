//! The code of a checked query:
//!
//! ```text
//! with (Sql.Proxy ()) (fun px0 -> … with <$e> (fun c0 -> … with <literal> (fun l0 ->
//!   Sql.Derive.query
//!     ((fun p -> with (Sql.Derive.columns Users.table) (fun a0 -> …
//!        Sql.Derive.checked [conditions] row)) : { x : _ } -> _)
//!     { text = …; tables = […]; params = […]; columns = […] })))
//! ```
//!
//! `Sql.Derive.with x f` types `x` before `f`, so a value of the wrong type
//! is wrong where the query uses it, at the span each term stands for.
//!
//! The function of the parameters never runs: its type is the template's.
//! Each proxy names the type of a placeholder or result whose kind or cast
//! only that type says, and each use of one is `Sql.Derive.like px`, a
//! proxy built where a method takes it apart, which a release build folds.

use std::mem;

use flick_plugin::{Arena, Builder, Expr, Field, NodeId, Pat, Span, Ty};

use super::text::Piece;
use super::{At, Cx, Out, Query, W};

/// The arena of `query`, the template's parameters declared as `declared`.
pub fn arena(cx: &mut Cx, query: &Query, declared: Option<&[String]>) -> Arena {
    let whole = cx.block.whole();
    let mut b = Builder::new(whole);
    let mut e = Emit { cx, b: &mut b };
    let root = e.template(query, declared);
    b.finish(root)
}

struct Emit<'c, 'b, 'x> {
    cx: &'c mut Cx<'x>,
    b: &'b mut Builder,
}

fn field(label: &str, value: NodeId) -> Field {
    Field {
        label: label.to_owned(),
        value,
    }
}

impl Emit<'_, '_, '_> {
    fn name(&mut self, name: &str) -> NodeId {
        self.b.expr(Expr::Name(name.to_owned()))
    }

    fn apply(&mut self, function: &str, arguments: Vec<NodeId>) -> NodeId {
        let function = self.name(function);
        self.b.expr(Expr::Apply {
            function,
            arguments,
        })
    }

    fn text(&mut self, s: &str) -> NodeId {
        self.b.expr(Expr::Text(s.to_owned()))
    }

    /// `Sql.Derive.like px<n>`.
    fn like(&mut self, proxy: usize) -> NodeId {
        let px = self.name(&format!("px{proxy}"));
        self.apply("Sql.Derive.like", vec![px])
    }

    /// The method `method` of `Sql` on the type proxy `proxy` names.
    fn method(&mut self, method: &str, proxy: usize) -> NodeId {
        let like = self.like(proxy);
        self.apply(&format!("Sql.{method}"), vec![like])
    }

    fn template(&mut self, query: &Query, declared: Option<&[String]>) -> NodeId {
        // every proxy of a result whose kind only its type says
        let outputs: Vec<Option<usize>> = (query.columns.iter())
            .map(|c| (c.form.is_none() && c.known.kind().is_none()).then(|| self.cx.proxy()))
            .collect();
        let typing = self.typing(query, declared, &outputs);
        let data = self.data(query, &outputs);
        let template = self.apply("Sql.Derive.query", vec![typing, data]);
        // the proxies, captures and literals, bound before the template
        let mut bound: Vec<(String, NodeId)> = Vec::new();
        for n in 0..self.cx.proxies {
            let unit = self.b.expr(Expr::Unit);
            bound.push((format!("px{n}"), self.apply("Sql.Proxy", vec![unit])));
        }
        let holes: Vec<(String, Span)> = (self.cx.captures.iter().enumerate())
            .map(|(k, c)| (format!("c{k}"), c.inner))
            .chain((self.cx.literals.iter().enumerate()).map(|(k, l)| (format!("l{k}"), l.at)))
            .collect();
        for (var, span) in holes {
            let hole = self.hole(span);
            bound.push((var, hole));
        }
        self.bind(bound, template)
    }

    /// `with v0 (fun x0 -> with v1 (fun x1 -> … body))`.
    fn bind(&mut self, bound: Vec<(String, NodeId)>, body: NodeId) -> NodeId {
        let mut body = body;
        for (var, value) in bound.into_iter().rev() {
            let param = self.b.pat(Pat::Var(var));
            let lambda = self.b.expr(Expr::Lambda {
                params: vec![param],
                body,
            });
            body = self.apply("Sql.Derive.with", vec![value, lambda]);
        }
        body
    }

    /// The user's expression at `span`, as the compiler parses it.
    fn hole(&mut self, span: Span) -> NodeId {
        match self.cx.compiler.parse_expr(span.start, span.end) {
            Ok(hole) => self.b.expr(Expr::Hole(hole.span)),
            Err(wrong) => {
                self.cx.wrong.push(wrong);
                self.b.expr(Expr::Unit)
            }
        }
    }

    /// The function from the parameters to a row, of the template's type.
    fn typing(
        &mut self,
        query: &Query,
        declared: Option<&[String]>,
        outputs: &[Option<usize>],
    ) -> NodeId {
        let mut checks = mem::take(&mut self.cx.checks);
        // the type of each placeholder whose kind its type says
        for (i, p) in self.cx.params.iter().enumerate() {
            if p.form.is_none() {
                checks.push(W::Call("typed_by", vec![W::Proxy(p.proxy), W::Param(i)]));
            }
        }
        for (k, c) in self.cx.captures.iter().enumerate() {
            if !c.template && !c.form.as_ref().is_some_and(|f| f.via) {
                checks.push(W::Call(
                    "typed_by",
                    vec![W::Proxy(c.proxy), W::Raw(format!("c{k}"))],
                ));
            }
        }
        for (k, l) in self.cx.literals.iter().enumerate() {
            checks.push(W::Call(
                "typed_by",
                vec![W::Proxy(l.proxy), W::Raw(format!("l{k}"))],
            ));
        }
        for (c, proxy) in query.columns.iter().zip(outputs) {
            if let Some(proxy) = proxy {
                checks.push(W::Call("typed_by", vec![W::Proxy(*proxy), c.w.clone()]));
            }
        }
        let whole = self.cx.block.whole();
        let checks: Vec<NodeId> = checks.iter().map(|w| self.w(w, whole)).collect();
        let checks = self.b.expr(Expr::Items(checks));
        let row = self.row(&query.columns);
        let checked = self.apply("Sql.Derive.checked", vec![checks, row]);
        // the columns of each alias
        let tables: Vec<usize> = self.cx.vars.clone();
        let bound = (tables.iter().enumerate())
            .map(|(v, &t)| {
                let table = self.table(t);
                let columns = self.apply("Sql.Derive.columns", vec![table]);
                // its `_` tells an editor the table's columns, at its name
                self.b.at(self.cx.tables[t].at);
                let any = self.b.ty(Ty::Wildcard);
                let typed = self.b.expr(Expr::Annot {
                    expr: columns,
                    ty: any,
                });
                self.b.at(self.cx.block.whole());
                (format!("a{v}"), typed)
            })
            .collect();
        let body = self.bind(bound, checked);
        let p = self.b.pat(Pat::Var("p".into()));
        let function = self.b.expr(Expr::Lambda {
            params: vec![p],
            body,
        });
        // the parameters: those the signature declares, or those used
        let labels: Vec<String> = declared.map_or_else(
            || self.cx.params.iter().map(|p| p.name.clone()).collect(),
            <[String]>::to_vec,
        );
        let fields = (labels.iter())
            .map(|l| {
                let nullable = self.cx.params.iter().any(|p| p.name == *l && p.nullable);
                let any = self.b.ty(Ty::Wildcard);
                let ty = if nullable {
                    let option = self.b.ty(Ty::Name("Option".into()));
                    self.b.ty(Ty::Apply {
                        function: option,
                        arguments: vec![any],
                    })
                } else {
                    any
                };
                field(l, ty)
            })
            .collect();
        let input = self.b.ty(Ty::Record(fields));
        let output = self.b.ty(Ty::Wildcard);
        let ty = self.b.ty(Ty::Function { input, output });
        self.b.expr(Expr::Annot { expr: function, ty })
    }

    /// `Users.table`, as the module names the table.
    fn table(&mut self, t: usize) -> NodeId {
        let (written, at) = (self.cx.tables[t].written.clone(), self.cx.tables[t].at);
        self.b.at(at);
        let name = self.name(&format!("{written}.table"));
        self.b.at(self.cx.block.whole());
        name
    }

    /// The row: each result, in `Option` when it may be `NULL`.
    fn row(&mut self, columns: &[Out]) -> NodeId {
        if columns.is_empty() {
            return self.name("Sql.Derive.nothing");
        }
        let fields = (columns.iter())
            .map(|c| {
                let whole = self.cx.block.whole();
                let mut value = self.w(&c.w, whole);
                if c.null {
                    value = self.apply("Sql.Derive.maybe", vec![value]);
                }
                field(&c.label, value)
            })
            .collect();
        self.b.expr(Expr::Record(fields))
    }

    /// The term of `w`, its nodes at `at` unless a part says where it is.
    fn w(&mut self, w: &W, at: Span) -> NodeId {
        let node = match w {
            W::At(inner, span) => return self.w(inner, *span),
            W::Column(var, column) => {
                self.b.at(at);
                let alias = self.name(&format!("a{var}"));
                let get = self.b.expr(Expr::Get {
                    record: alias,
                    label: column.clone(),
                });
                self.apply("Sql.Derive.value", vec![get])
            }
            W::Param(i) => {
                let p = &self.cx.params[*i];
                let (name, nullable) = (p.name.clone(), p.nullable);
                self.b.at(at);
                let record = self.name("p");
                let get = self.b.expr(Expr::Get {
                    record,
                    label: name,
                });
                // its `_` tells an editor the parameter's type, at its name
                let any = self.b.ty(Ty::Wildcard);
                let typed = self.b.expr(Expr::Annot { expr: get, ty: any });
                if nullable {
                    self.apply("Sql.Derive.inner", vec![typed])
                } else {
                    typed
                }
            }
            W::Capture(k) => {
                let c = &self.cx.captures[*k];
                let (via, proxy) = (c.form.as_ref().is_some_and(|f| f.via), c.proxy);
                self.b.at(at);
                let var = self.name(&format!("c{k}"));
                if via {
                    var
                } else {
                    let px = self.name(&format!("px{proxy}"));
                    self.apply("Sql.Derive.param", vec![px, var])
                }
            }
            W::Literal(k) => {
                self.b.at(at);
                self.name(&format!("l{k}"))
            }
            W::Boolean(v) => {
                self.b.at(at);
                self.b.expr(Expr::Boolean(*v))
            }
            W::Call(name, args) => {
                let args = args.iter().map(|a| self.w(a, at)).collect();
                self.b.at(at);
                self.apply(&format!("Sql.Derive.{name}"), args)
            }
            W::Num(name, proxy, inner) => {
                let inner = self.w(inner, at);
                self.b.at(at);
                let px = self.name(&format!("px{proxy}"));
                self.apply(&format!("Sql.Derive.{name}"), vec![px, inner])
            }
            W::Fact(inner, span) => {
                let inner = self.w(inner, at);
                self.b.at(*span);
                let any = self.b.ty(Ty::Wildcard);
                self.b.at(at);
                self.b.expr(Expr::Annot {
                    expr: inner,
                    ty: any,
                })
            }
            W::Proxy(n) => {
                self.b.at(at);
                self.name(&format!("px{n}"))
            }
            W::Raw(name) => {
                self.b.at(at);
                self.name(name)
            }
            W::Unit | W::Error => {
                self.b.at(at);
                self.b.expr(Expr::Unit)
            }
        };
        self.b.at(self.cx.block.whole());
        node
    }

    /// The text, tables, placeholders and results as the runtime reads them.
    fn data(&mut self, query: &Query, outputs: &[Option<usize>]) -> NodeId {
        let text = self.sql(&query.sql.0);
        let tables: Vec<NodeId> = (0..self.cx.tables.len())
            .map(|t| {
                let table = self.table(t);
                self.apply("Sql.Derive.erase", vec![table])
            })
            .collect();
        let tables = self.b.expr(Expr::Items(tables));
        let slots = self.cx.slots.clone();
        let params: Vec<NodeId> = slots.iter().map(|at| self.slot_value(*at)).collect();
        let params = self.b.expr(Expr::Items(params));
        let columns: Vec<NodeId> = (query.columns.iter().zip(outputs))
            .map(|(c, proxy)| self.result(c, *proxy))
            .collect();
        let columns = self.b.expr(Expr::Items(columns));
        self.b.expr(Expr::Record(vec![
            field("text", text),
            field("tables", tables),
            field("params", params),
            field("columns", columns),
        ]))
    }

    /// The text of `pieces`: literals joined, and the casts of the types.
    fn sql(&mut self, pieces: &[Piece]) -> NodeId {
        let mut parts: Vec<NodeId> = Vec::new();
        let mut text = String::new();
        let flush = |e: &mut Self, text: &mut String, parts: &mut Vec<NodeId>| {
            if !text.is_empty() {
                parts.push(e.text(text));
                text.clear();
            }
        };
        for piece in pieces {
            match piece {
                Piece::Text(s) => text.push_str(s),
                Piece::Slot(n) => {
                    let placeholder = format!("${}", n + 1);
                    let cast = match self.cx.slots[*n] {
                        At::Capture(k) => {
                            let c = &self.cx.captures[k];
                            let plain = !c.template && !c.form.as_ref().is_some_and(|f| f.via);
                            plain.then_some((c.proxy, c.span))
                        }
                        _ => None,
                    };
                    match cast {
                        Some((proxy, span)) => {
                            flush(self, &mut text, &mut parts);
                            self.b.at(span);
                            parts.push(self.method("cast_open", proxy));
                            text.push_str(&placeholder);
                            flush(self, &mut text, &mut parts);
                            parts.push(self.method("cast_close", proxy));
                            self.b.at(self.cx.block.whole());
                        }
                        None => text.push_str(&placeholder),
                    }
                }
                Piece::Cast(method, proxy) => {
                    flush(self, &mut text, &mut parts);
                    parts.push(self.method(method, *proxy));
                }
            }
        }
        flush(self, &mut text, &mut parts);
        match parts[..] {
            [] => self.text(""),
            [one] => one,
            _ => self.b.expr(Expr::Concat(parts)),
        }
    }

    /// A placeholder as the runtime reads it: its name, whether it may be
    /// `NULL`, its column or kind, and the value of a capture or literal.
    fn slot_value(&mut self, at: At) -> NodeId {
        // what the type of the value decides is wrong where the value is
        let origin = match at {
            At::Param(i) => self.cx.params[i].span,
            At::Capture(k) => self.cx.captures[k].span,
            At::Literal(k) => self.cx.literals[k].at,
        };
        self.b.at(origin);
        let node = self.slot_record(at);
        self.b.at(self.cx.block.whole());
        node
    }

    fn slot_record(&mut self, at: At) -> NodeId {
        if let At::Capture(k) = at
            && self.cx.captures[k].template
        {
            // the runtime splices the template's text and values in its place
            let name = self.cx.captures[k].text.clone();
            let fields = vec![
                field("name", self.text(&name)),
                field("nullable", self.b.expr(Expr::Boolean(false))),
                field("sql_kind", self.text("template")),
                field("value", self.name(&format!("c{k}"))),
            ];
            let record = self.b.expr(Expr::Record(fields));
            return self.apply("Sql.Derive.erase", vec![record]);
        }
        let (name, nullable, form, proxy, value) = match at {
            At::Param(i) => {
                let p = &self.cx.params[i];
                (
                    p.name.clone(),
                    Some(p.nullable),
                    p.form.clone(),
                    p.proxy,
                    None,
                )
            }
            At::Capture(k) => {
                let c = &self.cx.captures[k];
                let via = c.form.as_ref().is_some_and(|f| f.via);
                (
                    c.text.clone(),
                    via.then_some(false),
                    c.form.clone(),
                    c.proxy,
                    Some(format!("c{k}")),
                )
            }
            At::Literal(k) => {
                let l = &self.cx.literals[k];
                (
                    String::new(),
                    Some(false),
                    None,
                    l.proxy,
                    Some(format!("l{k}")),
                )
            }
        };
        let mut fields = vec![field("name", self.text(&name))];
        let nullable = match nullable {
            Some(n) => self.b.expr(Expr::Boolean(n)),
            None => self.method("nullable_of", proxy),
        };
        fields.push(field("nullable", nullable));
        self.form(&mut fields, form.as_ref(), None, Some(proxy));
        if let Some(value) = value {
            let value = self.name(&value);
            fields.push(field("value", value));
        }
        let record = self.b.expr(Expr::Record(fields));
        self.apply("Sql.Derive.erase", vec![record])
    }

    /// A result as the runtime reads it.
    fn result(&mut self, c: &Out, proxy: Option<usize>) -> NodeId {
        let name = self.text(&c.label);
        let nullable = self.b.expr(Expr::Boolean(c.null));
        let mut fields = vec![field("name", name), field("nullable", nullable)];
        self.form(&mut fields, c.form.as_ref(), c.known.kind(), proxy);
        let record = self.b.expr(Expr::Record(fields));
        self.apply("Sql.Derive.erase", vec![record])
    }

    /// The column a value crosses as, or its kind.
    fn form(
        &mut self,
        fields: &mut Vec<Field>,
        form: Option<&super::Form>,
        known: Option<&str>,
        proxy: Option<usize>,
    ) {
        if let Some(form) = form {
            let table = u32::try_from(form.table).expect("a query reads fewer than 2^32 tables");
            let table = self.b.expr(Expr::Number(f64::from(table)));
            let column = self.text(&form.column);
            fields.push(field("table", table));
            fields.push(field("column", column));
            return;
        }
        let kind = match (known, proxy) {
            (Some(kind), _) => self.text(kind),
            (None, Some(proxy)) => self.method("kind_of", proxy),
            (None, None) => self.text("text"),
        };
        fields.push(field("sql_kind", kind));
    }
}
