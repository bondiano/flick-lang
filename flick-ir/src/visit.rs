//! Traversal of direct children and source spans in evaluation order.
use crate::{Expr, ExprKind, ItemKind, Module, Span, Stage, Stmt};

impl Expr {
    /// How often the expression reads the local `name`.
    pub fn uses(&self, name: &str) -> usize {
        let here = matches!(&self.kind, ExprKind::Local(l) | ExprKind::Within(l, _) if l == name);
        let mut n = usize::from(here);
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
            ExprKind::Binary(_, l, r) => {
                f(l);
                f(r);
            }
            ExprKind::Handle(h) => h.each_child(f),
            ExprKind::Lit(_)
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
            ExprKind::Binary(_, l, r) => {
                f(l);
                f(r);
            }
            ExprKind::Handle(h) => h.each_child_mut(f),
            ExprKind::Lit(_)
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
    use crate::{Expr, ExprKind, FileId, Lit, Span, Stage};

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
}
