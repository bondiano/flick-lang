//! spike: the part of a foreign signature the bridge to Effect converts.

use std::env;
use std::fs::OpenOptions;
use std::io::Write;

use flick_ir::Shape;

use crate::syntax::cst::Node;
use crate::types::Checker;
use crate::types::lang::{Lang, label};
use crate::types::ty::{Stage, Type, stage_of};
use crate::types::typing::ForeignInfo;

/// Past this depth a type counts as data: deep enough for any signature of std.
const MAX_DEPTH: usize = 8;

/// spike: the row of the first runtime arrow: `async`, `open` with a variable, `closed` without `Async`, or `pure`.
fn row_class(mut ty: &Type) -> &'static str {
    loop {
        match ty {
            Type::Fun {
                effects: Some(effects),
                ..
            } => {
                let Type::Row(row) = &effects.services else {
                    return "open";
                };
                let async_label = label(Lang::Async);
                if row
                    .entries
                    .iter()
                    .any(|(name, _)| Some(name.as_str()) == async_label)
                {
                    return "async";
                }
                return if row.tail.is_some() { "open" } else { "closed" };
            }
            Type::Fun { result, .. } => ty = result,
            Type::Captured { ty: inner, .. } | Type::Bounded { ty: inner, .. } => ty = inner,
            Type::Forall { body, .. } => ty = body,
            _ => return "pure",
        }
    }
}

/// `inner` in `wrap`, or data when there is nothing in it to convert.
fn around(inner: Shape, wrap: fn(Box<Shape>) -> Shape) -> Shape {
    if inner.is_data() {
        inner
    } else {
        wrap(Box::new(inner))
    }
}

impl Checker<'_> {
    /// spike: with `FLICK_BRIDGE_DUMP=<file>`, a line per foreign binding of
    /// what the bridge converts, for `tools/load/spike/count.py`.
    pub(super) fn bridge_dump(&self, decl: &Node, info: &ForeignInfo, body: &Type) {
        let Ok(path) = env::var("FLICK_BRIDGE_DUMP") else {
            return;
        };
        let binding = decl
            .text(self.text)
            .lines()
            .find(|line| line.trim_start().starts_with("foreign"))
            .unwrap_or("")
            .trim()
            .to_owned();
        let line = format!(
            "{}\t{}\t{:?}\t{:?}\t{:?}\t{}\n",
            self.env.module,
            binding,
            info.mode,
            info.adapter.shapes,
            info.adapter.result_shape,
            row_class(body),
        );
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            // a lost line only makes the count short: the check goes on
            let _ = file.write_all(line.as_bytes());
        }
    }

    /// spike: the shapes of the first `arity` parameters; a type variable that an
    /// `Api` or a `Group` beside it also has is the record of handlers they need.
    pub(super) fn param_shapes(&self, params: &[Type], arity: usize) -> Vec<Shape> {
        let hidden = |param: &Type| {
            let Type::Var(var) = self.whnf_captured(param).0 else {
                return false;
            };
            params.iter().any(|other| {
                let other = self.whnf_captured(other).0;
                let (head, args) = other.spine();
                matches!(head, Type::Con { name, .. } if name == "Api" || name == "Group")
                    && args
                        .iter()
                        .any(|arg| matches!(arg, Type::Var(w) if *w == var))
            })
        };
        params
            .iter()
            .take(arity)
            .map(|param| {
                if hidden(param) {
                    Shape::Handlers
                } else {
                    self.shape(param, 0)
                }
            })
            .collect()
    }

    /// spike: the part of `ty` the bridge to Effect converts, from the signature.
    pub(super) fn shape(&self, ty: &Type, depth: usize) -> Shape {
        if depth > MAX_DEPTH {
            return Shape::Data;
        }
        let ty = self.whnf_captured(ty).0;
        match &ty {
            Type::Forall { body, .. } => self.shape(body, depth + 1),
            Type::Fun { .. } => self.fun_shape(ty.clone(), depth),
            Type::Tuple(items) => {
                let shapes: Vec<Shape> = items
                    .iter()
                    .map(|item| self.shape(item, depth + 1))
                    .collect();
                if shapes.iter().all(Shape::is_data) {
                    Shape::Data
                } else {
                    Shape::Tuple(shapes)
                }
            }
            Type::Record(row) => {
                let fields: Vec<(String, Shape)> = row
                    .entries
                    .iter()
                    .map(|(name, field)| (name.clone(), self.shape(field, depth + 1)))
                    .collect();
                if fields.iter().all(|(_, field)| field.is_data()) {
                    Shape::Data
                } else {
                    Shape::Record(fields)
                }
            }
            _ => self.named_shape(&ty, depth),
        }
    }

    /// The pure arrows of `ty` up to and including the first runtime one.
    fn fun_shape(&self, ty: Type, depth: usize) -> Shape {
        let mut args = Vec::new();
        let mut rest = ty;
        while let Type::Fun {
            param,
            effects,
            result,
        } = self.whnf_captured(&rest).0
        {
            args.push(self.shape(&param, depth + 1));
            if stage_of(effects.as_deref()) != Stage::Pure {
                return Shape::Fun {
                    args,
                    result: Box::new(self.shape(&result, depth + 1)),
                    runtime: true,
                    sync: effects.as_deref().is_some_and(|e| e.sync),
                };
            }
            rest = *result;
        }
        let result = self.shape(&rest, depth + 1);
        if result.is_data() && args.iter().all(Shape::is_data) {
            return Shape::Data;
        }
        Shape::Fun {
            args,
            result: Box::new(result),
            runtime: false,
            sync: false,
        }
    }

    /// An `Eff`, a type the language names, or data.
    fn named_shape(&self, ty: &Type, depth: usize) -> Shape {
        if let Some((_, _, value)) = self.eff_parts(ty) {
            return Shape::Eff(Box::new(self.shape(&value, depth + 1)));
        }
        let (head, args) = ty.spine();
        let inner = |item: &Type| self.shape(item, depth + 1);
        match args[..] {
            _ if head.is_lang(Lang::Layer) => Shape::Layer,
            _ if head.is_lang(Lang::Exit) => Shape::Exit,
            _ if head.is_lang(Lang::Failure) => Shape::Failure,
            [item] if head.is_lang(Lang::List) => around(inner(item), Shape::List),
            [item] if head.is_lang(Lang::Option) => around(inner(item), Shape::Option),
            [error, success] if head.is_lang(Lang::Result) => {
                let (error, success) = (inner(error), inner(success));
                if error.is_data() && success.is_data() {
                    Shape::Data
                } else {
                    Shape::Result(Box::new(error), Box::new(success))
                }
            }
            _ => Shape::Data,
        }
    }
}
