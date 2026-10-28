//! Refined types, schema witnesses and `codec` blocks: runtime calls that
//! build the inert schema and codec trees.

use super::{Gen, names};
use crate::ir::Expr;
use crate::types::{Constructor, Rep, SchemaItem, SchemaPlan, UrlRoute, companion_name};

impl Gen<'_> {
    /// `T$check`, `T$parse` and `T$value` of a refined type.
    pub(super) fn refined(&mut self, name: &str, check: &Expr) -> String {
        let (mut stmts, value) = self.block(|g| g.function(&[], check, None));
        let check_name = companion_name(name, "check");
        stmts.push(self.export(&check_name, &value));
        let parse = format!(
            "{}({}, {check_name})",
            self.rt("refinedParse"),
            names::string(name)
        );
        stmts.push(self.export(&companion_name(name, "parse"), &parse));
        let identity = format!("{} $x", self.arrow("$x"));
        stmts.push(self.export(&companion_name(name, "value"), &identity));
        stmts.join("\n")
    }

    /// `T$schema`, a function of the witnesses of its parameters, and the
    /// `T$json` of a refined type.
    pub(super) fn schema(&mut self, item: &SchemaItem) -> String {
        let mut body = self.plan(&item.plan);
        for i in (0..item.witnesses).rev() {
            body = format!("{} {body}", self.arrow(&format!("$w{i}")));
        }
        let name = companion_name(&item.ty, "schema");
        let mut out = vec![self.export(&name, &body)];
        if matches!(item.plan, SchemaPlan::Refined { .. }) {
            let json = format!("{}({name})", self.rt("schemaJsonCodec"));
            out.push(self.export(&companion_name(&item.ty, "json"), &json));
        }
        if let Some(routes) = &item.url {
            let url = self.url(&name, &item.ty, routes);
            out.push(self.export(&companion_name(&item.ty, "url"), &url));
        }
        if let Some(fields) = &item.form {
            let specs: Vec<String> = fields
                .iter()
                .map(|(l, p, optional)| {
                    format!("[{}, {}, {optional}]", names::string(l), self.plan(p))
                })
                .collect();
            let form = format!(
                "{}({name}, {}, [{}])",
                self.rt("formSpec"),
                names::string(&item.ty),
                specs.join(", ")
            );
            out.push(self.export(&companion_name(&item.ty, "form"), &form));
            // a field is its label at run time
            let labels: Vec<String> = fields
                .iter()
                .map(|(l, ..)| format!("{}: {}", names::string(l), names::string(l)))
                .collect();
            let record = format!("{{ {} }}", labels.join(", "));
            out.push(self.export(&companion_name(&item.ty, "fields"), &record));
        }
        out.join("\n")
    }

    /// `T$url`: per constructor its tag, path, segment witnesses and query.
    fn url(&mut self, schema: &str, ty: &str, routes: &[UrlRoute]) -> String {
        let package = (self.options.package)(self.module);
        let routes: Vec<String> = routes
            .iter()
            .map(|r| {
                let constructor = Constructor {
                    module: self.module.to_owned(),
                    ty: ty.to_owned(),
                    name: r.constructor.clone(),
                    arity: r.segments.len() + usize::from(r.query.is_some()),
                    rep: Rep::Tagged,
                };
                let tag = names::tag(&constructor, package.as_deref());
                let segments: Vec<String> = r.segments.iter().map(|p| self.plan(p)).collect();
                let query = r.query.as_ref().map_or("null".to_owned(), |query| {
                    let fields: Vec<String> = query
                        .iter()
                        .map(|(l, p, optional)| {
                            format!("[{}, {}, {optional}]", names::string(l), self.plan(p))
                        })
                        .collect();
                    format!("[{}]", fields.join(", "))
                });
                format!(
                    "[{}, {}, [{}], {query}]",
                    names::string(&tag),
                    names::string(&r.path),
                    segments.join(", ")
                )
            })
            .collect();
        format!(
            "{}({schema}, [{}])",
            self.rt("routeCodec"),
            routes.join(", ")
        )
    }

    fn plan(&mut self, plan: &SchemaPlan) -> String {
        let node =
            |g: &mut Self, f: &str, args: &[String]| format!("{}({})", g.rt(f), args.join(", "));
        match plan {
            SchemaPlan::Prim(name) => {
                let f = match name.as_str() {
                    "Float" => "schemaNumber",
                    "Int" => "schemaInt",
                    "String" => "schemaString",
                    "Bool" => "schemaBool",
                    "BigInt" => "schemaBigint",
                    "Unit" => "schemaUnit",
                    "Never" => "schemaNever",
                    _ => "schemaUnknown",
                };
                self.rt(f)
            }
            SchemaPlan::List(p) => {
                let p = self.plan(p);
                node(self, "schemaList", &[p])
            }
            SchemaPlan::Option(p) => {
                let p = self.plan(p);
                node(self, "schemaOption", &[p])
            }
            SchemaPlan::Redacted(p) => {
                let p = self.plan(p);
                node(self, "schemaRedacted", &[p])
            }
            SchemaPlan::Result(e, a) => {
                let (e, a) = (self.plan(e), self.plan(a));
                node(self, "schemaResult", &[e, a])
            }
            SchemaPlan::Tuple(items) => {
                let items: Vec<String> = items.iter().map(|p| self.plan(p)).collect();
                node(self, "schemaTuple", &[format!("[{}]", items.join(", "))])
            }
            SchemaPlan::Record(fields) => {
                let fields: Vec<String> = fields
                    .iter()
                    .map(|(l, p)| format!("[{}, {}]", names::string(l), self.plan(p)))
                    .collect();
                node(self, "schemaRecord", &[format!("[{}]", fields.join(", "))])
            }
            SchemaPlan::Adt { name, variants } => self.adt(name, variants),
            SchemaPlan::Param(i) => format!("$w{i}"),
            SchemaPlan::Applied(i, p) => {
                let p = self.plan(p);
                format!("$w{i}({p})")
            }
            SchemaPlan::Constructor(name) => match name.as_str() {
                "List" => self.rt("schemaList"),
                "Option" => self.rt("schemaOption"),
                _ => self.rt("schemaRedacted"),
            },
            SchemaPlan::Named {
                module,
                ty: named,
                args,
                lazy,
            } => {
                let mut call = self.global(module, &companion_name(named, "schema"));
                for a in args {
                    let a = self.plan(a);
                    call = format!("{call}({a})");
                }
                if *lazy {
                    let get = format!("{} {call}", self.arrow(""));
                    node(self, "schemaLazy", &[names::string(named), get])
                } else {
                    call
                }
            }
            SchemaPlan::Refined { name, base, known } => {
                let base = self.plan(base);
                let known = known.as_deref().map_or("null".to_owned(), names::string);
                let check = companion_name(name, "check");
                node(
                    self,
                    "refinedSchema",
                    &[names::string(name), base, check, known],
                )
            }
        }
    }

    fn adt(&mut self, name: &str, variants: &[(String, Vec<SchemaPlan>)]) -> String {
        let package = (self.options.package)(self.module);
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
                let tag = names::tag(&constructor, package.as_deref());
                let fields: Vec<String> = fields.iter().map(|p| self.plan(p)).collect();
                format!(
                    "[{}, {}, [{}]]",
                    names::string(&tag),
                    names::string(c),
                    fields.join(", ")
                )
            })
            .collect();
        let adt = self.rt("schemaAdt");
        format!("{adt}({}, [{}])", names::string(name), variants.join(", "))
    }

    /// A `codec` block: its fields, evaluated in order.
    pub(super) fn codec_block(&mut self, name: &str, fields: &[(String, Expr)]) -> String {
        let values: Vec<&Expr> = fields.iter().map(|(_, v)| v).collect();
        let values = self.ordered(&values);
        let entries: Vec<String> = fields
            .iter()
            .zip(values)
            .map(|((l, _), v)| format!("[{}, {v}]", names::string(l)))
            .collect();
        format!(
            "{}({}, [{}])",
            self.rt("codecBlock"),
            names::string(name),
            entries.join(", ")
        )
    }
}

