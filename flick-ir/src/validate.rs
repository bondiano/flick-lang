//! Whether a `Program` holds together: what a plugin's pass over the IR of
//! a build gives back is checked before the compiler uses it. The check is
//! of form, not of types: every local bound where it is used, every global
//! of a module there is, every span of a file the build read.

use std::collections::BTreeSet;

use crate::{Expr, ExprKind, FileId, Handler, ItemKind, Pat, Program, Span, Stmt};

/// What is wrong with `program`, in order: a module named twice, a local
/// used where no binder is in scope, a global of a module neither in it nor
/// `external`, a span of a file not in `files`.
pub fn validate(
    program: &Program,
    external: &dyn Fn(&str) -> bool,
    files: &BTreeSet<FileId>,
) -> Vec<String> {
    let mut v = Validator {
        modules: program.modules.iter().map(|m| m.name.as_str()).collect(),
        external,
        files,
        module: "",
        scope: Vec::new(),
        vars: Vec::new(),
        loops: Vec::new(),
        problems: Vec::new(),
    };
    let mut seen = BTreeSet::new();
    for module in &program.modules {
        if !seen.insert(module.name.as_str()) {
            v.problems
                .push(format!("module `{}` is there twice", module.name));
        }
        v.module = &module.name;
        for item in &module.items {
            v.span(item.span);
            match &item.kind {
                ItemKind::Value { dicts, body, .. } => v.within(dicts, body),
                ItemKind::Destructure { body, .. } => v.expr(body),
                ItemKind::Instance { info, methods } => {
                    for (_, body) in methods {
                        v.within(&info.context, body);
                    }
                }
                ItemKind::Foreign(foreign) => {
                    if let Some(catch) = &foreign.catch {
                        v.expr(catch);
                    }
                }
                ItemKind::Constructor(_) | ItemKind::Service { .. } => {}
            }
        }
    }
    v.problems
}

struct Validator<'p> {
    modules: BTreeSet<&'p str>,
    external: &'p dyn Fn(&str) -> bool,
    files: &'p BTreeSet<FileId>,
    module: &'p str,
    /// The locals in scope, innermost last.
    scope: Vec<String>,
    /// The names `Var` binds, of every block entered; a name is a variable
    /// where its innermost binder in `scope` is one.
    vars: Vec<String>,
    loops: Vec<String>,
    problems: Vec<String>,
}

