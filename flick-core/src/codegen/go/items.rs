//! Top-level definitions: declarations, and the statements of the module's
//! init function that assign its values.

use std::fmt::Write;

use super::{Gen, Shape, indent, names, param_list};
use crate::codegen::names as js_names;
use crate::ir::{Expr, ExprKind, Item, ItemKind, Module};
use crate::source::Span;
use crate::types::{Constructor, Dict, InstanceInfo, Rep, default_method};

/// The classes whose instances the runtime derives.
const DERIVED: &[&str] = &["Eq", "Ord", "Hash", "Show"];

impl Gen<'_> {
    /// The Go file of `module` with the items at `order`.
    pub(super) fn module(&mut self, module: &Module, order: &[usize]) -> String {
        let mut decls = Vec::new();
        let mut init = Vec::new();
        for &i in order {
            self.item(&module.items[i], &mut decls, &mut init);
        }
        let mut code = format!(
            "package main\n\nimport \"{}\"\n\nvar _ = flickrt.Unit\n\n",
            super::RUNTIME_PACKAGE
        );
        for decl in decls {
            code.push_str(&decl);
            code.push_str("\n\n");
        }
        let _ = write!(
            code,
            "func {}() {{\n{}}}\n",
            names::init(self.module),
            indent(&init)
        );
        code
    }

    fn item(&mut self, item: &Item, decls: &mut Vec<String>, init: &mut Vec<String>) {
        match &item.kind {
            ItemKind::Value { name, dicts, body } => match super::shape(&item.kind) {
                Shape::Var => {
                    let var = names::var(self.module, name);
                    decls.push(format!("var {var} flickrt.Value"));
                    let (mut stmts, v) = self.block(|g| g.expr(body));
                    stmts.push(format!("{var} = {v}"));
                    init.push(format!("{{\n{}}}", indent(&stmts)));
                }
                Shape::Func { .. } => decls.push(self.function(name, dicts, body)),
            },
            ItemKind::Destructure { pattern, body } => {
                let mut binders = Vec::new();
                pattern.binders(&mut binders);
                let (mut stmts, ()) = self.block(|g| {
                    let v = g.expr(body);
                    let v = g.spill(v);
                    g.destructure(pattern, &v);
                });
                for b in binders {
                    let var = names::var(self.module, b);
                    decls.push(format!("var {var} flickrt.Value"));
                    stmts.push(format!("{var} = {}", names::local(b)));
                }
                init.push(format!("{{\n{}}}", indent(&stmts)));
            }
            ItemKind::Foreign(f) => {
                if let Some(decl) = self.foreign(f, item.span) {
                    decls.push(decl);
                }
            }
            ItemKind::Instance { info, methods } => {
                let id = names::instance(self.module, &info.name);
                let dicts = self.bind_givens(&info.context);
                let (stmts, ()) = self.block(|g| g.instance(info, methods, item.span));
                self.givens.clear();
                let body = format!("{}\treturn d\n", indent(&stmts));
                if dicts.is_empty() {
                    decls.push(format!("var {id} flickrt.Value"));
                    init.push(format!("{id} = func() flickrt.Value {{\n{body}}}()"));
                } else {
                    decls.push(format!(
                        "func {id}({}) flickrt.Value {{\n{body}}}",
                        param_list(&dicts)
                    ));
                }
            }
            ItemKind::Constructor(_) => {}
            ItemKind::Service { .. } => {
                self.unsupported(item.span, "a service");
            }
            ItemKind::Refined { .. } => {
                self.unsupported(item.span, "a refined type");
            }
            ItemKind::Schema(_) => {
                self.unsupported(item.span, "a schema");
            }
        }
    }

    /// The Go function of a top-level function: its dictionaries, then its
    /// parameters, all at once.
    fn function(&mut self, name: &str, dicts: &[String], body: &Expr) -> String {
        let id = names::func(self.module, name);
        let mut heads = self.bind_givens(dicts);
        let stmts = match &body.kind {
            ExprKind::Lambda { params, body } => {
                let (stmts, ()) = self.block(|g| {
                    let bound = g.bind_params(params);
                    heads.extend(bound);
                    g.tail_expr(body);
                });
                stmts
            }
            _ => self.block(|g| g.tail_expr(body)).0,
        };
        self.givens.clear();
        format!(
            "func {id}({}) flickrt.Value {{\n{}}}",
            param_list(&heads),
            indent(&stmts)
        )
    }

    /// The statements that build the dictionary `d` of an instance.
    fn instance(&mut self, info: &InstanceInfo, methods: &[(String, Expr)], span: Span) {
        let supers: Vec<(String, String)> = info
            .supers
            .iter()
            .map(|(class, dict)| (class.clone(), self.dict(dict)))
            .collect();
        if let Some((fields, constructors)) = &info.derived {
            let class = &info.class.1;
            if !DERIVED.contains(&class.as_str()) {
                self.unsupported(span, &format!("`deriving {class}`"));
                self.buf.push("d := flickrt.NewDict()".to_owned());
                return;
            }
            let fields: Vec<String> = fields.iter().map(|d| self.dict(d)).collect();
            let constructors = match constructors {
                None => "nil".to_owned(),
                Some(cs) => {
                    let cs: Vec<String> =
                        cs.iter().map(|c| self.constructor_info(c, span)).collect();
                    format!("[]flickrt.Constructor{{{}}}", cs.join(", "))
                }
            };
            let supers: Vec<String> = supers
                .iter()
                .map(|(class, d)| {
                    format!(
                        "{}: func() flickrt.Value {{ return {d} }}",
                        names::string(class)
                    )
                })
                .collect();
            self.buf.push(format!(
                "d := flickrt.Derive({}, func() []flickrt.Value {{ return []flickrt.Value{{{}}} }}, {constructors}, map[string]func() flickrt.Value{{{}}})",
                names::string(class),
                fields.join(", "),
                supers.join(", ")
            ));
            return;
        }
        self.buf.push("d := flickrt.NewDict()".to_owned());
        for (class, d) in supers {
            self.buf.push(format!(
                "d.Lazy({}, func() flickrt.Value {{ return {d} }})",
                names::string(&format!("${class}"))
            ));
        }
        for (name, value) in methods {
            let (stmts, v) = self.block(|g| g.expr(value));
            let v = if stmts.is_empty() {
                v
            } else {
                format!(
                    "func() flickrt.Value {{\n{}\treturn {v}\n}}()",
                    indent(&stmts)
                )
            };
            self.buf
                .push(format!("d.Set({}, {v})", names::string(name)));
        }
        let (module, class) = &info.class;
        for (name, own) in &info.defaults {
            let default = default_method(class, name);
            self.reads(module);
            let mut dicts = vec!["d".to_owned()];
            let value = if *own == 0 {
                self.global_value(module, &default, dicts)
            } else {
                let os: Vec<String> = (0..*own).map(|i| format!("o[{i}]")).collect();
                dicts.extend(os);
                let inner = self.global_value(module, &default, dicts);
                format!("flickrt.Poly(func(o ...flickrt.Value) flickrt.Value {{ return {inner} }})")
            };
            self.buf.push(format!(
                "d.Lazy({}, func() flickrt.Value {{ return {value} }})",
                names::string(name)
            ));
        }
    }

    fn constructor_info(&mut self, c: &Constructor, span: Span) -> String {
        if let Rep::Typeof(_) = c.rep {
            self.unsupported(span, "an `untagged` type");
        }
        format!(
            "{{Tag: {}, Name: {}, Arity: {}, Unboxed: {}}}",
            names::string(&self.tag(c)),
            names::string(&c.name),
            c.arity,
            c.rep == Rep::Unboxed
        )
    }

    /// The tag of a tagged constructor, as the JS target writes it.
    pub(super) fn tag(&self, c: &Constructor) -> String {
        match js_names::native(c) {
            Some(js_names::Native::Some) => "Some".to_owned(),
            Some(js_names::Native::None) => "None".to_owned(),
            Some(js_names::Native::Ok) => "Success".to_owned(),
            Some(js_names::Native::Err) => "Failure".to_owned(),
            None => js_names::tag(c, (self.package)(&c.module).as_deref()),
        }
    }

    pub(super) fn dict(&mut self, dict: &Dict) -> String {
        match dict {
            Dict::Instance { module, name, args } => {
                self.reads(module);
                let id = names::instance(module, name);
                if args.is_empty() {
                    id
                } else {
                    let args: Vec<String> = args.iter().map(|a| self.dict(a)).collect();
                    format!("{id}({})", args.join(", "))
                }
            }
            Dict::Given(shown) => self
                .givens
                .iter()
                .rev()
                .find(|(g, _)| g == shown)
                .map_or_else(
                    || panic!("dictionary `{shown}` is in scope"),
                    |(_, p)| p.clone(),
                ),
            Dict::Super { dict, class } => {
                format!(
                    "flickrt.Super({}, {})",
                    self.dict(dict),
                    names::string(class)
                )
            }
            Dict::Structural { class, fields } => {
                let fields: Vec<String> = fields.iter().map(|f| self.dict(f)).collect();
                format!(
                    "flickrt.Structural({}, []flickrt.Value{{{}}})",
                    names::string(class),
                    fields.join(", ")
                )
            }
            Dict::Primitive { labels, .. } => {
                let labels: Vec<String> = labels.iter().map(|l| names::string(l)).collect();
                format!("[]flickrt.Value{{{}}}", labels.join(", "))
            }
            Dict::Error => "flickrt.Unit".to_owned(),
        }
    }
}
