//! Refined types, schema witnesses and `codec` blocks: runtime calls that
//! build the inert schema and codec trees.

use flick_pretty::docvec;

use super::doc::{After, Before, Js, Stmt, array, call, object};
use super::{Gen, names};
use crate::ir::Expr;
use crate::types::{Constructor, Rep, SchemaItem, SchemaPlan, UrlRoute, companion_name};

impl<'d> Gen<'_, 'd> {
    /// `T$check`, `T$parse` and `T$value` of a refined type.
    pub(super) fn refined(&mut self, name: &str, check: &Expr) -> Vec<Stmt<'d>> {
        let (mut stmts, value) = self.block(|g| g.function(&[], check, None));
        let check_name = companion_name(name, "check");
        stmts.push(self.export(&check_name, value));
        let refined_parse = self.rt("refinedParse");
        let parse = call(
            self.a,
            refined_parse,
            &[self.string(name), self.leaf(&check_name)],
        );
        stmts.push(self.export(&companion_name(name, "parse"), parse));
        let identity = self.lambda_of("$x", self.leaf("$x"));
        stmts.push(self.export(&companion_name(name, "value"), identity));
        stmts
    }

    /// `T$schema`, a function of the witnesses of its parameters, and the
    /// `T$json` of a refined type.
    pub(super) fn schema(&mut self, item: &SchemaItem) -> Vec<Stmt<'d>> {
        let a = self.a;
        let mut body = self.plan(&item.plan);
        for i in (0..item.witnesses).rev() {
            body = self.lambda_of(&format!("$w{i}"), body);
        }
        let name = companion_name(&item.ty, "schema");
        let mut out = vec![self.export(&name, body)];
        if matches!(item.plan, SchemaPlan::Refined { .. }) {
            let codec = self.rt("schemaJsonCodec");
            let json = call(a, codec, &[self.leaf(&name)]);
            out.push(self.export(&companion_name(&item.ty, "json"), json));
        }
        if let Some(routes) = &item.url {
            let url = self.url(&name, &item.ty, routes);
            out.push(self.export(&companion_name(&item.ty, "url"), url));
        }
        if let Some(fields) = &item.form {
            let specs: Vec<Js<'d>> = fields
                .iter()
                .map(|(l, p, optional)| {
                    let spec = [
                        self.string(l),
                        self.plan(p),
                        self.leaf(&optional.to_string()),
                    ];
                    array(a, &spec)
                })
                .collect();
            let form_spec = self.rt("formSpec");
            let form = call(
                a,
                form_spec,
                &[self.leaf(&name), self.string(&item.ty), array(a, &specs)],
            );
            out.push(self.export(&companion_name(&item.ty, "form"), form));
            // a field is its label at run time
            let labels: Vec<_> = fields
                .iter()
                .map(|(l, ..)| {
                    let label = self.string(l);
                    docvec![
                        a,
                        label.doc,
                        ": ",
                        label.at(a, Before::Delimiter, After::Delimiter)
                    ]
                })
                .collect();
            let record = object(a, &labels, &[]);
            out.push(self.export(&companion_name(&item.ty, "fields"), record));
        }
        out
    }

    /// `T$url`: per constructor its tag, path, segment witnesses and query.
    fn url(&mut self, schema: &str, ty: &str, routes: &[UrlRoute]) -> Js<'d> {
        let a = self.a;
        let package = (self.options.package)(self.module);
        let routes: Vec<Js<'d>> = routes
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
                let segments: Vec<Js<'d>> = r.segments.iter().map(|p| self.plan(p)).collect();
                let query = match &r.query {
                    Some(query) => {
                        let fields: Vec<Js<'d>> = query
                            .iter()
                            .map(|(l, p, optional)| {
                                let field = [
                                    self.string(l),
                                    self.plan(p),
                                    self.leaf(&optional.to_string()),
                                ];
                                array(a, &field)
                            })
                            .collect();
                        array(a, &fields)
                    }
                    None => self.leaf("null"),
                };
                let route = [
                    self.string(&tag),
                    self.string(&r.path),
                    array(a, &segments),
                    query,
                ];
                array(a, &route)
            })
            .collect();
        let route_codec = self.rt("routeCodec");
        call(a, route_codec, &[self.leaf(schema), array(a, &routes)])
    }

    /// The runtime's schema node `f` of `args`.
    fn node(&mut self, f: &str, args: &[Js<'d>]) -> Js<'d> {
        let f = self.rt(f);
        call(self.a, f, args)
    }

    fn plan(&mut self, plan: &SchemaPlan) -> Js<'d> {
        let a = self.a;
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
                self.node("schemaList", &[p])
            }
            SchemaPlan::Option(p) => {
                let p = self.plan(p);
                self.node("schemaOption", &[p])
            }
            SchemaPlan::Redacted(p) => {
                let p = self.plan(p);
                self.node("schemaRedacted", &[p])
            }
            SchemaPlan::Result(e, ok) => {
                let (e, ok) = (self.plan(e), self.plan(ok));
                self.node("schemaResult", &[e, ok])
            }
            SchemaPlan::Tuple(items) => {
                let items: Vec<Js<'d>> = items.iter().map(|p| self.plan(p)).collect();
                self.node("schemaTuple", &[array(a, &items)])
            }
            SchemaPlan::Record(fields) => {
                let fields: Vec<Js<'d>> = fields
                    .iter()
                    .map(|(l, p)| {
                        let field = [self.string(l), self.plan(p)];
                        array(a, &field)
                    })
                    .collect();
                self.node("schemaRecord", &[array(a, &fields)])
            }
            SchemaPlan::Adt { name, variants } => self.adt(name, variants),
            SchemaPlan::Param(i) => self.leaf(&format!("$w{i}")),
            SchemaPlan::Applied(i, p) => {
                let p = self.plan(p);
                call(a, self.leaf(&format!("$w{i}")), &[p])
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
                let mut applied = self.global(module, &companion_name(named, "schema"));
                for arg in args {
                    let arg = self.plan(arg);
                    applied = call(a, applied, &[arg]);
                }
                if *lazy {
                    let get = self.lambda_of("", applied);
                    self.node("schemaLazy", &[self.string(named), get])
                } else {
                    applied
                }
            }
            SchemaPlan::Refined { name, base, known } => {
                let base = self.plan(base);
                let known = known
                    .as_deref()
                    .map_or_else(|| self.leaf("null"), |k| self.string(k));
                let check = self.leaf(&companion_name(name, "check"));
                self.node("refinedSchema", &[self.string(name), base, check, known])
            }
        }
    }

    fn adt(&mut self, name: &str, variants: &[(String, Vec<SchemaPlan>)]) -> Js<'d> {
        let a = self.a;
        let package = (self.options.package)(self.module);
        let variants: Vec<Js<'d>> = variants
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
                let fields: Vec<Js<'d>> = fields.iter().map(|p| self.plan(p)).collect();
                let variant = [self.string(&tag), self.string(c), array(a, &fields)];
                array(a, &variant)
            })
            .collect();
        self.node("schemaAdt", &[self.string(name), array(a, &variants)])
    }

    /// A `codec` block: its fields, evaluated in order.
    pub(super) fn codec_block(&mut self, name: &str, fields: &[(String, Expr)]) -> Js<'d> {
        let a = self.a;
        let values: Vec<&Expr> = fields.iter().map(|(_, v)| v).collect();
        let values = self.ordered(&values);
        let entries: Vec<Js<'d>> = fields
            .iter()
            .zip(values)
            .map(|((l, _), v)| {
                let entry = [self.string(l), v];
                array(a, &entry)
            })
            .collect();
        self.node("codecBlock", &[self.string(name), array(a, &entries)])
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
