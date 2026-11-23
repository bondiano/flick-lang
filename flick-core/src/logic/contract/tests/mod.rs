//! Unit tests for Stage 3a & 3b: `PureTotalV2` extraction, operand-type
//! confirmation, `admit_v2`, Tarjan SCC dependency ordering, `ContractFailure`
//! diagnostics, `Typing::contracts` metadata plumbing, symbolic execution,
//! VC solving, certificates, and independent kernel verification.

mod admit;
mod adt;
mod arith;
mod branch;
mod calls;
mod extract;
mod soundness;

use std::collections::HashMap;
use std::rc::Rc;

use flick_ir::{
    Adapter, BinOp, Binding, Expr, ExprKind, Foreign, ForeignMode, InstanceInfo, IrType, Item,
    ItemKind, Lit, TypeRef,
};

use super::{ContractSpec, Counterexample, IntBinOp, IntCmpOp, LangTypes, Term, TermKind};
use crate::diagnostics::Severity;
use crate::ir;
use crate::logic::{Def, Program, Ty};
use crate::resolve::{self, NO_GENERATED, module_syntax};
use crate::source::{SourceMap, Span};
use crate::syntax;
use crate::types::{Limits, Options, Thresholds, Typing, Watched, builtins_of, check_typed};

type InstanceEntry = (InstanceInfo, Vec<(String, Expr)>);

fn span() -> Span {
    Span::new(SourceMap::default().add("t.fl", ""), 0, 0)
}

fn span_at(start: u32, end: u32) -> Span {
    Span::new(SourceMap::default().add("t.fl", "0123456789"), start, end)
}

fn lang() -> LangTypes {
    LangTypes {
        int: ("Prelude".into(), "Int".into()),
        bool_: ("Prelude".into(), "Bool".into()),
        float: ("Prelude".into(), "Float".into()),
        bigint: ("Prelude".into(), "BigInt".into()),
        string: ("Prelude".into(), "String".into()),
        unit: ("Prelude".into(), "Unit".into()),
        eq_class: ("Prelude".into(), "Eq".into()),
        ord_class: ("Prelude".into(), "Ord".into()),
        numeric_class: ("Numeric".into(), "Numeric".into()),
    }
}

fn ty_name(module: &str, name: &str) -> TypeRef {
    TypeRef::new(IrType::Named {
        module: module.into(),
        name: name.into(),
        args: Vec::new(),
    })
}

fn expr(kind: ExprKind) -> Expr {
    Expr::new(kind, span())
}

fn typed_expr(kind: ExprKind, module: &str, name: &str) -> Expr {
    let mut e = expr(kind);
    e.ty = Some(ty_name(module, name));
    e
}

fn int_lit(n: i32) -> Expr {
    expr(ExprKind::Lit(Lit::Int(n)))
}

fn bool_lit(b: bool) -> Expr {
    expr(ExprKind::Lit(Lit::Bool(b)))
}

fn local(name: &str) -> Expr {
    expr(ExprKind::Local(name.into()))
}

fn global(module: &str, name: &str) -> Expr {
    expr(ExprKind::Global {
        module: module.into(),
        name: name.into(),
        type_args: Vec::new(),
    })
}

fn bin(op: BinOp, l: Expr, r: Expr) -> Expr {
    expr(ExprKind::Binary(op, Box::new(l), Box::new(r)))
}

#[derive(Default)]
struct FakeProgram {
    items: HashMap<(String, String), Item>,
    instances: HashMap<(String, String), InstanceEntry>,
}

impl FakeProgram {
    fn add_value(&mut self, module: &str, name: &str, body: Expr) {
        self.items.insert(
            (module.into(), name.into()),
            Item {
                kind: ItemKind::Value {
                    name: name.into(),
                    type_params: Vec::new(),
                    dicts: Vec::new(),
                    dict_constraints: Vec::new(),
                    ty: None,
                    body,
                },
                span: span(),
            },
        );
    }

    fn add_dict_value(&mut self, module: &str, name: &str, body: Expr) {
        self.items.insert(
            (module.into(), name.into()),
            Item {
                kind: ItemKind::Value {
                    name: name.into(),
                    type_params: Vec::new(),
                    dicts: vec!["Eq a".into()],
                    dict_constraints: Vec::new(),
                    ty: None,
                    body,
                },
                span: span(),
            },
        );
    }

    fn add_foreign(&mut self, module: &str, name: &str) {
        self.items.insert(
            (module.into(), name.into()),
            Item {
                kind: ItemKind::Foreign(Foreign {
                    name: name.into(),
                    source_name: None,
                    mode: ForeignMode::Pure,
                    binding: Binding {
                        from: Some("flick:rt".into()),
                        name: name.into(),
                    },
                    stages: vec![false],
                    dicts: 0,
                    arity: 1,
                    adapter: Adapter::default(),
                    catch: None,
                    type_params: Vec::new(),
                    ty: None,
                }),
                span: span(),
            },
        );
    }

    fn add_instance(
        &mut self,
        module: &str,
        name: &str,
        class: (&str, &str),
        methods: Vec<(String, Expr)>,
    ) {
        let info = InstanceInfo {
            module: module.into(),
            name: name.into(),
            class: (class.0.into(), class.1.into()),
            span: (0, 0),
            context: Vec::new(),
            supers: Vec::new(),
            derived: None,
            defaults: Vec::new(),
            generic: None,
            enumerates: None,
        };
        self.instances
            .insert((module.into(), name.into()), (info, methods));
    }
}