impl Validator<'_> {
    fn span(&mut self, span: Span) {
        if !self.files.contains(&span.file) {
            let file = span.file;
            let module = self.module;
            self.problems.push(format!(
                "a span of `{module}` is in no file of the build: {file:?}"
            ));
        }
    }

    /// `body` with `names` bound around it.
    fn within(&mut self, names: &[String], body: &Expr) {
        let mark = self.scope.len();
        self.scope.extend(names.iter().cloned());
        self.expr(body);
        self.scope.truncate(mark);
    }

    fn used(&mut self, name: &str) {
        if !self.scope.iter().any(|s| s == name) {
            let module = self.module;
            self.problems.push(format!(
                "`{module}` uses the local `{name}`, which nothing binds there"
            ));
        }
    }

    /// A read or an assignment of `name`, whose binders in scope are `Var`s.
    fn variable(&mut self, name: &str, what: &str) {
        let binders = self.scope.iter().filter(|s| *s == name).count();
        let vars = self.vars.iter().filter(|s| *s == name).count();
        if binders == 0 || binders != vars {
            let module = self.module;
            self.problems.push(format!(
                "`{module}` {what} `{name}`, which is not a variable in scope there"
            ));
        }
    }

    fn loop_exit(&mut self, id: &str, value: Option<&Expr>) {
        if self.loops.last().map(String::as_str) != Some(id) {
            let problem = if self.loops.iter().any(|loop_id| loop_id == id) {
                format!("exit does not target the nearest loop: `{id}`")
            } else {
                format!("exit targets no loop: `{id}`")
            };
            self.problems.push(problem);
        }
        if let Some(value) = value {
            self.expr(value);
        }
    }

    fn control_loop(&mut self, id: &str, condition: Option<&Expr>, body: &Expr) {
        self.loops.push(id.to_owned());
        if let Some(condition) = condition {
            self.expr(condition);
        }
        self.expr(body);
        self.loops.pop();
    }

    fn expr(&mut self, e: &Expr) {
        self.span(e.span);
        let (mark, vars) = (self.scope.len(), self.vars.len());
        match &e.kind {
            ExprKind::ControlLoop {
                id,
                condition,
                body,
                ..
            } => {
                self.control_loop(id, condition.as_deref(), body);
            }
            ExprKind::LoopExit { id, value } => self.loop_exit(id, value.as_deref()),
            ExprKind::Local(name) => self.used(name),
            ExprKind::Var(name) => self.variable(name, "reads"),
            ExprKind::Global { module, .. } => {
                if !self.modules.contains(module.as_str()) && !(self.external)(module) {
                    let here = self.module;
                    self.problems
                        .push(format!("`{here}` uses `{module}`, a module of no build"));
                }
            }
            ExprKind::Within(name, inner) => {
                self.used(name);
                self.expr(inner);
            }
            ExprKind::Abstract(names, inner) => self.within(names, inner),
            ExprKind::Lambda { params, body } => {
                for p in params {
                    bind(&p.pat, &mut self.scope);
                }
                self.expr(body);
            }
            ExprKind::Each {
                pat,
                source,
                body,
                stop,
            } => {
                if let Some(stop) = stop {
                    self.expr(stop);
                }
                self.expr(source);
                bind(pat, &mut self.scope);
                self.expr(body);
            }
            ExprKind::Match(scrutinee, arms) => {
                self.expr(scrutinee);
                for arm in arms {
                    self.arm(&arm.pat, arm.guard.as_ref(), &arm.body);
                }
            }
            ExprKind::Block(stmts, result) => {
                // a group of local functions may call one another
                for stmt in stmts {
                    if let Stmt::Fun {
                        name, rec: true, ..
                    } = stmt
                    {
                        self.scope.push(name.clone());
                    }
                }
                for stmt in stmts {
                    match stmt {
                        Stmt::Let(pat, value) => {
                            self.expr(value);
                            bind(pat, &mut self.scope);
                        }
                        Stmt::Var(name, value) => {
                            self.expr(value);
                            self.scope.push(name.clone());
                            self.vars.push(name.clone());
                        }
                        Stmt::Assign(name, value) => {
                            self.expr(value);
                            self.variable(name, "assigns");
                        }
                        Stmt::Fun {
                            name, dicts, value, ..
                        } => {
                            self.within(dicts, value);
                            self.scope.push(name.clone());
                        }
                        Stmt::Expr(value) => self.expr(value),
                    }
                }
                self.expr(result);
            }
            ExprKind::Handle(handler) => self.handler(handler),
            _ => e.each_child(&mut |child| self.expr(child)),
        }
        self.scope.truncate(mark);
        self.vars.truncate(vars);
    }

    fn arm(&mut self, pat: &Pat, guard: Option<&Expr>, body: &Expr) {
        let mark = self.scope.len();
        bind(pat, &mut self.scope);
        if let Some(guard) = guard {
            self.expr(guard);
        }
        self.expr(body);
        self.scope.truncate(mark);
    }

    fn handler(&mut self, handler: &Handler) {
        for handled in &handler.effects {
            for op in &handled.operations {
                self.expr(&op.function);
            }
        }
        self.expr(&handler.body);
        if let Some(ret) = &handler.ret {
            self.expr(ret);
        }
        for arm in &handler.errors {
            self.arm(&arm.pat, arm.guard.as_ref(), &arm.body);
        }
    }
}

