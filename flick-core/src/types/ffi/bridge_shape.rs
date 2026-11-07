//! spike: the part of a foreign signature the bridge to Effect converts.

use crate::syntax::cst::Node;
use crate::types::Checker;
use crate::types::lang::Lang;
use crate::types::ty::{Stage, Type};
use crate::types::typing::ForeignInfo;

impl Checker<'_> {
    pub(super) fn bridge_dump(&self, decl: &Node, info: &ForeignInfo) {
        if let Ok(path) = std::env::var("FLICK_BRIDGE_DUMP") {
            use std::io::Write;
            let binding = decl.text(self.text).lines().find(|l| l.trim_start().starts_with("foreign")).unwrap_or("").trim().to_owned();
            let line = format!(
                "{}\t{}\t{:?}\t{:?}\t{:?}\n",
                self.env.module,
                binding,
                info.mode,
                info.adapter.shapes,
                info.adapter.result_shape,
            );
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
                let _ = f.write_all(line.as_bytes());
            }
        }
    }

    /// spike: the part of `ty` the bridge to Effect converts, from the signature.
    pub(super) fn shape(&self, ty: &Type, depth: usize) -> flick_ir::Shape {
        use flick_ir::Shape;
        if depth > 8 {
            return Shape::Data;
        }
        let t = self.whnf_captured(ty).0;
        match &t {
            Type::Forall { body, .. } => self.shape(body, depth + 1),
            Type::Fun { .. } => {
                let mut args = Vec::new();
                let mut cur = t.clone();
                loop {
                    let Type::Fun { param, effects, result } = self.whnf_captured(&cur).0 else {
                        break;
                    };
                    args.push(self.shape(&param, depth + 1));
                    if crate::types::ty::stage_of(effects.as_deref()) != Stage::Pure {
                        return Shape::Fun {
                            args,
                            result: Box::new(self.shape(&result, depth + 1)),
                            runtime: true,
                        };
                    }
                    cur = *result;
                }
                let result = self.shape(&cur, depth + 1);
                if result.is_data() && args.iter().all(Shape::is_data) {
                    return Shape::Data;
                }
                Shape::Fun { args, result: Box::new(result), runtime: false }
            }
            Type::Tuple(items) => {
                let shapes: Vec<Shape> = items.iter().map(|i| self.shape(i, depth + 1)).collect();
                if shapes.iter().all(Shape::is_data) { Shape::Data } else { Shape::Tuple(shapes) }
            }
            Type::Record(row) => {
                let fields: Vec<(String, Shape)> = row
                    .entries
                    .iter()
                    .map(|(n, ty)| (n.clone(), self.shape(ty, depth + 1)))
                    .collect();
                if fields.iter().all(|(_, s)| s.is_data()) { Shape::Data } else { Shape::Record(fields) }
            }
            _ => {
                if let Some((_, _, a)) = self.eff_parts(&t) {
                    return Shape::Eff(Box::new(self.shape(&a, depth + 1)));
                }
                let (head, targs) = t.spine();
                if head.is_lang(Lang::Layer) {
                    return Shape::Layer;
                }
                if head.is_lang(Lang::Exit) {
                    return Shape::Exit;
                }
                if head.is_lang(Lang::Failure) {
                    return Shape::Failure;
                }
                let inner = |item: &Type| -> Shape { self.shape(item, depth + 1) };
                match targs[..] {
                    [a] if head.is_lang(Lang::List) => {
                        let s = inner(a);
                        if s.is_data() { s } else { Shape::List(Box::new(s)) }
                    }
                    [a] if head.is_lang(Lang::Option) => {
                        let s = inner(a);
                        if s.is_data() { s } else { Shape::Option(Box::new(s)) }
                    }
                    _ => Shape::Data,
                }
            }
        }
    }

}
