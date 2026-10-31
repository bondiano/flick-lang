//! Schema witnesses: a `deriving Schema` item is a value of the runtime's
//! schemas, or a function of the witnesses of its type's parameters.

use std::iter;

use flick_pretty::{Document, Documentable, docvec};

use super::items::Decls;
use super::layout::{call, closure, list, signature, single, statement};
use super::{Gen, Shape, names};
use crate::source::Span;
use crate::types::{Constructor, Rep, SchemaItem, SchemaPlan, UrlRoute, companion_name};

impl<'a> Gen<'a> {
    /// `T$schema`: its declaration, and the statement of the module's init
    /// function that assigns it, for a type without parameters.
    pub(super) fn schema(
        &mut self,
        item: &SchemaItem,
        span: Span,
        decls: &mut Decls<'a>,
        init: &mut Vec<Document<'a>>,
    ) {
        let a = self.a;
        if item.form.is_some() {
            self.unsupported(span, "a schema deriving `Form`");
            return;
        }
        let name = companion_name(&item.ty, "schema");
        let context = (item.witnesses > 0).then(|| self.context.replace("fb"));
        let body = self.plan(&item.plan, span);
        if let Some(context) = context {
            self.context = context;
        }
        if item.witnesses == 0 {
            let var = a.alloc_str(&names::var(self.module, &name));
            decls.push((docvec![a, "var ", var, " flickrt.Value"], false));
            init.push(statement(a, docvec![a, var, " = ", body]));
            // a refined type's `T.json`: its schema's canonical codec
            if matches!(item.plan, SchemaPlan::Refined { .. }) {
                let json = a.alloc_str(&names::var(self.module, &companion_name(&item.ty, "json")));
                decls.push((docvec![a, "var ", json, " flickrt.Value"], false));
                let codec = call(a, "flickrt.SchemaJsonCodec", [docvec![a, var]]);
                init.push(statement(a, docvec![a, json, " = ", codec]));
            }
            // a route type has no parameters: the type checker refuses a generic one
            if let Some(routes) = &item.url {
                let url = a.alloc_str(&names::var(self.module, &companion_name(&item.ty, "url")));
                decls.push((docvec![a, "var ", url, " flickrt.Value"], false));
                let codec = self.url(&item.ty, routes, var, span);
                init.push(statement(a, docvec![a, url, " = ", codec]));
            }
        } else {
            let ws: Vec<String> = (0..item.witnesses).map(|i| format!("w{i}")).collect();
            let decl = docvec![
                a,
                "func ",
                names::func(self.module, &name),
                "(",
                signature(&ws),
                ") flickrt.Value ",
                single(a, docvec![a, "return ", body])
            ];
            decls.push((decl, false));
        }
    }

