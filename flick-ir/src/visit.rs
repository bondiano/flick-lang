//! Traversal of direct children and source spans in evaluation order.
use crate::{Dict, Expr, ExprKind, ItemKind, Module, Span, Stage, Stmt};

impl Expr {
    /// How often the expression reads the local `name`.
    pub fn uses(&self, name: &str) -> usize {
        fn dictionary(dict: &Dict, name: &str) -> usize {
            match dict {
                Dict::Field(local) | Dict::Opened { binder: local, .. } => {
                    usize::from(local == name)
                }
                Dict::Instance { args, .. } | Dict::Structural { fields: args, .. } => {
                    args.iter().map(|dict| dictionary(dict, name)).sum()
                }
                Dict::Super { dict, .. } => dictionary(dict, name),
                Dict::Given(_)
                | Dict::Value { .. }
                | Dict::Primitive { .. }
                | Dict::Site { .. }
                | Dict::Error => 0,
            }
        }
        let here = matches!(&self.kind, ExprKind::Local(l) | ExprKind::Within(l, _) if l == name);
        let mut n = usize::from(here);
        n += match &self.kind {
            ExprKind::Method { dict, .. } => dictionary(dict, name),
            ExprKind::Dicts(_, dicts) => dicts.iter().map(|dict| dictionary(dict, name)).sum(),
            _ => 0,
        };
        self.each_child(&mut |c| n += c.uses(name));
        n
    }

    /// `f a b`, applications none of which evaluates its argument first, as
    /// the function and the arguments with their stages, in order.
    pub fn spine(&self) -> (&Expr, Vec<(&Expr, Stage)>) {
        let mut args = Vec::new();
        let mut head = self;
        while let ExprKind::App {
            func,
            arg,
            stage,
            arg_first: false,
        } = &head.kind
        {
            args.push((&**arg, *stage));
            head = func;
        }
        args.reverse();
        (head, args)
    }

    /// The [`spine`](Self::spine) of the pure applications around the
    /// first one that is not: its head is that application, if any.
    pub fn pure_spine(&self) -> (&Expr, Vec<&Expr>) {
        let mut args = Vec::new();
        let mut head = self;
        while let ExprKind::App {
            func,
            arg,
            stage: Stage::Pure,
            arg_first: false,
        } = &head.kind
        {
            args.push(&**arg);
            head = func;
        }
        args.reverse();
        (head, args)
    }

    /// Calls `f` on each direct subexpression, in evaluation order.
    pub fn each_child<'a>(&'a self, f: &mut dyn FnMut(&'a Expr)) {
        match &self.kind {
            ExprKind::Dicts(inner, _)
            | ExprKind::Abstract(_, inner)
            | ExprKind::Field(inner, _)
            | ExprKind::Not(inner)
            | ExprKind::Unwrap(inner)
            | ExprKind::Neg(_, inner)
            | ExprKind::Ignore(inner)
            | ExprKind::Await(inner)
            | ExprKind::Task(inner)
            | ExprKind::Within(_, inner)
            | ExprKind::Lift(inner, _)
            | ExprKind::Lambda { body: inner, .. }
            | ExprKind::Yield(inner) => f(inner),
            ExprKind::Generate { body, limit, .. }
            | ExprKind::ControlLoop {
                body,
                condition: limit,
                ..
            } => {
                if let Some(limit) = limit {
                    f(limit);
                }
                f(body);
            }
            ExprKind::LoopExit { .. } | ExprKind::Loop { .. } | ExprKind::Each { .. } => {
                self.each_loop_child(f);
            }
            ExprKind::App {
                func,
                arg,
                arg_first,
                ..
            } => {
                if *arg_first {
                    f(arg);
                    f(func);
                } else {
                    f(func);
                    f(arg);
                }
            }
            ExprKind::Block(stmts, result) => {
                for stmt in stmts {
                    match stmt {
                        Stmt::Let(_, v)
                        | Stmt::Var(_, v)
                        | Stmt::Assign(_, v)
                        | Stmt::Expr(v)
                        | Stmt::Fun { value: v, .. } => f(v),
                    }
                }
                f(result);
            }
            ExprKind::If(c, t, e) => {
                f(c);
                f(t);
                f(e);
            }
            ExprKind::Match(s, arms) => {
                f(s);
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        f(g);
                    }
                    f(&arm.body);
                }
            }
            ExprKind::Tuple(items) | ExprKind::List(items) => {
                for item in items {
                    f(item);
                }
            }
            ExprKind::Record(fields) | ExprKind::Dictionary(fields) => {
                for (_, v) in fields {
                    f(v);
                }
            }
            ExprKind::Update(r, fields) => {
                f(r);
                for (_, v) in fields {
                    f(v);
                }
            }
            ExprKind::Binary(_, l, r) | ExprKind::WithOwnerEvidence(l, r) => {
                f(l);
                f(r);
            }
            ExprKind::Handle(h) => h.each_child(f),
            ExprKind::Lit(_)
            | ExprKind::ReadOwnerEvidence
            | ExprKind::Var(_)
            | ExprKind::Local(_)
            | ExprKind::Global { .. }
            | ExprKind::Method { .. }
            | ExprKind::Con(_)
            | ExprKind::Perform { .. }
            | ExprKind::ServiceTag(_)
            | ExprKind::Label(_) => {}
        }
    }
}

