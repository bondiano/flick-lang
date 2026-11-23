//! Whether a `Program` holds together: what a plugin's pass or optimization
//! over the IR gives back is checked before the compiler uses it.
//!
//! Checks both form (unique modules, bound locals and variables, valid globals
//! and loop exits, spans in files of the build, owner evidence) and types
//! (well-formed IR types, type argument arity, agreement of layout-relevant
//! annotations across constructors, binders, calls, literals and results).
//! Missing (`None` / empty) type annotations stay valid for erased paths.

mod agree;
mod decls;
mod exprs;
mod patterns;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod typed_tests;

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    Constraint, Expr, ExprKind, FileId, Handler, ItemKind, Pat, Program, Span, Stage, Stmt, TypeRef,
};
use decls::TypeIndex;

/// What is wrong with `program`, in order: module/form errors and type
/// well-formedness or consistency errors.
pub fn validate(
    program: &Program,
    external: &dyn Fn(&str) -> bool,
    files: &BTreeSet<FileId>,
) -> Vec<String> {
    let type_index = TypeIndex::build(program);
    let mut v = Validator {
        modules: program.modules.iter().map(|m| m.name.as_str()).collect(),
        external,
        files,
        types: &type_index,
        module: "",
        scope: Vec::new(),
        vars: Vec::new(),
        local_types: Vec::new(),
        var_types: Vec::new(),
        loops: Vec::new(),
        gadt_depth: 0,
        problems: Vec::new(),
    };
    let mut seen = BTreeSet::new();
    for module in &program.modules {
        if !seen.insert(module.name.as_str()) {
            v.problems
                .push(format!("module `{}` is there twice", module.name));
        }
        v.module = &module.name;
        v.check_module_decls(module);
        for item in &module.items {
            v.span(item.span);
            match &item.kind {
                ItemKind::Value {
                    name,
                    type_params,
                    dicts,
                    dict_constraints,
                    ty,
                    body,
                } => {
                    v.check_value_item(
                        name,
                        type_params,
                        dicts,
                        dict_constraints,
                        ty.as_ref(),
                        body,
                    );
                    v.within(dicts, body);
                }
                ItemKind::Destructure {
                    pattern,
                    binder_types,
                    body,
                } => {
                    let body_ty = v.effective_expr_type(body);
                    v.check_pat_binders(pattern, binder_types, body_ty.as_ref());
                    v.expr(body);
                }
                ItemKind::Instance { info, methods } => {
                    for (_, body) in methods {
                        v.within(&info.context, body);
                    }
                }
                ItemKind::Foreign(foreign) => {
                    if let Some(ty) = &foreign.ty {
                        v.check_type_wf(ty, None);
                    }
                    if let Some(catch) = &foreign.catch {
                        v.expr(catch);
                    }
                }
                ItemKind::Constructor(c) => v.check_constructor_item(c),
                ItemKind::Service { .. } => {}
            }
        }
    }
    v.problems
}

/// Invocation operands must be captured before installing contextual evidence.
fn evidence_invocation(invocation: &Expr) -> bool {
    let evaluated = |e: &Expr| matches!(e.kind, ExprKind::Local(_) | ExprKind::Lit(_));
    match &invocation.kind {
        ExprKind::App {
            func, arg, stage, ..
        } => *stage != Stage::Pure && evaluated(func) && evaluated(arg),
        ExprKind::Await(task) => evaluated(task),
        _ => false,
    }
}

struct LocalBinding {
    name: String,
    ty: Option<TypeRef>,
    /// Non-empty when bound by a polymorphic local `Stmt::Fun`.
    type_params: Vec<String>,
}