impl Program for FakeProgram {
    fn item(&self, module: &str, name: &str) -> Option<&Item> {
        self.items.get(&(module.into(), name.into()))
    }

    fn instance(&self, module: &str, name: &str) -> Option<(&InstanceInfo, &[(String, Expr)])> {
        self.instances
            .get(&(module.into(), name.into()))
            .map(|(info, methods)| (info, methods.as_slice()))
    }
}

fn check_and_lower(path: &str, source: &str) -> (Typing, flick_ir::Module) {
    let mut sources = SourceMap::default();
    let file = sources.add(path, source);
    let parsed = syntax::parse(source, file);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let syn = module_syntax(&parsed.tree, source, file);
    let module_name = syn.header.as_ref().map_or("", |h| h.name.as_str());
    let module = resolve::Module {
        name: module_name,
        file,
        text: source,
        tree: &parsed.tree,
        syntax: &syn,
        generated: NO_GENERATED,
    };
    let builtins = builtins_of(false);
    let imported = |name: &str| builtins.iter().find(|i| i.module == name).cloned();
    let (mut iface, res_diags) = resolve::resolve(&module, &imported);
    assert!(res_diags.is_empty(), "{res_diags:?}");
    let watched = Watched::default();
    let options = Options {
        local: &|_| true,
        limits: Limits::default(),
        no_let_assert: false,
        thresholds: Thresholds::default(),
        watched: &watched,
        stages_by_class: false,
    };
    let (diags, typing) = check_typed(&module, &imported, builtins, options, &mut iface);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    let ir_mod = ir::lower(&module, &typing).expect("lowering succeeds");
    (typing, ir_mod)
}

fn t_at(kind: TermKind, s: Span) -> Term {
    Term::new(kind, s)
}

fn t(kind: TermKind) -> Term {
    Term::new(kind, span())
}

fn tvar(name: &str) -> Term {
    t(TermKind::Var(name.into()))
}

fn tint(n: i128) -> Term {
    t(TermKind::IntLit(n))
}

fn tbool(b: bool) -> Term {
    t(TermKind::BoolLit(b))
}

fn tadd(a: Term, b: Term, s: Span) -> Term {
    t_at(TermKind::IntBin(IntBinOp::Add, Rc::new(a), Rc::new(b)), s)
}

fn tsub(a: Term, b: Term, s: Span) -> Term {
    t_at(TermKind::IntBin(IntBinOp::Sub, Rc::new(a), Rc::new(b)), s)
}

fn tneg(a: Term, s: Span) -> Term {
    t_at(TermKind::IntNeg(Rc::new(a)), s)
}

fn tcmp(op: IntCmpOp, a: Term, b: Term) -> Term {
    t(TermKind::IntCmp(op, Rc::new(a), Rc::new(b)))
}

fn tand(a: Term, b: Term) -> Term {
    t(TermKind::And(Rc::new(a), Rc::new(b)))
}

fn tor(a: Term, b: Term) -> Term {
    t(TermKind::Or(Rc::new(a), Rc::new(b)))
}

fn tnot(a: Term) -> Term {
    t(TermKind::Not(Rc::new(a)))
}

fn tif(c: Term, then_t: Term, else_t: Term) -> Term {
    t(TermKind::If(Rc::new(c), Rc::new(then_t), Rc::new(else_t)))
}

fn tlam(x: &str, body: Term) -> Term {
    t(TermKind::Lam(x.into(), Rc::new(body)))
}

fn tapp(f: Term, a: Term, s: Span) -> Term {
    t_at(
        TermKind::App {
            func: Rc::new(f),
            arg: Rc::new(a),
            arg_first: false,
        },
        s,
    )
}

fn tpipe(a: Term, f: Term, s: Span) -> Term {
    t_at(
        TermKind::App {
            func: Rc::new(f),
            arg: Rc::new(a),
            arg_first: true,
        },
        s,
    )
}

fn tdef(module: &str, name: &str) -> Term {
    t(TermKind::Def(Rc::new(Def::Value {
        module: module.into(),
        name: name.into(),
    })))
}

fn tlet(name: &str, val: Term, body: Term) -> Term {
    t(TermKind::Let(name.into(), Rc::new(val), Rc::new(body)))
}

fn consume_spec(req_span: Span, ens_span: Span, decl_span: Span) -> ContractSpec {
    ContractSpec {
        module: "App".into(),
        name: "consume".into(),
        decl_span,
        contract_span: decl_span,
        params: vec![("available".into(), Ty::Int), ("used".into(), Ty::Int)],
        result_ty: Ty::Int,
        requires: vec![(
            tand(
                tcmp(IntCmpOp::Le, tint(0), tvar("used")),
                tcmp(IntCmpOp::Le, tvar("used"), tvar("available")),
            ),
            req_span,
        )],
        ensures: vec![(
            tlam(
                "remaining",
                tand(
                    tcmp(IntCmpOp::Le, tint(0), tvar("remaining")),
                    tcmp(
                        IntCmpOp::Eq,
                        tadd(tvar("remaining"), tvar("used"), span()),
                        tvar("available"),
                    ),
                ),
            ),
            ens_span,
        )],
    }
}

fn ce_int(ce: &Counterexample, name: &str) -> i128 {
    ce.bindings
        .iter()
        .find(|(k, _)| k == name)
        .unwrap_or_else(|| panic!("missing `{name}` in counterexample: {ce:?}"))
        .1
        .parse::<i128>()
        .expect("integer counterexample binding")
}
