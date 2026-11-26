//! Real Flick-to-IR-to-SpecializedProgram compiler helper and genuine
//! layout-aware fixture backend / evaluator.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use flick_backend_support::layout::{PrimitiveTable, TypeRepr};
use flick_backend_support::specialize::{
    SpecArm, SpecExpr, SpecExprKind, SpecParam, SpecStmt, SpecializeConfig, SpecializeError,
    SpecializedProgram, SymbolId, specialize,
};
use flick_core::build;
use flick_core::diagnostics::Severity;
use flick_core::ir;
use flick_core::project::{self, Host};
use flick_core::source::{FileId, SourceMap};
use flick_ir::{Lit, Program};

use crate::ops::{box_deep, eval_binary, match_pat, unbox_deep, unbox_if_boxed};

struct InMemoryHost {
    files: BTreeMap<PathBuf, Vec<u8>>,
}

impl Host for InMemoryHost {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.display().to_string()))
    }

    fn display(&self, path: &Path) -> String {
        path.display().to_string()
    }
}

/// Compiles real Flick source text into a complete whole-program typed
/// [`flick_ir::Program`] including all built-in modules (`Prelude`, `Option`,
/// `List`, `Instances`, etc.).
pub(crate) fn compile_flick(source: &str) -> Program {
    let root = PathBuf::from("App/Main.fl");
    let mut files = BTreeMap::new();
    files.insert(root.clone(), source.as_bytes().to_vec());
    let host = InMemoryHost { files };
    let mut sources = SourceMap::default();
    let (diagnostics, compilation) = project::compile(&host, &mut sources, None, &[root]);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "Flick source failed to compile: {errors:#?}"
    );

    let mut modules: Vec<ir::Module> = build::builtin_modules().to_vec();
    for compiled in &compilation.modules {
        modules.push((*compiled.ir).clone());
    }
    let path = |f: FileId| format!("file:{}", f.index());
    Program::new("flick-test", modules, ir::lang_items(), &path)
}

/// Compiles real Flick source and specializes from `roots`.
pub(crate) fn compile_and_specialize(
    source: &str,
    roots: &[(&str, &str)],
    table: &PrimitiveTable,
    config: &SpecializeConfig,
) -> Result<SpecializedProgram, SpecializeError> {
    let program = compile_flick(source);
    let root_keys: Vec<(String, String)> = roots
        .iter()
        .map(|(m, n)| ((*m).to_owned(), (*n).to_owned()))
        .collect();
    specialize(&program, &root_keys, table, config)
}

#[derive(Debug, Clone)]
pub(crate) enum Val {
    Unit,
    Bool(bool),
    Int(i32),
    Float(f64),
    String(String),
    Tuple(Vec<Val>),
    List(Vec<Val>),
    /// Closed unboxed record stored in canonical `Record.labels` slot order.
    RecordSlots(Vec<(String, Val)>),
    /// Boxed open/fallback record stored by label.
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
    pub self_name: Option<String>,
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

type ForeignHook = Box<dyn Fn(&str, &str, bool, Vec<Val>) -> Option<Val>>;

pub(crate) struct FixtureBackend<'a> {
    pub program: &'a SpecializedProgram,
    pub stats: EvalStats,
    pub foreign_hook: Option<ForeignHook>,
}

impl<'a> FixtureBackend<'a> {
    pub(crate) fn new(program: &'a SpecializedProgram) -> Self {
        FixtureBackend {
            program,
            stats: EvalStats::default(),
            foreign_hook: None,
        }
    }

    pub(crate) fn with_foreign(
        program: &'a SpecializedProgram,
        hook: impl Fn(&str, &str, bool, Vec<Val>) -> Option<Val> + 'static,
    ) -> Self {
        FixtureBackend {
            program,
            stats: EvalStats::default(),
            foreign_hook: Some(Box::new(hook)),
        }
    }