/// The JS names of a schema item and what it uses of its module.
pub fn schema_names(item: &SchemaItem) -> Vec<String> {
    let mut out = vec![companion_name(&item.ty, "schema")];
    if matches!(item.plan, SchemaPlan::Refined { .. }) {
        out.push(companion_name(&item.ty, "json"));
    }
    if item.url.is_some() {
        out.push(companion_name(&item.ty, "url"));
    }
    if item.form.is_some() {
        out.push(companion_name(&item.ty, "form"));
        out.push(companion_name(&item.ty, "fields"));
    }
    out
}

/// What a plan refers to of this module, for the order of definitions.
pub fn plan_uses(plan: &SchemaPlan, module: &str, out: &mut Vec<String>) {
    match plan {
        SchemaPlan::List(p)
        | SchemaPlan::Option(p)
        | SchemaPlan::Redacted(p)
        | SchemaPlan::Applied(_, p) => {
            plan_uses(p, module, out);
        }
        SchemaPlan::Result(a, b) => {
            plan_uses(a, module, out);
            plan_uses(b, module, out);
        }
        SchemaPlan::Tuple(items) => items.iter().for_each(|p| plan_uses(p, module, out)),
        SchemaPlan::Record(fields) => fields.iter().for_each(|(_, p)| plan_uses(p, module, out)),
        SchemaPlan::Adt { variants, .. } => {
            for p in variants.iter().flat_map(|(_, fields)| fields) {
                plan_uses(p, module, out);
            }
        }
        SchemaPlan::Named {
            module: m,
            ty,
            args,
            lazy,
        } => {
            if m == module && !lazy {
                out.push(companion_name(ty, "schema"));
            }
            for p in args {
                plan_uses(p, module, out);
            }
        }
        SchemaPlan::Refined { name, base, .. } => {
            out.push(companion_name(name, "check"));
            plan_uses(base, module, out);
        }
        SchemaPlan::Prim(_) | SchemaPlan::Param(_) | SchemaPlan::Constructor(_) => {}
    }
}
