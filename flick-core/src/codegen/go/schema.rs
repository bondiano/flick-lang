//! Schema witnesses: a `deriving Schema` item is a value of the runtime's
//! schemas, or a function of the witnesses of its type's parameters.

use flick_pretty::{Document, Documentable, docvec};

use super::items::Decls;
use super::layout::{call, closure, list, single, statement};
use super::{Gen, Shape, names};
use crate::source::Span;
use crate::types::{Constructor, Rep, SchemaItem, SchemaPlan, companion_name};

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
        if item.url.is_some() || item.form.is_some() {
            self.unsupported(span, "a schema deriving `Url` or `Form`");
            return;
        }
        let name = companion_name(&item.ty, "schema");
        let body = self.plan(&item.plan, span);
        if item.witnesses == 0 {
            let var = a.alloc_str(&names::var(self.module, &name));
            decls.push((docvec![a, "var ", var, " flickrt.Value"], false));
            init.push(statement(a, docvec![a, var, " = ", body]));
        } else {
            let ws: Vec<String> = (0..item.witnesses).map(|i| format!("w{i}")).collect();
            let decl = docvec![
                a,
                "func ",
                names::func(self.module, &name),
                "(",
                ws.join(", "),
                " flickrt.Value) flickrt.Value ",
                single(a, docvec![a, "return ", body])
            ];
            decls.push((decl, false));
        }
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
                call(a, "flickrt.Apply", [docvec![a, "w", *i], p])
            }
            SchemaPlan::Constructor(name) => a.text(match name.as_str() {
                "List" => "flickrt.Fn(flickrt.SchemaList)",
                "Option" => "flickrt.Fn(flickrt.SchemaOption)",
                _ => "flickrt.Fn(flickrt.SchemaRedacted)",
            }),
            SchemaPlan::Named {
                module,
                ty,
                args,
                lazy,
            } => {
                let name = companion_name(ty, "schema");
                self.reads(module);
                let args: Vec<Document<'a>> = args.iter().map(|p| self.plan(p, span)).collect();
                let schema = match self.shape_of(module, &name) {
                    Shape::Func { .. } => call(a, names::func(module, &name), args),
                    Shape::Var => names::var(module, &name).to_doc(a),
                };
                if *lazy {
                    let get = closure(a, "func() flickrt.Value", [docvec![a, "return ", schema]]);
                    call(a, "flickrt.SchemaLazy", [self.string(ty), get])
                } else {
                    schema
                }
            }
            SchemaPlan::Refined { .. } => {
                self.unsupported(span, "a refined type");
                a.text("flickrt.SchemaUnknown")
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
