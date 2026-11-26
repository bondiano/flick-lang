//! Genuine layout-aware fixture backend / evaluator over [`SpecializedProgram`].

use std::collections::BTreeMap;
use std::rc::Rc;

use flick_ir::Lit;

use super::super::{
    SpecArm, SpecExpr, SpecExprKind, SpecParam, SpecStmt, SpecializedProgram, SymbolId,
};
use super::eval_ops::{eval_binary, match_pat, unbox_if_boxed};
use crate::layout::TypeRepr;

#[derive(Debug, Clone)]
pub(crate) enum Val {
    Unit,
    Bool(bool),
    Int(i32),
    Float(f64),
    String(String),
    Tuple(Vec<Val>),
    List(Vec<Val>),
    RecordSlots(Vec<(String, Val)>),
    RecordMap(BTreeMap<String, Val>),
    Con {
        name: String,
        fields: Vec<Val>,
        dicts: Vec<DictVal>,
    },
    Boxed(Box<Val>),
    Dict(DictVal),
    Closure(Rc<ClosureVal>),
    PartialCon {
        name: String,
        arity: usize,
        fields: Vec<Val>,
        dicts: Vec<DictVal>,
    },
    ForeignFn {
        module: String,
        name: String,
        arity: usize,
        polymorphic: bool,
        applied: Vec<Val>,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct DictVal {
    pub methods: BTreeMap<String, Val>,
    pub supers: BTreeMap<String, DictVal>,
}

#[derive(Debug, Clone)]
pub(crate) struct ClosureVal {
    pub dict_params: Vec<String>,
    pub params: Vec<SpecParam>,
    pub body: SpecExpr,
    pub env: Env,
    pub applied_dicts: Vec<DictVal>,
    pub applied_args: Vec<Val>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Env {
    pub locals: BTreeMap<String, Val>,
    pub dicts: BTreeMap<String, DictVal>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct EvalStats {
    pub runtime_boxes: usize,
    pub runtime_unboxes: usize,
    pub offset_reads: usize,
    pub label_reads: usize,
    pub direct_calls: usize,
    pub fallback_calls: usize,
    pub runtime_dict_calls: usize,
}

type ForeignHook = Box<dyn Fn(&str, &str, Vec<Val>) -> Val>;

pub(crate) struct FixtureEval<'a> {
    pub program: &'a SpecializedProgram,
    pub stats: EvalStats,
    pub foreign_hook: Option<ForeignHook>,
}

impl<'a> FixtureEval<'a> {
    pub(crate) fn new(program: &'a SpecializedProgram) -> Self {
        FixtureEval {
            program,
            stats: EvalStats::default(),
            foreign_hook: None,
        }
    }

    pub(crate) fn with_foreign(
        program: &'a SpecializedProgram,
        hook: impl Fn(&str, &str, Vec<Val>) -> Val + 'static,
    ) -> Self {
        FixtureEval {
            program,
            stats: EvalStats::default(),
            foreign_hook: Some(Box::new(hook)),
        }
    }

    pub(crate) fn eval_root(&mut self, module: &str, name: &str) -> Val {
        let sym = self
            .program
            .roots
            .get(&(module.to_owned(), name.to_owned()))
            .unwrap_or_else(|| panic!("missing root `{module}.{name}`"))
            .clone();
        self.eval_symbol(&sym, Vec::new())
    }

    pub(crate) fn eval_symbol(&mut self, sym: &SymbolId, dict_vals: Vec<DictVal>) -> Val {
        let def = self
            .program
            .definitions
            .get(sym)
            .unwrap_or_else(|| panic!("missing specialized def `{sym}`"));
        let mut env = Env::default();
        for (dname, dval) in def.runtime_dicts.iter().zip(dict_vals) {
            env.dicts.insert(dname.clone(), dval);
        }
        let body = def.body.clone();
        self.eval_expr(&body, &mut env)
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn eval_expr(&mut self, expr: &SpecExpr, env: &mut Env) -> Val {
        match &expr.kind {
            SpecExprKind::Box { expr: inner, .. } => {
                let v = self.eval_expr(inner, env);
                self.stats.runtime_boxes += 1;
                Val::Boxed(Box::new(v))
            }
            SpecExprKind::Unbox { expr: inner, .. } => {
                let v = self.eval_expr(inner, env);
                self.stats.runtime_unboxes += 1;
                match v {
                    Val::Boxed(b) => *b,
                    other => panic!("Unbox expected Val::Boxed, got {other:?}"),
                }
            }
            SpecExprKind::CallDirect { target, dicts } => {
                self.stats.direct_calls += 1;
                let dvals = dicts.iter().map(|d| self.eval_dict(d, env)).collect();
                self.eval_symbol(target, dvals)
            }
            SpecExprKind::CallFallback { target, dicts } => {
                self.stats.fallback_calls += 1;
                let dvals = dicts.iter().map(|d| self.eval_dict(d, env)).collect();
                self.eval_symbol(target, dvals)
            }
            SpecExprKind::CallForeign {
                target,
                module,
                name,
                polymorphic,
                ..
            } => {
                let arity = self
                    .program
                    .foreigns
                    .get(target)
                    .map_or(1, |f| f.foreign.taken().max(1));
                Val::ForeignFn {
                    module: module.clone(),
                    name: name.clone(),
                    arity,
                    polymorphic: *polymorphic,
                    applied: Vec::new(),
                }
            }
            SpecExprKind::MethodCall { dict, name } => {
                self.stats.runtime_dict_calls += 1;
                let dval = self.eval_dict(dict, env);
                dval.methods[name].clone()
            }
            SpecExprKind::FieldOffset {
                record,
                label,
                index,
                offset,
                ..
            } => {
                self.stats.offset_reads += 1;
                if let Some(rec_ty) = &record.ty
                    && let Some(layout) = self.program.layouts.get(rec_ty)
                    && let TypeRepr::Record(rl) = &layout.repr
                {
                    assert_eq!(rl.fields[*index].label, *label);
                    assert_eq!(rl.fields[*index].field.offset, *offset);
                }
                match self.eval_expr(record, env) {
                    Val::RecordSlots(slots) => slots[*index].1.clone(),
                    other => panic!("FieldOffset expected RecordSlots, got {other:?}"),
                }
            }
            SpecExprKind::FieldByLabel { record, label } => {
                self.stats.label_reads += 1;
                match unbox_if_boxed(self.eval_expr(record, env)) {
                    Val::RecordMap(map) => map[label].clone(),
                    Val::RecordSlots(slots) => {
                        slots
                            .into_iter()
                            .find(|(l, _)| l == label)
                            .expect("missing field")
                            .1
                    }
                    other => panic!("FieldByLabel expected record, got {other:?}"),
                }
            }
            SpecExprKind::Construct { ctor, dicts, .. } => {
                let dvals = dicts.iter().map(|d| self.eval_dict(d, env)).collect();
                if ctor.arity == 0 {
                    Val::Con {
                        name: ctor.name.clone(),
                        fields: Vec::new(),
                        dicts: dvals,
                    }
                } else {
                    Val::PartialCon {
                        name: ctor.name.clone(),
                        arity: ctor.arity,
                        fields: Vec::new(),
                        dicts: dvals,
                    }
                }
            }
            SpecExprKind::Lit(lit) => match lit {
                Lit::Unit => Val::Unit,
                Lit::Bool(b) => Val::Bool(*b),
                Lit::Int(n) => Val::Int(*n),
                Lit::Float(f) => Val::Float(*f),
                Lit::String(s) => Val::String(s.clone()),
                other => panic!("unsupported literal: {other:?}"),
            },
            SpecExprKind::Local(name) | SpecExprKind::Var(name) => env.locals[name].clone(),
            SpecExprKind::Abstract(names, inner) => Val::Closure(Rc::new(ClosureVal {
                dict_params: names.clone(),
                params: Vec::new(),
                body: (**inner).clone(),
                env: env.clone(),
                applied_dicts: Vec::new(),
                applied_args: Vec::new(),
            })),
            SpecExprKind::Dicts(inner, dicts) => {
                let target = self.eval_expr(inner, env);
                let dvals = dicts.iter().map(|d| self.eval_dict(d, env)).collect();
                self.apply_dicts(target, dvals)
            }
            SpecExprKind::Lambda { params, body } => Val::Closure(Rc::new(ClosureVal {
                dict_params: Vec::new(),
                params: params.clone(),
                body: (**body).clone(),
                env: env.clone(),
                applied_dicts: Vec::new(),
                applied_args: Vec::new(),
            })),
            SpecExprKind::App {
                func,
                arg,
                arg_first,
                ..
            } => {
                let (fv, av) = if *arg_first {
                    let a = self.eval_expr(arg, env);
                    let f = self.eval_expr(func, env);
                    (f, a)
                } else {
                    let f = self.eval_expr(func, env);
                    let a = self.eval_expr(arg, env);
                    (f, a)
                };
                self.apply_val(fv, av)
            }
            SpecExprKind::Block(stmts, tail) => self.eval_block(stmts, tail, env),
            SpecExprKind::If(c, t, e) => {
                let Val::Bool(b) = unbox_if_boxed(self.eval_expr(c, env)) else {
                    panic!("If condition must be Bool");
                };
                if b {
                    self.eval_expr(t, env)
                } else {
                    self.eval_expr(e, env)
                }
            }
            SpecExprKind::Match(scrut, arms) => {
                let sv = self.eval_expr(scrut, env);
                self.eval_match(&sv, arms, env)
            }
            SpecExprKind::Tuple(items) => {
                Val::Tuple(items.iter().map(|i| self.eval_expr(i, env)).collect())
            }
            SpecExprKind::List(items) => {
                Val::List(items.iter().map(|i| self.eval_expr(i, env)).collect())
            }
            SpecExprKind::Record { fields, boxed } => {
                let mut pairs: Vec<(String, Val)> = fields
                    .iter()
                    .map(|(l, e)| (l.clone(), self.eval_expr(e, env)))
                    .collect();
                if *boxed {
                    Val::RecordMap(pairs.into_iter().collect())
                } else {
                    flick_ir::sort_by_utf16(&mut pairs, |(l, _)| l.as_str());
                    Val::RecordSlots(pairs)
                }
            }
            SpecExprKind::Dictionary(fields) => {
                let methods = fields
                    .iter()
                    .map(|(l, e)| (l.clone(), self.eval_expr(e, env)))
                    .collect();
                Val::Dict(DictVal {
                    methods,
                    supers: BTreeMap::new(),
                })
            }
            SpecExprKind::Binary(op, lhs, rhs) => {
                let l = self.eval_expr(lhs, env);
                let r = self.eval_expr(rhs, env);
                let was_boxed = matches!(l, Val::Boxed(_)) || matches!(r, Val::Boxed(_));
                let res = eval_binary(*op, unbox_if_boxed(l), unbox_if_boxed(r));
                if was_boxed {
                    Val::Boxed(Box::new(res))
                } else {
                    res
                }
            }
            SpecExprKind::Ignore(inner) => {
                self.eval_expr(inner, env);
                Val::Unit
            }
            other => panic!("unsupported SpecExprKind in unit eval: {other:?}"),
        }
    }

    fn eval_block(&mut self, stmts: &[SpecStmt], tail: &SpecExpr, env: &Env) -> Val {
        let mut inner = env.clone();
        for stmt in stmts {
            match stmt {
                SpecStmt::Let { pat, value, .. } => {
                    let v = self.eval_expr(value, &mut inner);
                    assert!(match_pat(pat, &v, &mut inner));
                }
                SpecStmt::Var { name, value, .. } | SpecStmt::Assign(name, value) => {
                    let v = self.eval_expr(value, &mut inner);
                    inner.locals.insert(name.clone(), v);
                }
                SpecStmt::Fun {
                    name, dicts, value, ..
                } => {
                    let val = if dicts.is_empty() {
                        self.eval_expr(value, &mut inner)
                    } else {
                        Val::Closure(Rc::new(ClosureVal {
                            dict_params: dicts.clone(),
                            params: Vec::new(),
                            body: value.clone(),
                            env: inner.clone(),
                            applied_dicts: Vec::new(),
                            applied_args: Vec::new(),
                        }))
                    };
                    inner.locals.insert(name.clone(), val);
                }
                SpecStmt::Expr(e) => {
                    self.eval_expr(e, &mut inner);
                }
            }
        }
        self.eval_expr(tail, &mut inner)
    }

    fn eval_match(&mut self, scrut: &Val, arms: &[SpecArm], env: &Env) -> Val {
        for arm in arms {
            let mut branch_env = env.clone();
            if match_pat(&arm.pat, scrut, &mut branch_env) {
                if let Some(guard) = &arm.guard
                    && !matches!(
                        unbox_if_boxed(self.eval_expr(guard, &mut branch_env)),
                        Val::Bool(true)
                    )
                {
                    continue;
                }
                return self.eval_expr(&arm.body, &mut branch_env);
            }
        }
        panic!("non-exhaustive match on {scrut:?}");
    }
}
