//! The part of a foreign signature that the bridge to Effect converts.

use flick_ir::Shape;

use crate::types::Checker;
use crate::types::lang::Lang;
use crate::types::ty::{Stage, Type, stage_of};

/// Past this depth a type counts as data: deep enough for any signature of std.
const MAX_DEPTH: usize = 8;

/// `inner` in `wrap`, or data when there is nothing in it to convert.
fn around(inner: Shape, wrap: fn(Box<Shape>) -> Shape) -> Shape {
    if inner.is_data() {
        inner
    } else {
        wrap(Box::new(inner))
    }
}

impl Checker<'_> {
    /// The shapes of the first `arity` parameters; a type variable that an
    /// `Api` or a `Group` beside it also has is the record of handlers they need.
    pub(super) fn param_shapes(&self, params: &[Type], arity: usize) -> Vec<Shape> {
        let hidden = |param: &Type| {
            let Type::Var(var) = self.whnf_captured(param).0 else {
                return false;
            };
            params.iter().any(|other| {
                let other = self.whnf_captured(other).0;
                let (head, args) = other.spine();
                matches!(head, Type::Con { module, name } if (module == "HttpApi" && (name == "Api" || name == "Group")) || (module == "Rpc" && name == "Group"))
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

    /// The part of `ty` the bridge to Effect converts, from the signature.
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
                    .map(|(name, field)| (name.to_string(), self.shape(field, depth + 1)))
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
            _ if matches!(head, Type::Con { module, name } if module == "Fiber" && name == "Fiber") => {
                Shape::Fiber
            }
            _ if matches!(head, Type::Con { module, name } if module == "Duration" && name == "Duration") => {
                Shape::Duration
            }
            _ if matches!(head, Type::Con { module, name } if module == "Schedule" && name == "Schedule") => {
                Shape::Schedule
            }
            _ if matches!(head, Type::Con { module, name } if module == "Chunk.Core" && (name == "Chunk" || name == "NonEmptyChunk")) => {
                Shape::Chunk
            }
            _ if matches!(head, Type::Con { module, name } if module == "HashMap" && name == "HashMap") => {
                Shape::HashMap
            }
            _ if matches!(head, Type::Con { module, name } if module == "HashSet" && name == "HashSet") => {
                Shape::HashSet
            }
            _ if head.is_lang(Lang::Schema) => Shape::Schema,
            _ if head.is_lang(Lang::PureCodec) || head.is_lang(Lang::Codec) => Shape::Codec,
            _ if head.is_lang(Lang::Exit) => Shape::Exit,
            _ if head.is_lang(Lang::Failure) => Shape::Failure,
            [item] if head.is_lang(Lang::List) => around(inner(item), Shape::List),
            [item] if head.is_lang(Lang::Option) => Shape::Option(Box::new(inner(item))),
            [error, success] if head.is_lang(Lang::Result) => {
                let (error, success) = (inner(error), inner(success));
                Shape::Result(Box::new(error), Box::new(success))
            }
            _ => Shape::Data,
        }
    }
}
