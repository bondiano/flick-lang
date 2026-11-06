//! `try … with` as a handler of effects: `| E.op p… -> h` is the arm for an
//! operation, `| return p -> h` the arm for the value of the body. The class
//! of the operation says what the arm is: a `tail` arm is the value of the
//! operation, an `abort` arm the value of the handler, and a `control` arm
//! the value of the handler with the rest of the body as `resume`, a
//! function of the handler's region, which therefore stays in the arm.

use super::Checker;
use super::env::{Form, Found};
use super::gadt::branch::{dict_local, field_given};
use super::patterns::is_pattern;
use super::regions::Scope;
use super::services::{OpClass, Operation};
use super::ty::{Effects, Row, Type, capture_label};
use super::typing::Ref;
use crate::diagnostics::codes;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

/// An effect a handler has arms for.
pub struct Handled {
    /// `M.E`, its label in a row.
    pub label: String,
    /// `E ā`, its entry in a row, the parameters as the arms see them.
    pub entry: Type,
    /// The number of parameters of the effect.
    params: usize,
    operations: Vec<Operation>,
    /// The operations with an arm without a guard.
    pub covered: Vec<String>,
}

impl Handled {
    /// Whether every operation has an arm without a guard: then the effect
    /// leaves the row of the body.
    pub fn complete(&self) -> bool {
        self.operations
            .iter()
            .all(|o| self.covered.contains(&o.name))
    }
}

/// The pattern of an arm.
pub fn arm_pattern(arm: &Node) -> Option<&Node> {
    arm.nodes().find(|n| is_pattern(n.kind))
}

/// `E.op` or `M.E.op` of a pattern naming an operation: a path whose last
/// segment is a name, which no constructor has.
pub fn operation_path(pattern: &Node, text: &str) -> Option<(String, String)> {
    if pattern.kind != K::ConPat {
        return None;
    }
    let segments: Vec<_> = pattern
        .child_tokens()
        .filter(|t| matches!(t.kind, K::Con | K::Ident))
        .collect();
    let (last, qualifier) = segments.split_last()?;
    if last.kind != K::Ident || qualifier.is_empty() {
        return None;
    }
    let qualifier = qualifier
        .iter()
        .map(|t| t.text(text))
        .collect::<Vec<_>>()
        .join(".");
    Some((qualifier, last.text(text).to_owned()))
}

