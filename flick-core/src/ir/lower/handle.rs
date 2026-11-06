//! A handler of effects: the arms of a `try` for operations become one
//! function per operation, as a field of `S.Impl` is, and `provide S impl`
//! hands its record over as it is.

use super::Lower;
use crate::ir::{
    Arm, Expr, ExprKind, Handled, Handler, Lit, OpArm, Param, Pat, Stage, Stmt, operation_arity,
};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::handle::{arm_pattern, operation_path};
use crate::types::{Dict, OpClass, Ref, dict_local, is_pattern};

/// The arms of one operation, in order.
struct Group<'n> {
    effect: String,
    name: String,
    class: OpClass,
    stages: Vec<Stage>,
    dicts: usize,
    covers: bool,
    arms: Vec<&'n Node>,
}

impl Lower<'_> {
    /// `try body with …` with arms for operations, or `provide S impl { body }`.
    pub(super) fn handler(&mut self, node: &Node) -> ExprKind {
        let children: Vec<&Node> = node.nodes().collect();
        if node.kind == K::ProvideExpr {
            let (Some(operand), Some(body)) = (children.first(), children.get(1)) else {
                return ExprKind::Lit(Lit::Unit);
            };
            let mut parts = operand.nodes();
            let (Some(head), Some(record)) = (parts.next(), parts.next()) else {
                return ExprKind::Lit(Lit::Unit);
            };
            let Some(Ref::ServiceTag(effect)) = self.typing.names.get(&head.range()).cloned()
            else {
                return ExprKind::Lit(Lit::Unit);
            };
            return self.record_handler(node, effect, record, body);
        }
        let live: Vec<&Node> = node
            .nodes()
            .filter(|n| n.kind == K::MatchArm && !self.typing.dead_arms.contains(&n.range()))
            .collect();
        let mut groups: Vec<Group> = Vec::new();
        let mut ret = None;
        let mut errors = Vec::new();
        for arm in live {
            if arm.has_token(K::ReturnKw) {
                ret = Some(self.value_function(arm));
                continue;
            }
            match self.operation_of(arm) {
                Some((effect, name, stages, class, dicts, covers)) => {
                    match groups
                        .iter_mut()
                        .find(|g| g.effect == effect && g.name == name)
                    {
                        Some(group) => {
                            group.covers |= covers;
                            group.arms.push(arm);
                        }
                        None => groups.push(Group {
                            effect,
                            name,
                            class,
                            stages,
                            dicts,
                            covers,
                            arms: vec![arm],
                        }),
                    }
                }
                None => errors.push(self.arm(arm)),
            }
        }
        let mut effects: Vec<Handled> = Vec::new();
        for group in &groups {
            let entry = OpArm {
                name: group.name.clone(),
                class: group.class,
                dicts: group.dicts,
                arity: operation_arity(&group.stages),
                covers: group.covers,
                function: self.operation_function(group, self.span(node)),
            };
            match effects.iter_mut().find(|h| h.effect == group.effect) {
                Some(handled) => handled.operations.push(entry),
                None => effects.push(Handled {
                    effect: group.effect.clone(),
                    operations: vec![entry],
                }),
            }
        }
        let body = match children.first() {
            Some(body) => self.expr(body),
            None => Expr::new(ExprKind::Lit(Lit::Unit), self.span(node)),
        };
        ExprKind::Handle(Box::new(Handler {
            effects,
            body,
            ret,
            errors,
        }))
    }

    /// `provide S impl { body }`: each operation of `S` by the field of
    /// `impl`, evaluated before the body, as a `tail` arm.
    fn record_handler(
        &mut self,
        node: &Node,
        effect: String,
        record: &Node,
        body: &Node,
    ) -> ExprKind {
        let span = self.span(node);
        let at = |kind| Expr::new(kind, span);
        let record = self.expr(record);
        let local = self.hidden("$impl");
        let ops = self
            .typing
            .provided_ops
            .get(&node.range())
            .cloned()
            .unwrap_or_default();
        let mut operations = Vec::new();
        for (name, dicts, stages) in ops {
            let arity = operation_arity(&stages);
            let keys: Vec<String> = (0..dicts)
                .map(|i| format!("$impl{}_{i}", span.start))
                .collect();
            let args: Vec<String> = (0..arity).map(|_| self.hidden("$arg")).collect();
            let field = at(ExprKind::Field(
                Box::new(at(ExprKind::Local(local.clone()))),
                name.clone(),
            ));
            let field = if keys.is_empty() {
                field
            } else {
                at(ExprKind::Dicts(
                    Box::new(field),
                    keys.iter().map(|k| Dict::Given(k.clone())).collect(),
                ))
            };
            let call = args.iter().zip(&stages).fold(field, |f, (x, stage)| {
                at(ExprKind::App {
                    func: Box::new(f),
                    arg: Box::new(at(ExprKind::Local(x.clone()))),
                    stage: *stage,
                    arg_first: false,
                })
            });
            let params = args
                .iter()
                .zip(&stages)
                .map(|(x, stage)| Param {
                    pat: Pat::Bind(x.clone()),
                    stage: *stage,
                    ty: None,
                })
                .collect();
            let function = at(ExprKind::Lambda {
                params,
                body: Box::new(call),
            });
            let function = if keys.is_empty() {
                function
            } else {
                at(ExprKind::Abstract(keys, Box::new(function)))
            };
            operations.push(OpArm {
                name,
                class: OpClass::Tail,
                dicts,
                arity,
                covers: true,
                function,
            });
        }
        let handle = ExprKind::Handle(Box::new(Handler {
            effects: vec![Handled { effect, operations }],
            body: self.expr(body),
            ret: None,
            errors: Vec::new(),
        }));
        ExprKind::Block(
            vec![Stmt::Let(Pat::Bind(local), record)],
            Box::new(at(handle)),
        )
    }

    /// The effect, operation, stages, class and number of dictionaries of
    /// an arm for an operation, and whether it covers the operation.
    #[allow(clippy::type_complexity)]
    fn operation_of(
        &self,
        arm: &Node,
    ) -> Option<(String, String, Vec<Stage>, OpClass, usize, bool)> {
        let pattern = arm_pattern(arm)?;
        operation_path(pattern, self.text)?;
        let mut names = pattern
            .child_tokens()
            .filter(|t| matches!(t.kind, K::Con | K::Ident | K::Dot));
        let first = names.next()?;
        let end = names.last().map_or(first.end, |t| t.end);
        let Some(Ref::Operation {
            service,
            name,
            stages,
        }) = self.typing.names.get(&(first.start, end)).cloned()
        else {
            return None;
        };
        let (class, dicts, covers) = *self.typing.handled.get(&pattern.range())?;
        Some((service, name, stages, class, dicts, covers))
    }

    /// `| return p -> body`: a function of the body's value.
    fn value_function(&mut self, arm: &Node) -> Expr {
        let span = self.span(arm);
        let mark = self.scope.len();
        let pat = arm_pattern(arm).map_or(Pat::Wild, |p| self.pattern(p));
        let body = arm.nodes().find(|n| n.kind == K::Block).map_or_else(
            || Expr::new(ExprKind::Lit(Lit::Unit), span),
            |b| self.expr(b),
        );
        self.scope.truncate(mark);
        Expr::new(
            ExprKind::Lambda {
                params: vec![Param {
                    pat,
                    stage: Stage::Runtime,
                    ty: None,
                }],
                body: Box::new(body),
            },
            span,
        )
    }

    /// The function of an operation's arms: of its dictionaries, then of
    /// its arguments, and for `control` of `resume`. An argument no arm
    /// matches goes on to the handler around, as the operation itself.
    fn operation_function(&mut self, group: &Group, span: Span) -> Expr {
        let at = |kind| Expr::new(kind, span);
        let arity = operation_arity(&group.stages);
        // the dictionaries, as constraints the function is given: those the
        // arms hold as their patterns' are renamed to them
        let first = group
            .arms
            .first()
            .and_then(|a| arm_pattern(a))
            .map_or(0, |p| p.range().0);
        let dicts: Vec<String> = (0..group.dicts).map(|i| dict_local(first, i)).collect();
        let args: Vec<String> = (0..arity).map(|_| self.hidden("$arg")).collect();
        let mark = self.scope.len();
        let resume = (group.class == OpClass::Control).then(|| {
            let start = group.arms.first().map_or(0, |a| a.range().0);
            self.bind("resume", start)
        });
        let mut arms: Vec<Arm> = Vec::new();
        for arm in &group.arms {
            let mut lowered = self.operation_arm(arm, arity);
            let start = arm_pattern(arm).map_or(0, |p| p.range().0);
            let given: Vec<(String, String)> = dicts
                .iter()
                .enumerate()
                .map(|(i, d)| (dict_local(start, i), d.clone()))
                .collect();
            if !given.is_empty() {
                if let Some(g) = &mut lowered.guard {
                    as_given(g, &given);
                }
                as_given(&mut lowered.body, &given);
            }
            arms.push(lowered);
        }
        // arms that take every argument leave the operation nothing to go on with
        if !group.covers {
            let forwarded = forward(group, &dicts, &args, resume.as_deref(), span);
            arms.push(Arm {
                pat: Pat::Wild,
                guard: None,
                body: forwarded,
            });
        }
        self.scope.truncate(mark);
        let scrutinee = match &args[..] {
            [x] => at(ExprKind::Local(x.clone())),
            xs => at(ExprKind::Tuple(
                xs.iter().map(|x| at(ExprKind::Local(x.clone()))).collect(),
            )),
        };
        let body = at(ExprKind::Match(Box::new(scrutinee), arms));
        let mut params: Vec<Param> = args
            .iter()
            .zip(&group.stages)
            .map(|(x, stage)| Param {
                pat: Pat::Bind(x.clone()),
                stage: if resume.is_some() {
                    Stage::Pure
                } else {
                    *stage
                },
                ty: None,
            })
            .collect();
        if let Some(resume) = resume {
            params.push(Param {
                pat: Pat::Bind(resume),
                stage: Stage::Runtime,
                ty: None,
            });
        }
        let function = at(ExprKind::Lambda {
            params,
            body: Box::new(body),
        });
        if dicts.is_empty() {
            function
        } else {
            at(ExprKind::Abstract(dicts, Box::new(function)))
        }
    }

    /// An arm for an operation: its arguments as one pattern, a tuple of
    /// them unless it takes one.
    fn operation_arm(&mut self, arm: &Node, arity: usize) -> Arm {
        let span = self.span(arm);
        let mark = self.scope.len();
        let args: Vec<Pat> = arm_pattern(arm).map_or_else(Vec::new, |p| {
            p.nodes()
                .filter(|n| is_pattern(n.kind))
                .map(|n| self.pattern(n))
                .collect()
        });
        let pat = match args.len() {
            1 if arity == 1 => args.into_iter().next().unwrap_or(Pat::Wild),
            _ => Pat::Tuple(args),
        };
        let mut guard = None;
        let mut body = None;
        for child in arm.nodes() {
            match child.kind {
                K::Guard => guard = child.nodes().next().map(|g| self.expr(g)),
                K::Block => body = Some(self.expr(child)),
                _ => {}
            }
        }
        self.scope.truncate(mark);
        Arm {
            pat,
            guard,
            body: body.unwrap_or_else(|| Expr::new(ExprKind::Lit(Lit::Unit), span)),
        }
    }
}

