//! Top-level definitions: declarations, and the statements of the module's
//! init function that assign its values.

use std::iter;

use flick_pretty::{Document, docvec};

use super::layout::{Code, block, call, closure, list, named_again, print, signature, statement};
use super::{Gen, RUNTIME_PACKAGE, Shape, loops, names};
use crate::codegen::names as js_names;
use crate::codegen::tail::{Callee, Target};
use crate::ir::{Expr, ExprKind, Item, ItemKind, Module};
use crate::source::Span;
use crate::types::{Constructor, Dict, InstanceInfo, Rep, default_method};

/// The classes whose instances the runtime derives.
const DERIVED: &[&str] = &["Eq", "Ord", "Hash", "Show"];

/// The declarations of a module, each with whether it holds a `//line`
/// directive; the statements of its init function.
pub(super) type Decls<'a> = Vec<(Document<'a>, bool)>;

impl<'a> Gen<'a> {
    /// The Go file of `module` with the items at `order`.
    pub(super) fn module(&mut self, module: &Module, order: &[usize]) -> String {
        let a = self.a;
        let mut decls = Vec::new();
        let mut init = Vec::new();
        (self.groups, self.grouped) = loops::groups(self.module, &module.items);
        // every member of a group reaches the others: all are reached or none
        for g in 0..self.groups.len() {
            if order.contains(&self.groups[g].first()) {
                let lines = self.lines;
                let decl = self.group(g, &module.items);
                decls.push((decl, self.lines > lines));
            }
        }
        for &i in order {
            self.item(&module.items[i], &mut decls, &mut init);
        }
        let decls = decls.into_iter().map(|(decl, located)| {
            // the lines after a `//line` count from it: name the Go file again
            let again = if located {
                named_again(a, names::file(self.module))
            } else {
                a.empty()
            };
            docvec![a, decl, again, a.lines(2)]
        });
        let init = docvec![a, "func ", names::init(self.module), "() ", block(a, init)];
        print(docvec![
            a,
            "package main",
            a.lines(2),
            "import \"",
            RUNTIME_PACKAGE,
            "\"",
            a.lines(2),
            "var _ = flickrt.Unit",
            a.lines(2),
            a.concat(decls),
            init,
            a.line()
        ])
    }

    fn item(&mut self, item: &Item, decls: &mut Decls<'a>, init: &mut Vec<Document<'a>>) {
        let a = self.a;
        // local names are unique only within a definition
        self.natives.clear();
        // whether the declaration made now holds a `//line` directive
        let lines = self.lines;
        match &item.kind {
            ItemKind::Value { name, dicts, body } => match super::shape(&item.kind) {
                Shape::Var => {
                    let var = a.alloc_str(&names::var(self.module, name));
                    decls.push((docvec![a, "var ", var, " flickrt.Value"], false));
                    let (mut stmts, v) = self.block(|g| g.expr(body));
                    stmts.push(statement(a, docvec![a, var, " = ", v]));
                    init.push(statement(a, block(a, stmts)));
                }
                Shape::Func { .. } => {
                    let decl = self.function(name, dicts, body);
                    decls.push((decl, self.lines > lines));
                }
            },
            ItemKind::Destructure { pattern, body } => {
                let mut binders = Vec::new();
                pattern.binders(&mut binders);
                let (mut stmts, ()) = self.block(|g| {
                    let v = g.expr(body);
                    let v = g.spill(v);
                    g.destructure(pattern, v.doc);
                });
                for b in binders {
                    let var = a.alloc_str(&names::var(self.module, b));
                    decls.push((docvec![a, "var ", var, " flickrt.Value"], false));
                    stmts.push(statement(a, docvec![a, var, " = ", names::local(b)]));
                }
                init.push(statement(a, block(a, stmts)));
            }
            ItemKind::Foreign(f) => {
                if let Some(decl) = self.foreign(f, item.span) {
                    decls.push((decl, false));
                }
            }
            ItemKind::Instance { info, methods } => {
                let id = a.alloc_str(&names::instance(self.module, &info.name));
                let dicts = self.bind_givens(&info.context);
                let context = self.context.replace("fb");
                let (mut stmts, ()) = self.block(|g| g.instance(info, methods, item.span));
                self.context = context;
                self.givens.clear();
                stmts.push(statement(a, "return d"));
                let body = block(a, stmts);
                if dicts.is_empty() {
                    decls.push((docvec![a, "var ", id, " flickrt.Value"], false));
                    let make = docvec![
                        a,
                        id,
                        " = func(fb *flickrt.Fiber) flickrt.Value ",
                        body,
                        "(nil)"
                    ];
                    init.push(statement(a, make));
                } else {
                    let params = signature(&dicts);
                    let decl = docvec![a, "func ", id, "(", params, ") flickrt.Value ", body];
                    decls.push((decl, self.lines > lines));
                }
            }
            // a service is its key, written where it is used
            ItemKind::Constructor(_) | ItemKind::Service { .. } => {}
            ItemKind::Refined { .. } => {
                self.unsupported(item.span, "a refined type");
            }
            ItemKind::Schema(s) => self.schema(s, item.span, decls, init),
        }
    }

