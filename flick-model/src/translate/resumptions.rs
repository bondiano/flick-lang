//! Normalize immutable continuation aliases and beta-reduce their known callers
//! before assigning the loop completion protocol. Evaluation stays left to right.
use std::collections::{HashMap, HashSet, VecDeque};

use super::Tr;
use crate::syntax::Path;
use flick_core::{
    ir::{self, ExprKind, Pat, Stmt},
    opt,
    source::Span,
};

impl Tr {
    pub(super) fn normalize_resumptions(&mut self, body: &ir::Expr, id: &str) -> ir::Expr {
        let roots = self
            .resumptions
            .iter()
            .filter(|(_, target)| target == id)
            .map(|(name, _)| name.clone())
            .collect();
        Normalize {
            tr: self,
            roots,
            aliases: HashMap::new(),
            active: Vec::new(),
        }
        .expr(body)
    }
}

struct Normalize<'a> {
    tr: &'a mut Tr,
    roots: HashSet<String>,
    aliases: HashMap<String, ir::Expr>,
    active: Vec<Path>,
}

fn mentions(e: &ir::Expr, roots: &HashSet<String>) -> bool {
    if matches!(&e.kind, ExprKind::Local(n) if roots.contains(n)) {
        return true;
    }
    let mut found = false;
    e.each_child(&mut |c| found |= mentions(c, roots));
    found
}

impl Normalize<'_> {
    /// A strict prefix of a known lambda only constructs a closure. Merely
    /// being pure is insufficient: a full application can diverge or die.
    fn value(&self, e: &ir::Expr) -> bool {
        match &e.kind {
            ExprKind::Local(_) | ExprKind::Global { .. } | ExprKind::Lambda { .. } => true,
            ExprKind::Abstract(_, e) | ExprKind::Lift(e, _) => self.value(e),
            ExprKind::App { .. } => {
                let mut head = e;
                let mut count = 0;
                while let ExprKind::App {
                    func,
                    arg,
                    stage: ir::Stage::Pure,
                    ..
                } = &head.kind
                {
                    if !self.value(arg) {
                        return false;
                    }
                    count += 1;
                    head = func;
                }
                let function = match &head.kind {
                    ExprKind::Lambda { .. } => head,
                    ExprKind::Global { module, name } => {
                        let Some(f) = self.tr.functions.get(&Path::new(module, name)) else {
                            return false;
                        };
                        f
                    }
                    _ => return false,
                };
                matches!(&function.kind, ExprKind::Lambda { params, .. } if params.len() > count)
            }
            _ => false,
        }
    }
    fn value_tail(&self, e: &ir::Expr) -> bool {
        if let ExprKind::Block(_, tail) = &e.kind {
            return self.value_tail(tail);
        }
        self.value(e)
    }
    fn expr(&mut self, e: &ir::Expr) -> ir::Expr {
        if let ExprKind::Local(n) = &e.kind
            && let Some(alias) = self.aliases.get(n)
        {
            return alias.clone();
        }
        if let ExprKind::Block(stmts, tail) = &e.kind {
            return self.block(stmts, tail, e.span);
        }
        let mut out = e.clone();
        out.each_child_mut(&mut |c| *c = self.expr(c));
        if matches!(out.kind, ExprKind::App { .. })
            && mentions(&out, &self.roots)
            && let Some(inlined) = self.application(&out)
        {
            return inlined;
        }
        out
    }
    fn block(&mut self, stmts: &[Stmt], tail: &ir::Expr, span: Span) -> ir::Expr {
        let saved = self.aliases.clone();
        let mut out = Vec::new();
        for stmt in stmts {
            if let Stmt::Fun {
                name,
                rec: false,
                dicts,
                value,
            } = stmt
                && dicts.is_empty()
                && matches!(value.kind, ExprKind::Lambda { .. })
            {
                let value = self.expr(value);
                self.aliases.insert(name.clone(), value);
                continue;
            }
            if let Stmt::Let(p, e) = stmt {
                let mut e = self.expr(e);
                if let Pat::Bind(name) = p
                    && self.value_tail(&e)
                {
                    while let ExprKind::Block(stmts, tail) = e.kind {
                        out.extend(stmts);
                        e = *tail;
                    }
                    self.aliases.insert(name.clone(), e);
                } else {
                    let mut bound = Vec::new();
                    p.binders(&mut bound);
                    for name in bound {
                        self.aliases.remove(name);
                    }
                    out.push(Stmt::Let(p.clone(), e));
                }
            } else {
                let mut stmt = stmt.clone();
                let e = match &mut stmt {
                    Stmt::Let(_, e)
                    | Stmt::Var(_, e)
                    | Stmt::Assign(_, e)
                    | Stmt::Expr(e)
                    | Stmt::Fun { value: e, .. } => e,
                };
                *e = self.expr(e);
                out.push(stmt);
            }
        }
        let tail = self.expr(tail);
        self.aliases = saved;
        if out.is_empty() {
            tail
        } else {
            ir::Expr::new(ExprKind::Block(out, Box::new(tail)), span)
        }
    }
    fn application(&mut self, e: &ir::Expr) -> Option<ir::Expr> {
        let mut head = e;
        let mut args = Vec::new();
        while let ExprKind::App {
            func,
            arg,
            arg_first,
            ..
        } = &head.kind
        {
            args.push(((**arg).clone(), *arg_first));
            head = func;
        }
        args.reverse();
        let (key, mut function) = match &head.kind {
            ExprKind::Lambda { .. } => (None, head.clone()),
            ExprKind::Global { module, name } => {
                let key = Path::new(module, name);
                if self.active.contains(&key) {
                    return None;
                }
                (Some(key.clone()), self.tr.functions.get(&key)?.clone())
            }
            _ => return None,
        };
        let ExprKind::Lambda { params, .. } = &function.kind else {
            return None;
        };
        if params.len() != args.len() {
            return None;
        }
        let suffix = self.tr.fresh("resume_inline").to_string();
        opt::rename(&mut function, &suffix);
        let ExprKind::Lambda { params, body } = function.kind else {
            unreachable!()
        };
        let names: Vec<_> = args
            .iter()
            .map(|_| self.tr.fresh("resume_arg").to_string())
            .collect();
        let mut order = VecDeque::new();
        for (i, (_, first)) in args.iter().enumerate() {
            if *first {
                order.push_front(i);
            } else {
                order.push_back(i);
            }
        }
        let mut captured: Vec<_> = order
            .into_iter()
            .map(|i| Stmt::Let(Pat::Bind(names[i].clone()), args[i].0.clone()))
            .collect();
        captured.extend(params.into_iter().zip(names).map(|(param, name)| {
            Stmt::Let(param.pat, ir::Expr::new(ExprKind::Local(name), e.span))
        }));
        let expanded = ir::Expr::new(ExprKind::Block(captured, body), e.span);
        if let Some(key) = &key {
            self.active.push(key.clone());
        }
        let result = self.expr(&expanded);
        if key.is_some() {
            self.active.pop();
        }
        Some(result)
    }
}