impl Checker<'_> {
    /// Whether `arms` are those of a handler of effects: one of them is for
    /// an operation or for the value of the body.
    pub fn handles_effects(&self, arms: &[&Node]) -> bool {
        arms.iter().any(|arm| {
            arm.has_token(K::ReturnKw)
                || arm_pattern(arm).is_some_and(|p| operation_path(p, self.text).is_some())
        })
    }

    /// The effect and the operation `qualifier.name` of an arm, the effect
    /// joined to `handled` when it is new; `None` after a diagnostic.
    pub fn handled_operation(
        &mut self,
        handled: &mut Vec<Handled>,
        (qualifier, name, span): (&str, &str, Span),
    ) -> Option<(usize, Operation)> {
        let effect = match self.env.find_type(qualifier) {
            Found::Entity(e) => self
                .env
                .type_def(&e.module, &e.name)
                .cloned()
                .map(|d| (e, d)),
            Found::Unknown => return None,
            Found::Missing => None,
        };
        let Some((entity, def)) = effect else {
            self.error(
                codes::T001,
                span,
                format!("`{qualifier}` is no effect in scope"),
            );
            return None;
        };
        let Form::Service {
            params, operations, ..
        } = def.form
        else {
            self.error(
                codes::T001,
                span,
                format!("`{qualifier}.{name}` names no operation: `{qualifier}` is not an effect"),
            );
            return None;
        };
        let label = format!("{}.{}", entity.module, entity.name);
        let Some(op) = operations.iter().find(|o| o.name == name).cloned() else {
            self.error(
                codes::T001,
                span,
                format!("effect `{qualifier}` has no operation `{name}`"),
            );
            return None;
        };
        let stages = self.arrow_stages(&op.ty);
        self.record_name(
            span,
            Ref::Operation {
                service: label.clone(),
                name: name.to_owned(),
                stages,
            },
        );
        let index = if let Some(i) = handled.iter().position(|h| h.label == label) {
            i
        } else {
            let args: Vec<Type> = (0..params.len()).map(|_| self.fresh()).collect();
            handled.push(Handled {
                entry: Type::apps(Type::con(&entity.module, &entity.name), args),
                label,
                params: params.len(),
                operations,
                covered: Vec::new(),
            });
            handled.len() - 1
        };
        Some((index, op))
    }

    /// `| E.op p… [when g] -> h` of the handler whose value is `result`, with
    /// `resume` of the region `region` for a `control` arm; whether it
    /// covers the operation: it has no guard.
    pub fn operation_arm(
        &mut self,
        arm: &Node,
        handled: &Handled,
        op: &Operation,
        result: &Type,
        region: Option<&Type>,
    ) -> bool {
        let mark = self.locals.len();
        let frame = self.enter_arm();
        let (vars, constraints, body) = match &op.ty {
            Type::Forall {
                vars,
                constraints,
                body,
            } => (&vars[..], &constraints[..], (**body).clone()),
            other => (&[][..], &[][..], other.clone()),
        };
        // the effect's parameters are those of its entry; the operation's own
        // variables are any type: rigid in the arm
        let args = handled.entry.spine().1;
        let subst: Vec<(String, Type)> = vars
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let ty = match args.get(i).filter(|_| i < handled.params) {
                    Some(arg) => (*arg).clone(),
                    None => self.store.skolem(&v.name),
                };
                (v.name.clone(), ty)
            })
            .collect();
        let pattern = arm_pattern(arm);
        let start = pattern.map_or(arm.range().0, |p| p.range().0);
        if let Some(pattern) = pattern {
            self.typing
                .handled
                .insert(pattern.range(), (op.class, constraints.len()));
        }
        for (i, c) in constraints.iter().enumerate() {
            let given = field_given(&dict_local(start, i), c.substitute(&subst));
            self.classes.givens.push(given);
        }
        // the parameters up to the first runtime arrow, as the operation is called
        let mut params = Vec::new();
        let mut value = body.substitute(&subst);
        while let Type::Fun {
            param,
            effects,
            result,
        } = value
        {
            params.push(*param);
            value = *result;
            if effects.is_some() {
                break;
            }
        }
        let args: Vec<&Node> = pattern.map_or_else(Vec::new, |p| {
            p.nodes().filter(|n| is_pattern(n.kind)).collect()
        });
        if args.len() != params.len() {
            let span = pattern.map_or_else(|| self.span(arm), |p| self.span(p));
            self.error(
                codes::T018,
                span,
                format!(
                    "operation `{}` takes {} argument(s) before it runs, but the arm matches {}",
                    op.name,
                    params.len(),
                    args.len()
                ),
            );
        }
        for (arg, ty) in args.iter().zip(&params) {
            self.bind_pattern(arg, ty);
        }
        let mut guarded = false;
        for child in arm.nodes() {
            match child.kind {
                K::Guard => {
                    guarded = true;
                    if let Some(condition) = child.nodes().next() {
                        self.check(condition, &Type::lang(super::lang::Lang::Bool));
                    }
                }
                K::Block => match op.class {
                    OpClass::Tail => self.check(child, &value),
                    OpClass::Abort => self.check(child, result),
                    OpClass::Control => {
                        let resume = self.resume_type(&value, result, region);
                        self.push_local("resume", child.range().0, resume);
                        self.check(child, result);
                    }
                },
                _ => {}
            }
        }
        self.leave_arm(frame);
        self.locals.truncate(mark);
        !guarded
    }

    /// The type of `resume` in a `control` arm: the rest of the body, under
    /// this handler, needs what the handler needs, and holds its region.
    fn resume_type(&self, value: &Type, result: &Type, region: Option<&Type>) -> Type {
        let resume = Type::Fun {
            param: Box::new(value.clone()),
            effects: Some(Box::new(Effects::new(
                self.stage.services.clone(),
                self.stage.errors.clone(),
            ))),
            result: Box::new(result.clone()),
        };
        match region {
            Some(h) => Type::Captured {
                ty: Box::new(resume),
                captures: Box::new(Type::Row(Row {
                    entries: capture_label(h)
                        .map(|l| (l, h.clone()))
                        .into_iter()
                        .collect(),
                    tail: None,
                })),
                value: false,
            },
            None => resume,
        }
    }

    /// An arm for an operation not found, after its diagnostic: what it
    /// binds is unknown.
    pub fn unknown_operation_arm(&mut self, arm: &Node, result: &Type) {
        let mark = self.locals.len();
        if let Some(pattern) = arm_pattern(arm) {
            for arg in pattern.nodes().filter(|n| is_pattern(n.kind)) {
                self.bind_pattern(arg, &Type::Error);
            }
        }
        for child in arm.nodes() {
            match child.kind {
                K::Guard => {
                    if let Some(condition) = child.nodes().next() {
                        self.check(condition, &Type::lang(super::lang::Lang::Bool));
                    }
                }
                K::Block => self.check(child, result),
                _ => {}
            }
        }
        self.locals.truncate(mark);
    }

    /// The region of `resume` in the `control` arms of the handler `node`,
    /// when it has any: a skolem one level deeper, nested like a `scope`
    /// but owning nothing at run time.
    pub fn enter_resume(
        &mut self,
        node: &Node,
        span: Span,
        operations: &[Option<(usize, Operation)>],
    ) -> Option<Type> {
        if !operations
            .iter()
            .flatten()
            .any(|(_, op)| op.class == OpClass::Control)
        {
            return None;
        }
        self.store.level += 1;
        let region = self.store.skolem("resume");
        let Type::Skolem(id) = region else {
            unreachable!("a skolem")
        };
        self.regions.scopes.push(Scope {
            region: id,
            span,
            node: node.range(),
            outer: self.stage.services.clone(),
            parent: self.regions.nest.last().copied(),
            handler: true,
        });
        self.regions.nest.push(self.regions.scopes.len() - 1);
        Some(region)
    }

    /// After the arms: `resume` stays in them, so nothing of its region
    /// is in the value of the handler (G002).
    pub fn leave_resume(&mut self, ty: &Type, node: &Node) {
        self.regions.nest.pop();
        self.store.level -= 1;
        if let Err(skolem) = self.store.leave(ty) {
            let shown = self.show(ty);
            let head = format!("the value of this handler has type `{shown}`");
            self.region_escape(skolem, &head, self.span(node));
        }
    }
}