struct Validator<'p> {
    modules: BTreeSet<&'p str>,
    external: &'p dyn Fn(&str) -> bool,
    files: &'p BTreeSet<FileId>,
    types: &'p TypeIndex<'p>,
    module: &'p str,
    /// The locals in scope, innermost last.
    scope: Vec<String>,
    /// The names `Var` binds, of every block entered; a name is a variable
    /// where its innermost binder in `scope` is one.
    vars: Vec<String>,
    /// Typed local bindings in scope, parallel to `scope`.
    local_types: Vec<LocalBinding>,
    /// Typed `Var` bindings in scope, parallel to `vars`.
    var_types: Vec<(String, Option<TypeRef>)>,
    loops: Vec<String>,
    /// Depth inside match arms on indexed (GADT) constructors where rigid
    /// type variables may be refined by local equalities.
    gadt_depth: usize,
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

    fn push_local(&mut self, name: String, ty: Option<TypeRef>, type_params: Vec<String>) {
        self.scope.push(name.clone());
        self.local_types.push(LocalBinding {
            name,
            ty,
            type_params,
        });
    }

    fn push_pat(&mut self, pat: &Pat, binder_types: &[(String, TypeRef)]) {
        let map: BTreeMap<&str, &TypeRef> =
            binder_types.iter().map(|(n, t)| (n.as_str(), t)).collect();
        let mut names = Vec::new();
        bind(pat, &mut names);
        for name in names {
            let ty = map.get(name.as_str()).copied().cloned();
            self.push_local(name, ty, Vec::new());
        }
    }

    /// `body` with `names` bound around it (untyped dictionary binders).
    fn within(&mut self, names: &[String], body: &Expr) {
        let mark = self.scope.len();
        for name in names {
            self.push_local(name.clone(), None, Vec::new());
        }
        self.expr(body);
        self.scope.truncate(mark);
        self.local_types.truncate(mark);
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
        if !self.loops.iter().any(|loop_id| loop_id == id) {
            self.problems.push(format!("exit targets no loop: `{id}`"));
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

    #[allow(clippy::too_many_lines)]
    fn expr(&mut self, e: &Expr) {
        self.span(e.span);
        if let Some(ty) = &e.ty {
            self.check_type_wf(ty, None);
        }
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
            ExprKind::Local(name) => {
                self.used(name);
                self.check_local_use(name, e.ty.as_ref());
            }
            ExprKind::Var(name) => {
                self.variable(name, "reads");
                self.check_var_use(name, e.ty.as_ref());
            }
            ExprKind::Global {
                module,
                name,
                type_args,
            } => {
                if !self.modules.contains(module.as_str()) && !(self.external)(module) {
                    let here = self.module;
                    self.problems
                        .push(format!("`{here}` uses `{module}`, a module of no build"));
                }
                self.check_global_use(module, name, type_args, e.ty.as_ref());
            }
            ExprKind::Method { type_args, .. } => {
                for a in type_args {
                    self.check_type_wf_arg(a, None);
                }
            }
            ExprKind::Con(c) => {
                self.check_con_use(c, e.ty.as_ref());
            }
            ExprKind::Within(name, inner) => {
                self.used(name);
                self.expr(inner);
            }
            ExprKind::WithOwnerEvidence(owner, invocation) => {
                if !evidence_invocation(invocation) {
                    self.problems.push(
                        "owner evidence requires an immediate non-pure call or await with evaluated operands".into(),
                    );
                }
                self.expr(owner);
                self.expr(invocation);
            }
            ExprKind::Abstract(names, inner) => self.within(names, inner),
            ExprKind::Lambda { params, body } => {
                for p in params {
                    if let Some(ty) = &p.ty {
                        self.check_type_wf(ty, None);
                    }
                    self.check_pat_binders(&p.pat, &p.binder_types, p.ty.as_ref());
                    if p.binder_types.is_empty()
                        && let Pat::Bind(name) = &p.pat
                    {
                        self.push_local(name.clone(), p.ty.clone(), Vec::new());
                    } else {
                        self.push_pat(&p.pat, &p.binder_types);
                    }
                }
                self.expr(body);
                self.check_lambda(params, body, e.ty.as_ref());
            }
            ExprKind::Each {
                pat,
                binder_types,
                source,
                body,
                stop,
            } => {
                if let Some(stop) = stop {
                    self.expr(stop);
                }
                self.expr(source);
                self.check_pat_binders(pat, binder_types, None);
                self.push_pat(pat, binder_types);
                self.expr(body);
            }
            ExprKind::Match(scrutinee, arms) => {
                self.expr(scrutinee);
                let scrut_ty = self.effective_expr_type(scrutinee);
                for arm in arms {
                    self.arm(
                        &arm.pat,
                        &arm.binder_types,
                        arm.guard.as_ref(),
                        &arm.body,
                        scrut_ty.as_ref(),
                        e.ty.as_ref(),
                    );
                }
            }
            ExprKind::Block(stmts, result) => {
                // a group of local functions may call one another
                for stmt in stmts {
                    if let Stmt::Fun {
                        name,
                        rec: true,
                        type_params,
                        ty,
                        ..
                    } = stmt
                    {
                        self.push_local(name.clone(), ty.clone(), type_params.clone());
                    }
                }
                for stmt in stmts {
                    match stmt {
                        Stmt::Let {
                            pat,
                            binder_types,
                            value,
                        } => {
                            self.expr(value);
                            let val_ty = self.effective_expr_type(value);
                            self.check_pat_binders(pat, binder_types, val_ty.as_ref());
                            if binder_types.is_empty()
                                && let Pat::Bind(name) = pat
                            {
                                self.push_local(name.clone(), val_ty, Vec::new());
                            } else {
                                self.push_pat(pat, binder_types);
                            }
                        }
                        Stmt::Var { name, ty, value } => {
                            if let Some(ty) = ty {
                                self.check_type_wf(ty, None);
                            }
                            self.expr(value);
                            let val_ty = self.effective_expr_type(value);
                            let effective = ty.clone().or_else(|| val_ty.clone());
                            if let (Some(decl), Some(actual)) = (ty, &val_ty) {
                                self.expect_agree(
                                    decl,
                                    actual,
                                    &format!("variable `{name}` initializer"),
                                );
                            }
                            self.push_local(name.clone(), effective.clone(), Vec::new());
                            self.vars.push(name.clone());
                            self.var_types.push((name.clone(), effective));
                        }
                        Stmt::Assign(name, value) => {
                            self.expr(value);
                            self.variable(name, "assigns");
                            self.check_assign(name, value);
                        }
                        Stmt::Fun {
                            name,
                            type_params,
                            dicts,
                            dict_constraints,
                            ty,
                            value,
                            ..
                        } => {
                            self.check_fun_stmt(
                                name,
                                type_params,
                                dicts,
                                dict_constraints,
                                ty.as_ref(),
                                value,
                            );
                            self.within(dicts, value);
                            self.push_local(
                                name.clone(),
                                ty.clone().or_else(|| value.ty.clone()),
                                type_params.clone(),
                            );
                        }
                        Stmt::Expr(value) => self.expr(value),
                    }
                }
                self.expr(result);
                if let (Some(bt), Some(rt)) = (&e.ty, self.effective_expr_type(result)) {
                    self.expect_agree(bt, &rt, "block result");
                }
            }
            ExprKind::Handle(handler) => self.handler(handler),
            _ => {
                e.each_child(&mut |child| self.expr(child));
                self.check_expr_consistency(e);
            }
        }
        self.scope.truncate(mark);
        self.local_types.truncate(mark);
        self.vars.truncate(vars);
        self.var_types.truncate(vars);
    }

    fn arm(
        &mut self,
        pat: &Pat,
        binder_types: &[(String, TypeRef)],
        guard: Option<&Expr>,
        body: &Expr,
        scrutinee_ty: Option<&TypeRef>,
        match_ty: Option<&TypeRef>,
    ) {
        let mark = self.scope.len();
        let is_gadt = self.pat_is_gadt(pat);
        if is_gadt {
            self.gadt_depth += 1;
        }
        self.check_pat_binders(pat, binder_types, scrutinee_ty);
        self.push_pat(pat, binder_types);
        if let Some(guard) = guard {
            self.expr(guard);
            if let Some(gt) = self.effective_expr_type(guard) {
                self.check_bool(&gt, "match guard");
            }
        }
        self.expr(body);
        if let (Some(mt), Some(bt)) = (match_ty, self.effective_expr_type(body)) {
            self.expect_agree(mt, &bt, "match arm body");
        }
        if is_gadt {
            self.gadt_depth -= 1;
        }
        self.scope.truncate(mark);
        self.local_types.truncate(mark);
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
            self.arm(
                &arm.pat,
                &arm.binder_types,
                arm.guard.as_ref(),
                &arm.body,
                None,
                None,
            );
        }
    }

    fn check_fun_stmt(
        &mut self,
        name: &str,
        _type_params: &[String],
        dicts: &[String],
        dict_constraints: &[Constraint],
        ty: Option<&TypeRef>,
        value: &Expr,
    ) {
        if !dict_constraints.is_empty() && dict_constraints.len() != dicts.len() {
            let module = self.module;
            self.problems.push(format!(
                "`{module}` local function `{name}` has {} dictionary constraints for {} dictionaries",
                dict_constraints.len(),
                dicts.len()
            ));
        }
        for c in dict_constraints {
            for a in &c.args {
                self.check_type_wf(a, None);
            }
        }
        if let Some(ty) = ty {
            self.check_type_wf(ty, None);
            if let Some(vty) = self.effective_expr_type(value) {
                self.expect_agree(ty, &vty, &format!("local function `{name}` body"));
            }
        }
    }
}

/// Pushes the names `pat` binds onto `scope`.
pub(super) fn bind(pat: &Pat, scope: &mut Vec<String>) {
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