impl Module {
    /// Calls `f` on the span of every item and expression.
    pub fn each_span_mut(&mut self, f: &mut dyn FnMut(&mut Span)) {
        for item in &mut self.items {
            f(&mut item.span);
            match &mut item.kind {
                ItemKind::Value { body, .. } | ItemKind::Destructure { body, .. } => {
                    body.each_span_mut(f);
                }
                ItemKind::Instance { methods, .. } => {
                    for (_, body) in methods {
                        body.each_span_mut(f);
                    }
                }
                ItemKind::Foreign(foreign) => {
                    if let Some(catch) = &mut foreign.catch {
                        catch.each_span_mut(f);
                    }
                }
                ItemKind::Constructor(_) | ItemKind::Service { .. } => {}
            }
        }
    }
}

impl Expr {
    /// Calls `f` on the span of this expression and of every one inside.
    pub fn each_span_mut(&mut self, f: &mut dyn FnMut(&mut Span)) {
        f(&mut self.span);
        self.each_child_mut(&mut |child| child.each_span_mut(f));
    }

    /// Calls `f` on each direct subexpression, mutably, in evaluation order.
    pub fn each_child_mut(&mut self, f: &mut dyn FnMut(&mut Expr)) {
        match &mut self.kind {
            ExprKind::Dicts(inner, _)
            | ExprKind::Abstract(_, inner)
            | ExprKind::Field(inner, _)
            | ExprKind::Not(inner)
            | ExprKind::Unwrap(inner)
            | ExprKind::Neg(_, inner)
            | ExprKind::Ignore(inner)
            | ExprKind::Await(inner)
            | ExprKind::Task(inner)
            | ExprKind::Within(_, inner)
            | ExprKind::Lift(inner, _)
            | ExprKind::Lambda { body: inner, .. }
            | ExprKind::Yield(inner) => f(inner),
            ExprKind::Generate { body, limit, .. }
            | ExprKind::ControlLoop {
                body,
                condition: limit,
                ..
            } => {
                if let Some(limit) = limit {
                    f(limit);
                }
                f(body);
            }
            ExprKind::LoopExit { .. } | ExprKind::Loop { .. } | ExprKind::Each { .. } => {
                self.each_loop_child_mut(f);
            }
            ExprKind::App {
                func,
                arg,
                arg_first,
                ..
            } => {
                if *arg_first {
                    f(arg);
                    f(func);
                } else {
                    f(func);
                    f(arg);
                }
            }
            ExprKind::Block(stmts, result) => {
                for stmt in stmts {
                    match stmt {
                        Stmt::Let(_, v)
                        | Stmt::Var(_, v)
                        | Stmt::Assign(_, v)
                        | Stmt::Expr(v)
                        | Stmt::Fun { value: v, .. } => f(v),
                    }
                }
                f(result);
            }
            ExprKind::If(c, t, e) => {
                f(c);
                f(t);
                f(e);
            }
            ExprKind::Match(s, arms) => {
                f(s);
                for arm in arms {
                    if let Some(g) = &mut arm.guard {
                        f(g);
                    }
                    f(&mut arm.body);
                }
            }
            ExprKind::Tuple(items) | ExprKind::List(items) => {
                for item in items {
                    f(item);
                }
            }
            ExprKind::Record(fields) | ExprKind::Dictionary(fields) => {
                for (_, v) in fields {
                    f(v);
                }
            }
            ExprKind::Update(r, fields) => {
                f(r);
                for (_, v) in fields {
                    f(v);
                }
            }
            ExprKind::Binary(_, l, r) | ExprKind::WithOwnerEvidence(l, r) => {
                f(l);
                f(r);
            }
            ExprKind::Handle(h) => h.each_child_mut(f),
            ExprKind::Lit(_)
            | ExprKind::ReadOwnerEvidence
            | ExprKind::Var(_)
            | ExprKind::Local(_)
            | ExprKind::Global { .. }
            | ExprKind::Method { .. }
            | ExprKind::Con(_)
            | ExprKind::Perform { .. }
            | ExprKind::ServiceTag(_)
            | ExprKind::Label(_) => {}
        }
    }
}