/// Pushes the names `pat` binds onto `scope`.
fn bind(pat: &Pat, scope: &mut Vec<String>) {
    match pat {
        Pat::Bind(name) => scope.push(name.clone()),
        Pat::Tuple(items) | Pat::Con(_, items) => {
            for p in items {
                bind(p, scope);
            }
        }
        Pat::Or(alternatives) => {
            let common = alternatives
                .iter()
                .fold(None, |common: Option<BTreeSet<String>>, p| {
                    let mut names = Vec::new();
                    bind(p, &mut names);
                    let names: BTreeSet<_> = names.into_iter().collect();
                    Some(match common {
                        Some(common) => common.intersection(&names).cloned().collect(),
                        None => names,
                    })
                });
            scope.extend(common.into_iter().flatten());
        }
        Pat::List(items, rest) => {
            for p in items {
                bind(p, scope);
            }
            if let Some(rest) = rest {
                bind(rest, scope);
            }
        }
        Pat::Record(fields) => fields.iter().for_each(|(_, p)| bind(p, scope)),
        Pat::Wild | Pat::Lit(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::validate;
    use crate::{
        Arm, Expr, ExprKind, FileId, Item, ItemKind, Lit, Module, Param, Pat, Program, Span, Stage,
        Stmt,
    };

    fn at(kind: ExprKind) -> Expr {
        Expr::new(kind, Span::new(FileId::new(0), 0, 0))
    }

    fn program(body: Expr) -> Program {
        let item = Item {
            kind: ItemKind::Value {
                name: "f".into(),
                dicts: Vec::new(),
                body,
            },
            span: Span::new(FileId::new(0), 0, 0),
        };
        Program {
            ir_version: crate::VERSION.into(),
            compiler: "test".into(),
            files: Vec::new(),
            modules: vec![Module {
                name: "App.Main".into(),
                items: vec![item],
                labels: BTreeMap::default(),
                initialization: crate::Initialization::default(),
                records: BTreeMap::default(),
            }],
            lang: Vec::new(),
        }
    }

    #[test]
    fn a_local_needs_its_binder_and_a_global_its_module() {
        let files = BTreeSet::from([FileId::new(0)]);
        let external = |m: &str| m == "List";
        let x = Pat::Bind("x".into());
        let bound = at(ExprKind::Lambda {
            params: vec![Param {
                pat: x,
                stage: Stage::Pure,
                ty: None,
            }],
            body: Box::new(at(ExprKind::Local("x".into()))),
        });
        assert_eq!(
            validate(&program(bound), &external, &files),
            Vec::<String>::new()
        );
        let dangling = at(ExprKind::Local("y".into()));
        assert_eq!(
            validate(&program(dangling), &external, &files),
            ["`App.Main` uses the local `y`, which nothing binds there"]
        );
        let elsewhere = at(ExprKind::Global {
            module: "Nowhere".into(),
            name: "g".into(),
        });
        assert_eq!(
            validate(&program(elsewhere), &external, &files),
            ["`App.Main` uses `Nowhere`, a module of no build"]
        );
        let unit = at(ExprKind::Lit(Lit::Unit));
        assert_eq!(
            validate(&program(unit), &external, &BTreeSet::new()).len(),
            2
        );
    }

    #[test]
    fn a_variable_is_read_and_assigned_where_a_var_binds_it() {
        let files = BTreeSet::from([FileId::new(0)]);
        let one = || at(ExprKind::Lit(Lit::Int(1)));
        let block = |stmts, result| at(ExprKind::Block(stmts, Box::new(result)));
        let counted = block(
            vec![
                Stmt::Var("n".into(), one()),
                Stmt::Assign("n".into(), one()),
            ],
            at(ExprKind::Var("n".into())),
        );
        assert_eq!(
            validate(&program(counted), &|_| false, &files),
            Vec::<String>::new()
        );
        let immutable = block(
            vec![
                Stmt::Let(Pat::Bind("n".into()), one()),
                Stmt::Assign("n".into(), one()),
            ],
            at(ExprKind::Var("n".into())),
        );
        assert_eq!(
            validate(&program(immutable), &|_| false, &files),
            [
                "`App.Main` assigns `n`, which is not a variable in scope there",
                "`App.Main` reads `n`, which is not a variable in scope there",
            ]
        );
        let outside = block(
            vec![Stmt::Expr(block(vec![Stmt::Var("n".into(), one())], one()))],
            at(ExprKind::Var("n".into())),
        );
        assert_eq!(
            validate(&program(outside), &|_| false, &files),
            ["`App.Main` reads `n`, which is not a variable in scope there"]
        );
    }

    #[test]
    fn an_or_pattern_binds_only_names_in_every_alternative() {
        let body = at(ExprKind::Match(
            Box::new(at(ExprKind::Lit(Lit::Unit))),
            vec![Arm {
                pat: Pat::Or(vec![Pat::Bind("x".into()), Pat::Wild]),
                guard: None,
                body: at(ExprKind::Local("x".into())),
            }],
        ));
        let files = BTreeSet::from([FileId::new(0)]);
        assert_eq!(
            validate(&program(body), &|_| false, &files),
            ["`App.Main` uses the local `x`, which nothing binds there"]
        );
    }
    #[test]
    fn exits_target_the_nearest_loop() {
        let files = BTreeSet::from([FileId::new(0)]);
        let exit = || {
            at(ExprKind::LoopExit {
                id: "outer".into(),
                value: None,
            })
        };
        let looping = |id: &str, body| {
            at(ExprKind::ControlLoop {
                id: id.into(),
                condition: None,
                body: Box::new(body),
                iterable: false,
            })
        };
        assert_eq!(
            validate(&program(looping("outer", exit())), &|_| false, &files),
            Vec::<String>::new()
        );
        assert_eq!(
            validate(&program(exit()), &|_| false, &files),
            ["exit targets no loop: `outer`"]
        );
        let nested = looping("outer", looping("inner", exit()));
        assert_eq!(
            validate(&program(nested), &|_| false, &files),
            ["exit does not target the nearest loop: `outer`"]
        );
    }
}
