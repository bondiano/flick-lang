//! Schema witnesses: a `deriving Schema` item is a value of the runtime's
//! schemas, or a function of the witnesses of its type's parameters.

use super::{Gen, Shape, names};
use crate::source::Span;
use crate::types::{Constructor, Rep, SchemaItem, SchemaPlan, companion_name};

impl Gen<'_> {
    /// `T$schema`: its declaration, and the statement of the module's init
    /// function that assigns it, for a type without parameters.
    pub(super) fn schema(
        &mut self,
        item: &SchemaItem,
        span: Span,
        decls: &mut Vec<String>,
        init: &mut Vec<String>,
    ) {
        if item.url.is_some() || item.form.is_some() {
            self.unsupported(span, "a schema deriving `Url` or `Form`");
            return;
        }
        let name = companion_name(&item.ty, "schema");
        let body = self.plan(&item.plan, span);
        if item.witnesses == 0 {
            let var = names::var(self.module, &name);
            decls.push(format!("var {var} flickrt.Value"));
            init.push(format!("{var} = {body}"));
        } else {
            let ws: Vec<String> = (0..item.witnesses).map(|i| format!("w{i}")).collect();
            decls.push(format!(
                "func {}({} flickrt.Value) flickrt.Value {{\n\treturn {body}\n}}",
                names::func(self.module, &name),
                ws.join(", ")
            ));
        }
    }

    fn plan(&mut self, plan: &SchemaPlan, span: Span) -> String {
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
                format!("flickrt.{s}")
            }
            SchemaPlan::List(p) => format!("flickrt.SchemaList({})", self.plan(p, span)),
            SchemaPlan::Option(p) => format!("flickrt.SchemaOption({})", self.plan(p, span)),
            SchemaPlan::Redacted(p) => format!("flickrt.SchemaRedacted({})", self.plan(p, span)),
            SchemaPlan::Result(e, a) => {
                let (e, a) = (self.plan(e, span), self.plan(a, span));
                format!("flickrt.SchemaResult({e}, {a})")
            }
            SchemaPlan::Tuple(items) => {
                let items: Vec<String> = items.iter().map(|p| self.plan(p, span)).collect();
                format!("flickrt.SchemaTuple({})", items.join(", "))
            }
            SchemaPlan::Record(fields) => {
                let fields: Vec<String> = fields
                    .iter()
                    .map(|(l, p)| format!("{}, {}", names::string(l), self.plan(p, span)))
                    .collect();
                format!("flickrt.SchemaRecord({})", fields.join(", "))
            }
            SchemaPlan::Adt { name, variants } => self.adt(name, variants, span),
            SchemaPlan::Param(i) => format!("w{i}"),
            SchemaPlan::Applied(i, p) => format!("flickrt.Apply(w{i}, {})", self.plan(p, span)),
            SchemaPlan::Constructor(name) => match name.as_str() {
                "List" => "flickrt.Fn(flickrt.SchemaList)".to_owned(),
                "Option" => "flickrt.Fn(flickrt.SchemaOption)".to_owned(),
                _ => "flickrt.Fn(flickrt.SchemaRedacted)".to_owned(),
            },
            SchemaPlan::Named {
                module,
                ty,
                args,
                lazy,
            } => {
                let name = companion_name(ty, "schema");
                self.reads(module);
                let args: Vec<String> = args.iter().map(|p| self.plan(p, span)).collect();
                let call = match self.shape_of(module, &name) {
                    Shape::Func { .. } => {
                        format!("{}({})", names::func(module, &name), args.join(", "))
                    }
                    Shape::Var => names::var(module, &name),
                };
                if *lazy {
                    format!(
                        "flickrt.SchemaLazy({}, func() flickrt.Value {{ return {call} }})",
                        names::string(ty)
                    )
                } else {
                    call
                }
            }
            SchemaPlan::Refined { .. } => {
                self.unsupported(span, "a refined type");
                "flickrt.SchemaUnknown".to_owned()
            }
        }
    }

    fn adt(&mut self, name: &str, variants: &[(String, Vec<SchemaPlan>)], span: Span) -> String {
        let variants: Vec<String> = variants
            .iter()
            .map(|(c, fields)| {
                let constructor = Constructor {
                    module: self.module.to_owned(),
                    ty: name.to_owned(),
                    name: c.clone(),
                    arity: fields.len(),
                    rep: Rep::Tagged,
                };
                let tag = self.tag(&constructor);
                let fields: Vec<String> = fields.iter().map(|p| self.plan(p, span)).collect();
                format!(
                    "{{{}, {}, []flickrt.Value{{{}}}}}",
                    names::string(&tag),
                    names::string(c),
                    fields.join(", ")
                )
            })
            .collect();
        format!(
            "flickrt.SchemaAdt({}, []flickrt.Variant{{{}}})",
            names::string(name),
            variants.join(", ")
        )
    }
}