    pub(crate) fn run_root(&mut self, module: &str, name: &str) -> Val {
        let sym = self
            .program
            .roots
            .get(&(module.to_owned(), name.to_owned()))
            .unwrap_or_else(|| panic!("missing root `{module}.{name}`"))
            .clone();
        let v = self.eval_symbol(&sym, Vec::new());
        // If the root is a `Unit -> T` function (e.g. `run ()`), invoke it with `()`.
        if matches!(unbox_if_boxed(v.clone()), Val::Closure(_)) {
            self.apply_val(v, Val::Unit)
        } else {
            v
        }
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
                box_deep(v)
            }
            SpecExprKind::Unbox { expr: inner, .. } => {
                let v = self.eval_expr(inner, env);
                self.stats.runtime_unboxes += 1;
                unbox_deep(v)
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
                    .map_or(1, |f| f.foreign.stages.len().max(1));
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
                dval.methods
                    .get(name)
                    .unwrap_or_else(|| panic!("missing method `{name}` in runtime dict"))
                    .clone()
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
            SpecExprKind::UpdateOffset { record, fields } => {
                let Val::RecordSlots(mut slots) = self.eval_expr(record, env) else {
                    panic!("UpdateOffset expected RecordSlots");
                };
                for (_, index, _, val_expr) in fields {
                    slots[*index].1 = self.eval_expr(val_expr, env);
                }
                Val::RecordSlots(slots)
            }
            SpecExprKind::UpdateByLabel { record, fields } => {
                let mut map = match unbox_if_boxed(self.eval_expr(record, env)) {
                    Val::RecordMap(m) => m,
                    Val::RecordSlots(s) => s.into_iter().collect(),
                    other => panic!("UpdateByLabel expected record, got {other:?}"),
                };
                for (label, val_expr) in fields {
                    map.insert(label.clone(), self.eval_expr(val_expr, env));
                }
                Val::Boxed(Box::new(Val::RecordMap(map)))
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
            SpecExprKind::Local(name) | SpecExprKind::Var(name) => env
                .locals
                .get(name)
                .unwrap_or_else(|| panic!("unbound local `{name}`"))
                .clone(),
            SpecExprKind::Abstract(names, inner) => Val::Closure(Rc::new(ClosureVal {
                dict_params: names.clone(),
                params: Vec::new(),
                body: (**inner).clone(),
                env: env.clone(),
                applied_dicts: Vec::new(),
                applied_args: Vec::new(),
                self_name: None,
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
                self_name: None,
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
            SpecExprKind::Not(inner) => {
                let v = self.eval_expr(inner, env);
                let was_boxed = matches!(v, Val::Boxed(_));
                let Val::Bool(b) = unbox_if_boxed(v) else {
                    panic!("Not expected Bool");
                };
                if was_boxed {
                    Val::Boxed(Box::new(Val::Bool(!b)))
                } else {
                    Val::Bool(!b)
                }
            }
            SpecExprKind::Ignore(inner) => {
                self.eval_expr(inner, env);
                Val::Unit
            }
            other => panic!("unsupported SpecExprKind in fixture backend: {other:?}"),
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
                    name,
                    rec,
                    dicts,
                    value,
                    ..
                } => {
                    let val = match &value.kind {
                        SpecExprKind::Lambda { params, body } if dicts.is_empty() => {
                            Val::Closure(Rc::new(ClosureVal {
                                dict_params: Vec::new(),
                                params: params.clone(),
                                body: (**body).clone(),
                                env: inner.clone(),
                                applied_dicts: Vec::new(),
                                applied_args: Vec::new(),
                                self_name: rec.then(|| name.clone()),
                            }))
                        }
                        _ => Val::Closure(Rc::new(ClosureVal {
                            dict_params: dicts.clone(),
                            params: Vec::new(),
                            body: value.clone(),
                            env: inner.clone(),
                            applied_dicts: Vec::new(),
                            applied_args: Vec::new(),
                            self_name: rec.then(|| name.clone()),
                        })),
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
