//! Methods of known instances, called directly. A method written as a
//! function in an instance without a context is a Go function of its own,
//! as a top-level function is, and the instance's dictionary holds it.
//! Where the checker resolved a method to such an instance, the code calls
//! that function with no lookup in the dictionary. It is a function of
//! values: generic code calls it through the dictionary, as `List.sort`
//! calls `compare` of `Ord Int`, and a function of Go's own types would
//! need a wrapper converting every argument and result there, which costs
//! more than the lookup it spares. Every other method stays a lookup: that
//! of a dictionary given to the code, of the argument of a parameterized
//! module or opened by `unpack`, one a value holds, one built from a
//! context, and a method the instance computes or takes from a default of
//! its class.

use flick_pretty::Document;

use super::layout::{Code, call};
use super::shape::{self, Shape};
use super::{Gen, Input};
use crate::ir::reach::Key;
use crate::ir::{Expr, ExprKind, ItemKind, Param};
use flick_ir::Dict;

/// The top-level name of the function of `method` of `instance`: an
/// instance's name has a space between its class and types, which no
/// other top-level name has.
pub(super) fn name(instance: &str, method: &str) -> String {
    format!("{instance}${method}")
}

/// The methods of an instance item called directly: written as a function
/// in an instance without a context. Each with its name, parameters and body.
pub(super) fn methods(kind: &ItemKind) -> Vec<(&str, &[Param], &Expr)> {
    let ItemKind::Instance { info, methods } = kind else {
        return Vec::new();
    };
    if !info.context.is_empty() {
        return Vec::new();
    }
    methods
        .iter()
        .filter_map(|(method, code)| match &code.kind {
            ExprKind::Lambda { params, body } => Some((method.as_str(), &params[..], &**body)),
            _ => None,
        })
        .collect()
}

/// The shapes of the functions of the methods of `kind`, an item of
/// `module`, by their top-level names.
pub(super) fn shapes(input: &Input, module: &str, kind: &ItemKind) -> Vec<(Key, Shape)> {
    let ItemKind::Instance { info, .. } = kind else {
        return Vec::new();
    };
    methods(kind)
        .into_iter()
        .map(|(method, params, body)| {
            let key = (module.to_owned(), name(&info.name, method));
            (key, shape::function(input, 0, params, body).of_values())
        })
        .collect()
}

impl<'a> Gen<'a> {
    /// The method `name` of `dict` as a value: the function of a known
    /// instance, or else looked up in the dictionary.
    pub(super) fn method(&mut self, dict: &Dict, name: &str) -> Code<'a> {
        if let Some((module, function)) = self.direct_method(dict, name) {
            return self.global_value(&module, &function, Vec::new());
        }
        let dict = self.dict(dict);
        let args = [self.context(), dict.doc, self.string(name)];
        call(self.a, "flickrt.Method", args).into()
    }

    /// The module and top-level name of the function of `method` of
    /// `dict`, when it is called directly.
    pub(super) fn direct_method(&self, dict: &Dict, method: &str) -> Option<Key> {
        let Dict::Instance { module, name, args } = dict else {
            return None;
        };
        let key = (module.clone(), self::name(name, method));
        (args.is_empty() && self.shapes.contains_key(&key)).then_some(key)
    }

    /// The Go functions of the methods of `kind`, an instance of this
    /// module, each with whether it holds a `//line` directive.
    pub(super) fn method_functions(&mut self, kind: &ItemKind) -> Vec<(Document<'a>, bool)> {
        let ItemKind::Instance { info, methods: all } = kind else {
            return Vec::new();
        };
        let mut decls = Vec::new();
        for (method, code) in all {
            let name = self::name(&info.name, method);
            if !self
                .shapes
                .contains_key(&(self.module.to_owned(), name.clone()))
            {
                continue;
            }
            // local names are unique only within a definition
            self.natives.clear();
            self.held.clear();
            let lines = self.lines;
            let decl = self.function(&name, &[], code);
            decls.push((decl, self.lines > lines));
        }
        decls
    }
}