    /// `T$url`: per constructor its tag, path, segment witnesses and query;
    /// a nil query is a constructor without a final record field.
    fn url(&mut self, ty: &str, routes: &[UrlRoute], schema: &'a str, span: Span) -> Document<'a> {
        let a = self.a;
        let routes: Vec<Document<'a>> = routes
            .iter()
            .map(|r| {
                let constructor = Constructor {
                    module: self.module.to_owned(),
                    ty: ty.to_owned(),
                    name: r.constructor.clone(),
                    arity: r.segments.len() + usize::from(r.query.is_some()),
                    rep: Rep::Tagged,
                };
                let tag = self.string(&self.tag(&constructor));
                let segments: Vec<Document<'a>> =
                    r.segments.iter().map(|p| self.plan(p, span)).collect();
                let segments = list(a, "[]flickrt.Value{", segments, "}");
                let query = match &r.query {
                    None => a.text("nil"),
                    Some(fields) => {
                        let fields: Vec<Document<'a>> = fields
                            .iter()
                            .map(|(l, p, optional)| {
                                let optional = if *optional { "true" } else { "false" };
                                let p = self.plan(p, span);
                                list(a, "{", [self.string(l), p, a.text(optional)], "}")
                            })
                            .collect();
                        list(a, "[]flickrt.QueryField{", fields, "}")
                    }
                };
                list(a, "{", [tag, self.string(&r.path), segments, query], "}")
            })
            .collect();
        let routes = list(a, "[]flickrt.Route{", routes, "}");
        call(a, "flickrt.RouteCodec", [a.text(schema), routes])
    }

    fn plan(&mut self, plan: &SchemaPlan, span: Span) -> Document<'a> {
        let a = self.a;
        match plan {
            SchemaPlan::Prim(name) => {
                let s = match name.as_str() {
                    "Float" => "SchemaNumber",
                    "Int" => "SchemaInt",
                    "String" => "SchemaString",
                    "Bool" => "SchemaBool",
                    "BigInt" => "SchemaBigint",
                    "Unit" => "SchemaUnit",
                    "Never" => "SchemaNever",
                    _ => "SchemaUnknown",
                };
                docvec![a, "flickrt.", s]
            }
            SchemaPlan::List(p) => call(a, "flickrt.SchemaList", [self.plan(p, span)]),
            SchemaPlan::Option(p) => call(a, "flickrt.SchemaOption", [self.plan(p, span)]),
            SchemaPlan::Redacted(p) => call(a, "flickrt.SchemaRedacted", [self.plan(p, span)]),
            SchemaPlan::Result(e, ok) => {
                let (e, ok) = (self.plan(e, span), self.plan(ok, span));
                call(a, "flickrt.SchemaResult", [e, ok])
            }
            SchemaPlan::Tuple(items) => {
                let items: Vec<Document<'a>> = items.iter().map(|p| self.plan(p, span)).collect();
                call(a, "flickrt.SchemaTuple", items)
            }
            SchemaPlan::Record(fields) => {
                let fields: Vec<Document<'a>> = fields
                    .iter()
                    .map(|(l, p)| docvec![a, self.string(l), ", ", self.plan(p, span)])
                    .collect();
                call(a, "flickrt.SchemaRecord", fields)
            }
            SchemaPlan::Adt { name, variants } => self.adt(name, variants, span),
            SchemaPlan::Param(i) => docvec![a, "w", *i],
            SchemaPlan::Applied(i, p) => {
                let p = self.plan(p, span);
                call(a, "flickrt.Apply", [self.context(), docvec![a, "w", *i], p])
            }
            SchemaPlan::Constructor(name) => {
                let constructor = match name.as_str() {
                    "List" => "flickrt.SchemaList",
                    "Option" => "flickrt.SchemaOption",
                    _ => "flickrt.SchemaRedacted",
                };
                let make = call(a, constructor, [a.text("w")]);
                let get = closure(
                    a,
                    "func(_ *flickrt.Fiber, w flickrt.Value) flickrt.Value",
                    [docvec![a, "return ", make]],
                );
                call(a, "flickrt.Fn", [get])
            }
            SchemaPlan::Named {
                module,
                ty,
                args,
                lazy,
            } => {
                let name = companion_name(ty, "schema");
                self.reads(module);
                // Lazy schema metadata is immutable; never capture its creator's fiber.
                let context = lazy.then(|| self.context.take());
                let args: Vec<Document<'a>> = args.iter().map(|p| self.plan(p, span)).collect();
                let schema = match self.shape_of(module, &name) {
                    Shape::Func { .. } => call(
                        a,
                        names::func(module, &name),
                        iter::once(self.context()).chain(args),
                    ),
                    Shape::Var => names::var(module, &name).to_doc(a),
                };
                if let Some(context) = context {
                    self.context = context;
                }
                if *lazy {
                    let get = closure(a, "func() flickrt.Value", [docvec![a, "return ", schema]]);
                    call(a, "flickrt.SchemaLazy", [self.string(ty), get])
                } else {
                    schema
                }
            }
            SchemaPlan::Refined { name, base, known } => {
                let base = self.plan(base, span);
                let check = names::var(self.module, &companion_name(name, "check"));
                let known = self.string(known.as_deref().unwrap_or(""));
                call(
                    a,
                    "flickrt.SchemaRefined",
                    [self.string(name), base, a.text(a.alloc_str(&check)), known],
                )
            }
        }
    }

    fn adt(
        &mut self,
        name: &str,
        variants: &[(String, Vec<SchemaPlan>)],
        span: Span,
    ) -> Document<'a> {
        let a = self.a;
        let variants: Vec<Document<'a>> = variants
            .iter()
            .map(|(c, fields)| {
                let constructor = Constructor {
                    module: self.module.to_owned(),
                    ty: name.to_owned(),
                    name: c.clone(),
                    arity: fields.len(),
                    rep: Rep::Tagged,
                };
                let tag = self.string(&self.tag(&constructor));
                let fields: Vec<Document<'a>> = fields.iter().map(|p| self.plan(p, span)).collect();
                let fields = list(a, "[]flickrt.Value{", fields, "}");
                list(a, "{", [tag, self.string(c), fields], "}")
            })
            .collect();
        let variants = list(a, "[]flickrt.Variant{", variants, "}");
        call(a, "flickrt.SchemaAdt", [self.string(name), variants])
    }
}
