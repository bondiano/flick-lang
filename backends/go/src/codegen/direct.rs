//! Methods of known instances, called directly. A method written as a
//! function is a Go function of its own, as a top-level function is, and
//! the instance's dictionary holds it: one of an instance with a context
//! takes the dictionaries of the context first, as a constrained function
//! does, and its dictionary holds it applied to them. A method computed
//! before it is a function, in an instance without a context, is a package
//! variable the dictionary is built with. Where the checker resolved a
//! method to such an instance, the code calls that function, or applies
//! that variable, with no lookup in the dictionary and without building
//! one of a context: the dictionaries of the context are passed as they
//! are resolved. It is a function of values: generic code calls it through
//! the dictionary, as `List.sort` calls `compare` of `Ord Int`, and a
//! function of Go's own types would need a wrapper converting every
//! argument and result there, which costs more than the lookup it spares.
//! An instance with a context is called directly only when each of its
//! methods is a function: building its dictionary then runs none of the
//! program's code, so not building it is not seen. Every other method
//! stays a lookup: that of a dictionary given to the code, of the argument
//! of a parameterized module or opened by `unpack`, one a value holds, one
//! of an instance with a context that computes a method, and a method the
//! instance takes from a default of its class.

use flick_pretty::{Document, docvec};

use super::go_type::TypeDecls;
use super::layout::{Code, block, call, statement};
use super::shape::{self, Shape};
use super::{Gen, Input, names};
use flick_backend_support::reach::Key;
use flick_ir::Dict;
use flick_ir::{Expr, ExprKind, ItemKind};

/// The top-level name of the function of `method` of `instance`: an
/// instance's name has a space between its class and types, which no
/// other top-level name has.
pub(super) fn name(instance: &str, method: &str) -> String {
    format!("{instance}${method}")
}

/// The methods of an instance item called directly, each with its name
/// and code: every method of an instance without a context, and those of
/// one with a context when all of them are written as functions.
pub(super) fn methods(kind: &ItemKind) -> Vec<(&str, &Expr)> {
    let ItemKind::Instance { info, methods } = kind else {
        return Vec::new();
    };
    let function = |code: &Expr| matches!(code.kind, ExprKind::Lambda { .. });
    if !info.context.is_empty() && !methods.iter().all(|(_, code)| function(code)) {
        return Vec::new();
    }
    methods
        .iter()
        .map(|(method, code)| (method.as_str(), code))
        .collect()
}

/// The shapes of the methods of `kind`, an item of `module`, by their
/// top-level names: a function of the context's dictionaries and its
/// parameters, or a variable.
pub(super) fn shapes(
    input: &Input,
    decls: &TypeDecls,
    module: &str,
    kind: &ItemKind,
) -> Vec<(Key, Shape)> {
    let ItemKind::Instance { info, .. } = kind else {
        return Vec::new();
    };
    methods(kind)
        .into_iter()
        .map(|(method, code)| {
            let key = (module.to_owned(), name(&info.name, method));
            let shape = match &code.kind {
                ExprKind::Lambda { params, body } => {
                    shape::function(input, decls, info.context.len(), params, body).of_values()
                }
                _ => Shape::Var,
            };
            (key, shape)
        })
        .collect()
}

impl<'a> Gen<'a> {
    /// The method `name` of `dict` as a value: the function of a known
    /// instance given the dictionaries of its context, or else looked up
    /// in the dictionary.
    pub(super) fn method(&mut self, dict: &Dict, name: &str) -> Code<'a> {
        if let Some((module, function)) = self.direct_method(dict, name) {
            let context = self.context_dicts(dict);
            return self.global_value(&module, &function, context);
        }
        let dict = self.dict(dict);
        let args = [self.context(), dict.doc, self.string(name)];
        call(self.a, "flickrt.Method", args).into()
    }

    /// The module and top-level name of the function or variable of
    /// `method` of `dict`, when it is called directly: a known instance
    /// whose context is given in full.
    pub(super) fn direct_method(&self, dict: &Dict, method: &str) -> Option<Key> {
        let Dict::Instance { module, name, args } = dict else {
            return None;
        };
        let key = (module.clone(), self::name(name, method));
        let direct = match self.shapes.get(&key)? {
            Shape::Func { dicts, .. } => *dicts == args.len(),
            Shape::Var => args.is_empty(),
        };
        direct.then_some(key)
    }

    /// The dictionaries of the context of `dict`, an instance, each made
    /// once where the code is: a function value made of them holds them.
    pub(super) fn context_dicts(&mut self, dict: &Dict) -> Vec<Code<'a>> {
        let Dict::Instance { args, .. } = dict else {
            return Vec::new();
        };
        args.iter()
            .map(|d| {
                let d = self.dict(d);
                self.spill(d)
            })
            .collect()
    }

    /// What the dictionary `this`, an instance of this module being built,
    /// holds as its method `name` of code `value`: the function of the
    /// method given the context's dictionaries, or the computed value,
    /// which the variable of a method called directly holds too.
    pub(super) fn method_entry(&mut self, this: &Dict, name: &str, value: &Expr) -> Document<'a> {
        let a = self.a;
        let direct = self.direct_method(this, name);
        if direct.is_some() && matches!(value.kind, ExprKind::Lambda { .. }) {
            return self.method(this, name).doc;
        }
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
        let Some((module, method)) = direct else {
            return v;
        };
        let var = a.alloc_str(&names::var(&module, &method));
        self.emit(docvec![a, var, " = ", v]);
        a.text(var)
    }

    /// The Go functions and variables of the methods of `kind`, an
    /// instance of this module, each with whether it holds a `//line`
    /// directive.
    pub(super) fn method_functions(&mut self, kind: &ItemKind) -> Vec<(Document<'a>, bool)> {
        let ItemKind::Instance { info, methods: all } = kind else {
            return Vec::new();
        };
        let mut decls = Vec::new();
        for (method, code) in all {
            let name = self::name(&info.name, method);
            match self.shapes.get(&(self.module.to_owned(), name.clone())) {
                None => {}
                // set as the dictionary is built
                Some(Shape::Var) => {
                    let var = self.a.alloc_str(&names::var(self.module, &name));
                    decls.push((docvec![self.a, "var ", var, " flickrt.Value"], false));
                }
                Some(Shape::Func { .. }) => {
                    // local names are unique only within a definition
                    self.go_types.clear();
                    self.held.clear();
                    let lines = self.lines;
                    let decl = self.function(&name, &info.context, code);
                    decls.push((decl, self.lines > lines));
                }
            }
        }
        decls
    }
}