    /// The Go function of a top-level function: its dictionaries, then its
    /// parameters, all at once.
    fn function(&mut self, name: &str, dicts: &[String], body: &Expr) -> Document<'a> {
        let a = self.a;
        let id = names::func(self.module, name);
        let effectful = self.shape_of(self.module, name).effectful();
        self.fiber = effectful.then_some("fb");
        self.context = Some("fb");
        let mut heads = self.bind_givens(dicts);
        let stmts = match &body.kind {
            ExprKind::Lambda { params, body } => {
                let target = Target {
                    callee: Callee::Global(format!("{}.{name}", self.module)),
                    dicts: dicts.to_vec(),
                };
                let lines = self.lines;
                let (bound, stmts) = self.function_body(params, body, Some(target));
                let grouped = self.group_call(name, &bound);
                if grouped.is_some() {
                    // the body is the group's: this function holds no directive
                    self.lines = lines;
                }
                heads.extend(bound);
                grouped.map_or(stmts, |go| vec![go])
            }
            _ => self.block(|g| g.tail_expr(body)).0,
        };
        self.givens.clear();
        self.fiber = None;
        self.context = None;
        let params = signature(&heads);
        docvec![
            a,
            "func ",
            id,
            "(",
            params,
            ") flickrt.Value ",
            block(a, stmts)
        ]
    }

    /// The statements that build the dictionary `d` of an instance.
    fn instance(&mut self, info: &InstanceInfo, methods: &[(String, Expr)], span: Span) {
        let a = self.a;
        let supers: Vec<(String, Code<'a>)> = info
            .supers
            .iter()
            .map(|(class, dict)| (class.clone(), self.dict(dict)))
            .collect();
        if let Some((fields, constructors)) = &info.derived {
            let class = &info.class.1;
            if !DERIVED.contains(&class.as_str()) {
                self.unsupported(span, &format!("`deriving {class}`"));
                self.emit("d := flickrt.NewDict()");
                return;
            }
            let fields: Vec<Code<'a>> = fields.iter().map(|d| self.dict(d)).collect();
            let fields = list(a, "[]flickrt.Value{", fields.iter().map(|f| f.doc), "}");
            let fields = closure(
                a,
                "func(fb *flickrt.Fiber) []flickrt.Value",
                [docvec![a, "return ", fields]],
            );
            let constructors = match constructors {
                None => a.text("nil"),
                Some(cs) => {
                    let cs: Vec<Document<'a>> =
                        cs.iter().map(|c| self.constructor_info(c, span)).collect();
                    list(a, "[]flickrt.Constructor{", cs, "}")
                }
            };
            let supers = supers.iter().map(|(class, d)| {
                let get = closure(
                    a,
                    "func(fb *flickrt.Fiber) flickrt.Value",
                    [docvec![a, "return ", *d]],
                );
                docvec![a, self.string(class), ": ", get]
            });
            let supers = list(
                a,
                "map[string]func(*flickrt.Fiber) flickrt.Value{",
                supers,
                "}",
            );
            let derive = call(
                a,
                "flickrt.Derive",
                [self.string(class), fields, constructors, supers],
            );
            self.emit(docvec![a, "d := ", derive]);
            return;
        }
        self.emit("d := flickrt.NewDict()");
        for (class, d) in supers {
            let get = closure(
                a,
                "func(fb *flickrt.Fiber) flickrt.Value",
                [docvec![a, "return ", d]],
            );
            let key = self.string(&format!("${class}"));
            self.emit(call(a, "d.Lazy", [key, get]));
        }
        for (name, value) in methods {
            self.natives.clear();
            let (stmts, v) = self.block(|g| g.expr(value));
            let v = if stmts.is_empty() {
                v.doc
            } else {
                let ret = statement(a, docvec![a, "return ", v]);
                let body = block(a, stmts.into_iter().chain([ret]));
                docvec![
                    a,
                    "func(fb *flickrt.Fiber) flickrt.Value ",
                    body,
                    "(",
                    self.context(),
                    ")"
                ]
            };
            self.emit(call(a, "d.Set", [self.string(name), v]));
        }
        let (module, class) = &info.class;
        for (name, own) in &info.defaults {
            let default = default_method(class, name);
            self.reads(module);
            let mut dicts = vec![self.code("d")];
            let value = if *own == 0 {
                self.global_value(module, &default, dicts)
            } else {
                dicts.extend((0..*own).map(|i| self.code(&format!("o[{i}]"))));
                let inner = self.global_value(module, &default, dicts);
                let head = "func(fb *flickrt.Fiber, o ...flickrt.Value) flickrt.Value";
                let f = closure(a, head, [docvec![a, "return ", inner]]);
                call(a, "flickrt.Poly", [f]).into()
            };
            let get = closure(
                a,
                "func(fb *flickrt.Fiber) flickrt.Value",
                [docvec![a, "return ", value]],
            );
            self.emit(call(a, "d.Lazy", [self.string(name), get]));
        }
    }

    fn constructor_info(&mut self, c: &Constructor, span: Span) -> Document<'a> {
        let a = self.a;
        if let Rep::Typeof(_) = c.rep {
            self.unsupported(span, "an `untagged` type");
        }
        let unboxed = if c.rep == Rep::Unboxed {
            "true"
        } else {
            "false"
        };
        let fields = [
            docvec![a, "Tag: ", self.string(&self.tag(c))],
            docvec![a, "Name: ", self.string(&c.name)],
            docvec![a, "Arity: ", c.arity],
            docvec![a, "Unboxed: ", unboxed],
        ];
        list(a, "{", fields, "}")
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

    pub(super) fn dict(&mut self, dict: &Dict) -> Code<'a> {
        let a = self.a;
        match dict {
            Dict::Instance { module, name, args } => {
                self.reads(module);
                let id = names::instance(module, name);
                if args.is_empty() {
                    self.code(&id)
                } else {
                    let args: Vec<Document<'a>> = args.iter().map(|d| self.dict(d).doc).collect();
                    call(a, id, iter::once(self.context()).chain(args)).into()
                }
            }
            Dict::Given(shown) => {
                let given = self
                    .givens
                    .iter()
                    .rev()
                    .find(|(g, _)| g == shown)
                    .map_or_else(|| panic!("dictionary `{shown}` is in scope"), |&(_, p)| p);
                self.code(given)
            }
            Dict::Super { dict, class } => {
                let d = self.dict(dict);
                call(
                    a,
                    "flickrt.Super",
                    [self.context(), d.doc, self.string(class)],
                )
                .into()
            }
            Dict::Structural { class, fields } => {
                let fields: Vec<Document<'a>> = fields.iter().map(|f| self.dict(f).doc).collect();
                let fields = list(a, "[]flickrt.Value{", fields, "}");
                call(a, "flickrt.Structural", [self.string(class), fields]).into()
            }
            Dict::Primitive { labels, .. } => {
                let labels = labels.iter().map(|l| self.string(l));
                list(a, "[]flickrt.Value{", labels, "}").into()
            }
            Dict::Error => self.unit(),
        }
    }
}
