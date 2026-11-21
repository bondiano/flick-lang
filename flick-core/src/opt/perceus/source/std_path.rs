//! Audited `Path` foreign adapters (`packages/std/src/Path.fl`).

use flick_ir::{Foreign, ForeignMode};

use super::expr_builder::ExprBuilder;
use super::lazy_scope::has_standard_shape;
use super::std_schema_rt::{rt, str_lit};
use super::{Code, Env, Expr, ExprKind, Local, Lower, LowerError, Span};

impl Lower<'_> {
    pub(super) fn std_path_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 || !has_standard_shape(value) || value.mode != ForeignMode::Pure {
            return None;
        }
        let from = value.binding.from.as_deref()?;
        if from != "flick:Path" {
            return None;
        }
        let bind = value.binding.name.as_str();
        if value.stages.len() != 1 || !matches!(bind, "flickLayer" | "flickNode") {
            return None;
        }
        let b = ExprBuilder::new(span, "$pth");
        let expr = path_layer_template(&b);
        Some(self.expr(&expr, &Env::new(), block))
    }
}

fn path_layer_template(b: &ExprBuilder) -> Expr {
    let ops = b.at(ExprKind::Record(vec![
        ("sep".into(), b.lam(&["_"], str_lit(b, "/"))),
        ("basename".into(), rt(b, "Path.Basename")),
        ("basename_without".into(), rt(b, "Path.BasenameWithout")),
        ("dirname".into(), rt(b, "Path.Dirname")),
        ("extname".into(), rt(b, "Path.Extname")),
        ("format".into(), rt(b, "Path.Format")),
        (
            "from_file_url".into(),
            b.lam(
                &["url"],
                b.app(
                    rt(b, "Prelude.FromResult"),
                    b.app(rt(b, "Path.FromFileUrl"), b.loc("url")),
                ),
            ),
        ),
        ("is_absolute".into(), rt(b, "Path.IsAbsolute")),
        ("join".into(), rt(b, "Path.Join")),
        ("normalize".into(), rt(b, "Path.Normalize")),
        ("parse".into(), rt(b, "Path.Parse")),
        ("relative".into(), rt(b, "Path.Relative")),
        ("resolve".into(), rt(b, "Path.Resolve")),
        ("resolve_within".into(), rt(b, "Path.ResolveWithin")),
        (
            "to_file_url".into(),
            b.lam(
                &["p"],
                b.app(
                    rt(b, "Prelude.FromResult"),
                    b.app(rt(b, "Path.ToFileUrl"), b.loc("p")),
                ),
            ),
        ),
        ("to_namespaced_path".into(), rt(b, "Path.ToNamespacedPath")),
    ]));
    b.lam(&["tag"], b.app2(rt(b, "Layer.Succeed"), b.loc("tag"), ops))
}