/// `e` with each dictionary an arm's pattern holds, `Field` of a local of
/// `given`, as the constraint the operation's function is given instead.
fn as_given(e: &mut Expr, given: &[(String, String)]) {
    fn dict(d: &mut Dict, given: &[(String, String)]) {
        match d {
            Dict::Field(local) => {
                if let Some((_, to)) = given.iter().find(|(from, _)| from == local) {
                    *d = Dict::Given(to.clone());
                }
            }
            Dict::Instance { args: dicts, .. } | Dict::Structural { fields: dicts, .. } => {
                for d in dicts {
                    dict(d, given);
                }
            }
            Dict::Super { dict: inner, .. } => dict(inner, given),
            Dict::Given(_) | Dict::Primitive { .. } | Dict::Site { .. } | Dict::Error => {}
        }
    }
    match &mut e.kind {
        ExprKind::Dicts(_, dicts) => {
            for d in dicts {
                dict(d, given);
            }
        }
        ExprKind::Method { dict: d, .. } => dict(d, given),
        _ => {}
    }
    e.each_child_mut(&mut |c| as_given(c, given));
}

/// The operation itself, performed where the handler is, for arguments no
/// arm matches; for `control`, the rest of the body resumes with its value.
fn forward(
    group: &Group,
    dicts: &[String],
    args: &[String],
    resume: Option<&str>,
    span: Span,
) -> Expr {
    let at = |kind| Expr::new(kind, span);
    let operation = at(ExprKind::Operation {
        service: group.effect.clone(),
        name: group.name.clone(),
        stages: group.stages.clone(),
    });
    let operation = if dicts.is_empty() {
        operation
    } else {
        let given = dicts.iter().map(|d| Dict::Given(d.clone())).collect();
        at(ExprKind::Dicts(Box::new(operation), given))
    };
    let performed = args
        .iter()
        .zip(&group.stages)
        .fold(operation, |f, (x, stage)| {
            at(ExprKind::App {
                func: Box::new(f),
                arg: Box::new(at(ExprKind::Local(x.clone()))),
                stage: *stage,
                arg_first: false,
            })
        });
    match resume {
        Some(resume) => at(ExprKind::App {
            func: Box::new(at(ExprKind::Local(resume.to_owned()))),
            arg: Box::new(performed),
            stage: Stage::Runtime,
            arg_first: false,
        }),
        None => performed,
    }
}