impl Expr {
    fn each_loop_child<'a>(&'a self, f: &mut dyn FnMut(&'a Expr)) {
        match &self.kind {
            ExprKind::LoopExit { value, .. } => {
                if let Some(value) = value {
                    f(value);
                }
            }
            ExprKind::Loop { condition, body } => {
                f(condition);
                f(body);
            }
            ExprKind::Each {
                source, body, stop, ..
            } => {
                f(source);
                if let Some(stop) = stop {
                    f(stop);
                }
                f(body);
            }
            _ => unreachable!("loop traversal"),
        }
    }
}

impl Expr {
    fn each_loop_child_mut(&mut self, f: &mut dyn FnMut(&mut Expr)) {
        match &mut self.kind {
            ExprKind::LoopExit { value, .. } => {
                if let Some(value) = value {
                    f(value);
                }
            }
            ExprKind::Loop { condition, body } => {
                f(condition);
                f(body);
            }
            ExprKind::Each {
                source, body, stop, ..
            } => {
                f(source);
                if let Some(stop) = stop {
                    f(stop);
                }
                f(body);
            }
            _ => unreachable!("loop traversal"),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{Dict, Expr, ExprKind, FileId, Lit, Span, Stage};

    fn at() -> Span {
        Span::new(FileId::new(0), 0, 0)
    }

    fn app(func: Expr, arg: i32, stage: Stage) -> Expr {
        let arg = Box::new(Expr::new(ExprKind::Lit(Lit::Int(arg)), at()));
        let kind = ExprKind::App {
            func: Box::new(func),
            arg,
            stage,
            arg_first: false,
        };
        Expr::new(kind, at())
    }

    /// `f 1 2 3` with its second stage running: the arguments in order, and
    /// the pure ones only up to that stage.
    #[test]
    fn a_spine_is_the_head_and_the_arguments_in_order() {
        let f = Expr::new(ExprKind::Local("f".to_owned()), at());
        let e = app(
            app(app(f, 1, Stage::Pure), 2, Stage::Runtime),
            3,
            Stage::Pure,
        );
        let (head, args) = e.spine();
        assert!(matches!(&head.kind, ExprKind::Local(f) if f == "f"));
        let found: Vec<_> = (args.iter()).map(|(a, s)| (a.kind.clone(), *s)).collect();
        let lit = |n| ExprKind::Lit(Lit::Int(n));
        let expected = [
            (lit(1), Stage::Pure),
            (lit(2), Stage::Runtime),
            (lit(3), Stage::Pure),
        ];
        assert_eq!(found, expected);
        let (head, args) = e.pure_spine();
        assert!(matches!(
            head.kind,
            ExprKind::App {
                stage: Stage::Runtime,
                ..
            }
        ));
        assert_eq!(args.len(), 1);
    }

    #[test]
    fn dictionary_metadata_reads_are_counted_through_every_nested_dictionary_shape() {
        let dict = Dict::Instance {
            module: "Test".into(),
            name: "instance".into(),
            args: vec![
                Dict::Structural {
                    class: "Base".into(),
                    fields: vec![
                        Dict::Field("shared".into()),
                        Dict::Super {
                            class: "Parent".into(),
                            dict: Box::new(Dict::Opened {
                                binder: "shared".into(),
                                field: "dictionary".into(),
                            }),
                        },
                        Dict::Given("shared".into()),
                        Dict::Value {
                            module: "shared".into(),
                            name: "dictionary".into(),
                        },
                        Dict::Primitive {
                            constraint: "shared".into(),
                            labels: vec!["shared".into()],
                        },
                        Dict::Site {
                            place: "shared".into(),
                            definition: "shared".into(),
                        },
                        Dict::Error,
                    ],
                },
                Dict::Opened {
                    binder: "other".into(),
                    field: "shared".into(),
                },
            ],
        };
        let applied = Expr::new(
            ExprKind::Dicts(
                Box::new(Expr::new(ExprKind::Local("shared".into()), at())),
                vec![dict.clone(), Dict::Field("shared".into())],
            ),
            at(),
        );
        assert_eq!(applied.uses("shared"), 4);
        assert_eq!(applied.uses("other"), 1);
        let method = Expr::new(
            ExprKind::Method {
                dict,
                name: "shared".into(),
            },
            at(),
        );
        assert_eq!(method.uses("shared"), 2);
        assert_eq!(method.uses("other"), 1);
    }

    #[test]
    fn owner_evidence_children_and_dictionary_metadata_are_each_counted_once() {
        let expression = Expr::new(
            ExprKind::WithOwnerEvidence(
                Box::new(Expr::new(ExprKind::Local("shared".into()), at())),
                Box::new(Expr::new(
                    ExprKind::Method {
                        dict: Dict::Field("shared".into()),
                        name: "read".into(),
                    },
                    at(),
                )),
            ),
            at(),
        );
        assert_eq!(expression.uses("shared"), 2);
        assert_eq!(expression.uses("absent"), 0);
    }
}
