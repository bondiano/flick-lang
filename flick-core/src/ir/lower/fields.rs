//! Paths of fields and accessors. A path with an optional step,
//! `user.?address.city` or `.?address.city`, lowers into `match`es on
//! `Option`: a `None` ends the path with `None`, and `Some` of the last
//! field is its value, so the IR, the model and the backends see no form
//! of their own. The root is read once, and no step after a `None`.

use super::Lower;
use super::enums::native;
use super::expr::with_dicts;
use super::numeric::apply;
use crate::ir::{Arm, Expr, ExprKind, Param, Pat, Stage, TypeRef};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::fields;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::Lang;
use crate::types::{Constructor, StageKey, accessor_keys};

impl Lower<'_> {
    /// A field; a polymorphic one with constraints takes their dictionaries.
    pub(super) fn field(&mut self, node: &Node, record: &Node) -> Expr {
        let (root, steps) = fields::steps(node);
        if let Some(root) = root.filter(|_| steps.iter().any(|s| fields::is_optional(s))) {
            let span = self.span(node);
            let value = self.expr(root);
            let ty = self.ir_type(node.range());
            let at = |step: &Node| Span::new(span.file, span.start, step.range().1);
            return self.read_steps(value, &steps, &at, ty);
        }
        let record = self.expr(record);
        self.read_field(record, node, self.span(node))
    }

    /// `record.label` with the label written in `label`: the field takes
    /// the dictionaries its instance needs there, keyed by that node.
    fn read_field(&self, record: Expr, label: &Node, span: Span) -> Expr {
        let name = self.token_text(label, K::Ident).unwrap_or_default();
        let field = Expr::new(ExprKind::Field(Box::new(record), name), span);
        let dicts = (self.typing.dicts.get(&label.range()))
            .cloned()
            .unwrap_or_default();
        with_dicts(field, dicts)
    }

    /// What `steps` read from `value`, spanned by `at`. Up to the first
    /// optional step they are plain reads; that step matches the value so
    /// far, and the rest of the path reads from its `Some`. Each `match`
    /// has the type `ty` of the whole path.
    fn read_steps(
        &mut self,
        mut value: Expr,
        steps: &[&Node],
        at: &dyn Fn(&Node) -> Span,
        ty: Option<TypeRef>,
    ) -> Expr {
        for (i, step) in steps.iter().enumerate() {
            if fields::is_optional(step) {
                let payload = (step.child_tokens())
                    .find(|t| t.kind == K::Question)
                    .and_then(|mark| self.ir_type((mark.start, mark.end)));
                let name = self.hidden("$some");
                let span = at(step);
                let some = Expr::with_ty(ExprKind::Local(name.clone()), span, payload.clone());
                let read = self.read_field(some, step, span);
                let rest = self.read_steps(read, &steps[i + 1..], at, ty.clone());
                return option_match(value, name, payload, rest, span, ty);
            }
            value = self.read_field(value, step, at(step));
        }
        // the path had an optional step before these plain ones
        let item = ty.as_deref().and_then(option_item);
        let span = value.span;
        let con = ExprKind::Con(Constructor {
            fields: item.iter().cloned().collect(),
            type_args: item.iter().cloned().collect(),
            ..native(Lang::Some, Lang::Option)
        });
        let mut some = apply(Expr::new(con, span), [Box::new(value)], span);
        some.ty = ty;
        some
    }

    /// `.a.b` as `fun x -> x.a.b`, with an `x` no source can name. Each
    /// step reads a field of the one before, with the dictionaries its
    /// instance takes, as a field does.
    pub(super) fn accessor(&mut self, node: &Node) -> Expr {
        let span = self.span(node);
        let (param, result) = accessor_keys(node);
        let entry = StageKey::Arrow {
            node: node.range(),
            index: 0,
        };
        self.register_owner_entry(entry);
        let x = self.hidden("$x");
        let value = Expr::new(ExprKind::Local(x.clone()), span);
        let steps: Vec<&Node> = node.nodes().collect();
        // as the span of `x.a.b` is that of the whole path so far
        let at = |step: &Node| Span::new(span.file, node.range().0, step.range().1);
        let result_ty = match self.ir_type(node.range()).as_deref() {
            Some(flick_ir::IrType::Fun { result, .. }) => Some(result.clone()),
            _ => None,
        };
        let mut value = if steps.iter().any(|s| fields::is_optional(s)) {
            self.read_steps(value, &steps, &at, result_ty.clone())
        } else {
            (steps.iter()).fold(value, |value, step| self.read_field(value, step, at(step)))
        };
        let stage = self
            .typing
            .stages
            .get(&node.range())
            .and_then(|s| s.first());
        // the last read has the type of the accessor's result
        if value.ty.is_none() {
            value.ty = result_ty;
        }
        let body = self.adapted(result, value);
        let ty = self.ir_type(param);
        let binder_types = (ty.clone())
            .map(|t| vec![(x.clone(), t)])
            .unwrap_or_default();
        let lambda = ExprKind::Lambda {
            params: vec![Param {
                pat: Pat::Bind(x),
                stage: stage.copied().unwrap_or(Stage::Pure),
                ty,
                binder_types,
            }],
            body: Box::new(self.owner_entry(entry, body)),
        };
        Expr::new(lambda, span)
    }
}

/// `match value with | Some name -> some | None -> None`, of type `ty`,
/// where `name` has the type `payload`.
fn option_match(
    value: Expr,
    name: String,
    payload: Option<TypeRef>,
    some: Expr,
    span: Span,
    ty: Option<TypeRef>,
) -> Expr {
    let type_args: Vec<TypeRef> = payload.iter().cloned().collect();
    let some_pat = Pat::Con(
        Constructor {
            fields: type_args.clone(),
            type_args: type_args.clone(),
            ..native(Lang::Some, Lang::Option)
        },
        vec![Pat::Bind(name.clone())],
    );
    let none = || Constructor {
        arity: 0,
        type_args: type_args.clone(),
        ..native(Lang::None, Lang::Option)
    };
    let result_args: Vec<TypeRef> = ty.as_deref().and_then(option_item).into_iter().collect();
    let none_value = Expr::with_ty(
        ExprKind::Con(Constructor {
            type_args: result_args,
            ..none()
        }),
        span,
        ty.clone(),
    );
    let arms = vec![
        Arm {
            pat: some_pat,
            binder_types: payload.map(|t| vec![(name, t)]).unwrap_or_default(),
            guard: None,
            body: some,
        },
        Arm {
            pat: Pat::Con(none(), Vec::new()),
            binder_types: Vec::new(),
            guard: None,
            body: none_value,
        },
    ];
    Expr::with_ty(ExprKind::Match(Box::new(value), arms), span, ty)
}

/// The item type of an `Option` type.
fn option_item(ty: &flick_ir::IrType) -> Option<TypeRef> {
    match ty {
        flick_ir::IrType::Named { args, .. } if args.len() == 1 => Some(args[0].clone()),
        _ => None,
    }
}
