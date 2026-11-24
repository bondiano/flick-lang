//! On-demand lowering and `PureTotalV2` verification of function contracts.
//!
//! Contracts are lowered ONLY when [`check_contracts`] is invoked (i.e. by
//! `flick prove` or [`crate::prove`]). The module's runtime IR
//! ([`super::lower`]) never contains contracts, so `check`, `build`, `run`,
//! `test`, and codegen stay untouched.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use super::proofs::{Modules, Run};
use super::{Expr, ExprKind, ItemKind, Lit, Lower};
use crate::diagnostics::Diagnostic;
use crate::ir::Pat as IrPat;
use crate::logic::contract::{
    ContractDepNode, ContractFailure, ContractOrder, ContractSpec, ContractSummary, LangTypes,
    PrimHint, Term, TermKind, VerifiedContracts, collect_ir_contract_deps, extract,
    order_contracts, prim_hint_of_ty,
};
use crate::logic::{self, Program, Refused};
use crate::resolve;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::{self, Lang};
use crate::types::{ContractInfo, Range, Typing};
use flick_ir::Stage;

/// A contract that passed kernel verification in this `flick prove` run, stored
/// in a thread-safe (`Send + Sync`) form so downstream modules checked in later
/// parallel levels can import its summary without unfolding its body.
#[derive(Debug, Clone)]
pub struct ProvenContract {
    pub module: String,
    pub contract: LoweredContract,
    pub info: ContractInfo,
}

/// One function's contract, lowered on demand with the function's parameters in
/// scope.
#[derive(Debug, Clone)]
pub struct LoweredContract {
    /// Source range of the `LetDecl`, matching `ContractInfo::at`.
    pub at: Range,
    pub decl_span: Span,
    pub contract_span: Span,
    /// Unique lowered names of the `LetDecl`'s `K::Params`, in order.
    pub params: Vec<String>,
    /// Each `require e` expression in declaration order, with its expression span.
    pub requires: Vec<(Expr, Span)>,
    /// Each `ensure e` expression in declaration order, with its expression span.
    pub ensures: Vec<(Expr, Span)>,
    /// Set if any `K::Params` pattern is not `Bind`, `Wild`, or `Lit(Unit)`.
    pub unsupported_param: Option<Span>,
}

/// Lowers every function contract of `module`, which already checked without
/// errors. Never modifies runtime IR.
pub fn lower_contracts(module: &resolve::Module, typing: &Typing) -> Vec<LoweredContract> {
    let mut lower = Lower::new(module, typing);
    let mut out = Vec::new();
    for decl in module.tree.nodes() {
        match decl.kind {
            K::LetDecl => {
                if let Some(contract) = decl.nodes().find(|n| n.kind == K::Contract) {
                    out.push(lower.contract(decl, contract));
                }
            }
            K::RecGroup => {
                for member in decl
                    .nodes()
                    .filter(|n| n.kind == K::Members)
                    .flat_map(Node::nodes)
                    .filter(|n| n.kind == K::LetDecl)
                {
                    if let Some(contract) = member.nodes().find(|n| n.kind == K::Contract) {
                        out.push(lower.contract(member, contract));
                    }
                }
            }
            _ => {}
        }
    }
    out
}

impl Lower<'_> {
    fn contract(&mut self, decl: &Node, contract: &Node) -> LoweredContract {
        self.fresh_definition();
        let decl_span = self.span(decl);
        let contract_span = self.span(contract);
        let param_nodes: Vec<&Node> = decl
            .nodes()
            .filter(|n| n.kind == K::Params)
            .flat_map(Node::nodes)
            .collect();
        let mut requires = Vec::new();
        let mut ensures = Vec::new();
        let lambda = self.function_with(
            &param_nodes,
            |l| {
                for entry in contract.nodes() {
                    let Some(expr_node) = entry.nodes().next() else {
                        continue;
                    };
                    match entry.kind {
                        K::Requires => requires.push((l.expr(expr_node), l.span(expr_node))),
                        K::Ensures => ensures.push((l.expr(expr_node), l.span(expr_node))),
                        _ => {}
                    }
                }
                Expr::new(ExprKind::Lit(Lit::Unit), decl_span)
            },
            None,
            decl_span,
        );
        let mut params = Vec::with_capacity(param_nodes.len());
        let mut unsupported_param = None;
        let mut raw_params = Vec::new();
        collect_lambda_params(&lambda, &mut raw_params);
        for (idx, param) in raw_params.into_iter().enumerate() {
            match &param.pat {
                IrPat::Bind(name) => params.push(name.clone()),
                IrPat::Wild | IrPat::Lit(Lit::Unit) => params.push(format!("$unused{idx}")),
                _ => {
                    if unsupported_param.is_none() {
                        let span = param_nodes
                            .get(idx)
                            .map_or(decl_span, |node| self.span(node));
                        unsupported_param = Some(span);
                    }
                    params.push(format!("$unsupported{idx}"));
                }
            }
        }
        LoweredContract {
            at: decl.range(),
            decl_span,
            contract_span,
            params,
            requires,
            ensures,
            unsupported_param,
        }
    }
}

fn collect_lambda_params<'a>(expr: &'a Expr, out: &mut Vec<&'a flick_ir::Param>) {
    match &expr.kind {
        ExprKind::Lambda { params, body } => {
            out.extend(params.iter());
            collect_lambda_params(body, out);
        }
        ExprKind::App { arg, .. } if matches!(arg.kind, ExprKind::Lambda { .. }) => {
            collect_lambda_params(arg, out);
        }
        _ => {}
    }
}

/// Builds `PureTotalV2`'s built-in type and class identities from the
/// compiler's `@lang` table.
fn lang_types() -> LangTypes {
    let pair = |item: Lang| {
        let found = lang::item(item);
        (found.module.clone(), found.name.clone())
    };
    LangTypes {
        int: pair(Lang::Int),
        bool_: pair(Lang::Bool),
        float: pair(Lang::Float),
        bigint: pair(Lang::BigInt),
        string: pair(Lang::String),
        unit: pair(Lang::Unit),
        eq_class: pair(Lang::Eq),
        ord_class: pair(Lang::Ord),
        numeric_class: pair(Lang::Numeric),
    }
}

/// Verifies every contract declared in `module` (which already passed type
/// checking cleanly) in `PureTotalV2`, in callee-before-caller order.
///
/// Failed, refused, and cyclic contracts are never added to
/// [`VerifiedContracts`] and are never assumed by callers. Returns the
/// diagnostics alongside the verified [`ProvenContract`] values certified for
/// `module` in this run.
pub fn check_contracts(
    module: &resolve::Module,
    typing: &Typing,
    run: &Run,
) -> (Vec<Diagnostic>, Vec<ProvenContract>) {
    if typing.contracts.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let lowered = lower_contracts(module, typing);
    let paired: Vec<(LoweredContract, &ContractInfo)> = typing
        .contracts
        .iter()
        .filter_map(|info| {
            let contract = lowered.iter().find(|c| c.at == info.at)?.clone();
            Some((contract, info))
        })
        .collect();
    if paired.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let own = super::lower(module, typing).ok();
    let program = Modules {
        own: own.as_ref(),
        project: run.project,
        builtins: (run.builtins)(),
    };
    let lang = lang_types();

    let contracted_names: HashSet<String> =
        paired.iter().map(|(_, info)| info.name.clone()).collect();
    let mut verified = seed_verified(module.name, &paired, &program, run, &lang);

    let mut dep_nodes = Vec::with_capacity(paired.len());
    for (contract, info) in &paired {
        let mut ir_roots: Vec<&Expr> =
            Vec::with_capacity(1 + contract.requires.len() + contract.ensures.len());
        if let Some(item) = program.item(module.name, &info.name)
            && let ItemKind::Value { body, .. } = &item.kind
        {
            ir_roots.push(body);
        }
        for (req, _) in &contract.requires {
            ir_roots.push(req);
        }
        for (ens, _) in &contract.ensures {
            ir_roots.push(ens);
        }
        let deps = collect_ir_contract_deps(&ir_roots, module.name, &contracted_names, &program);
        dep_nodes.push(ContractDepNode {
            name: info.name.clone(),
            deps,
        });
    }

    let by_name: HashMap<&str, (&LoweredContract, &ContractInfo)> = paired
        .iter()
        .map(|(contract, info)| (info.name.as_str(), (contract, *info)))
        .collect();

    let schedule = order_contracts(&dep_nodes);
    let mut diagnostics = Vec::new();
    let mut proven_out = Vec::new();

    for entry in schedule {
        match entry {
            ContractOrder::Cyclic(name) => {
                let Some((contract, info)) = by_name.get(name.as_str()) else {
                    continue;
                };
                // Explicitly keep cyclic contracts unverified so callers never assume them.
                verified.mark_unverified(module.name.to_owned(), info.name.clone());
                let failure = ContractFailure::Refused(Refused {
                    span: contract.decl_span,
                    why: format!("`{name}` is on a cycle: recursion is outside the domain"),
                });
                diagnostics.push(failure.diagnostic(&info.name));
            }
            ContractOrder::Acyclic(name) => {
                let Some((contract, info)) = by_name.get(name.as_str()) else {
                    continue;
                };
                match verify_one(module.name, contract, info, &program, &mut verified, &lang) {
                    Ok(()) => proven_out.push(ProvenContract {
                        module: module.name.to_owned(),
                        contract: (*contract).clone(),
                        info: (*info).clone(),
                    }),
                    Err(failure) => {
                        verified.mark_unverified(module.name.to_owned(), info.name.clone());
                        diagnostics.push(failure.diagnostic(&info.name));
                    }
                }
            }
        }
    }

    (diagnostics, proven_out)
}

fn seed_verified(
    module_name: &str,
    paired: &[(LoweredContract, &ContractInfo)],
    program: &dyn Program,
    run: &Run,
    lang: &LangTypes,
) -> VerifiedContracts {
    let mut verified = VerifiedContracts::new();
    // Seed EVERY contracted function declared anywhere in the run as unverified
    // first so `admit_v2` NEVER silently unfolds a contracted callee's body.
    let mut other_modules: HashSet<&str> = HashSet::new();
    for (dep_mod, dep_name) in run.all_contracted {
        verified.mark_unverified(dep_mod.clone(), dep_name.clone());
        if dep_mod != module_name {
            other_modules.insert(dep_mod.as_str());
        }
    }
    // Import verified summaries ONLY from modules certified earlier in this same run.
    for other_mod in other_modules {
        for proven in (run.contracts)(other_mod) {
            if let Ok(spec) = build_spec(
                &proven.module,
                &proven.contract,
                &proven.info,
                program,
                lang,
            ) {
                verified.insert_verified(ContractSummary::from_spec(&spec, &proven.info.data));
            }
        }
    }
    for (_, info) in paired {
        verified.mark_unverified(module_name.to_owned(), info.name.clone());
    }
    verified
}

fn check_admissible(
    contract: &LoweredContract,
    info: &ContractInfo,
) -> Result<(), ContractFailure> {
    let refuse = |span: Span, why: String| Err(ContractFailure::Refused(Refused { span, why }));
    if info.has_dicts {
        return refuse(
            contract.decl_span,
            format!(
                "`{}` takes class dictionaries, which are outside the domain",
                info.name
            ),
        );
    }
    if info.has_effects {
        return refuse(
            contract.decl_span,
            format!("`{}` has effects, which are outside the domain", info.name),
        );
    }
    if info.recursive {
        return refuse(
            contract.decl_span,
            format!(
                "`{}` is recursive: recursion is outside the domain",
                info.name
            ),
        );
    }
    if let Some(span) = contract.unsupported_param {
        return refuse(
            span,
            "a parameter pattern is not supported in a contract".into(),
        );
    }
    if contract.params.len() != info.params.len() {
        return refuse(
            contract.decl_span,
            format!(
                "the parameters of `{}` could not be matched to its signature",
                info.name
            ),
        );
    }
    Ok(())
}

fn verify_one(
    module_name: &str,
    contract: &LoweredContract,
    info: &ContractInfo,
    program: &dyn Program,
    verified: &mut VerifiedContracts,
    lang: &LangTypes,
) -> Result<(), ContractFailure> {
    let refuse = |span: Span, why: String| Err(ContractFailure::Refused(Refused { span, why }));
    check_admissible(contract, info)?;

    let Some(item) = program.item(module_name, &info.name) else {
        return refuse(
            contract.decl_span,
            format!(
                "the body of `{}` is not available to this contract",
                info.name
            ),
        );
    };
    let ItemKind::Value { dicts, body, .. } = &item.kind else {
        return refuse(
            contract.decl_span,
            format!("`{}` is no function a contract checks", info.name),
        );
    };
    if !dicts.is_empty() {
        return refuse(
            contract.decl_span,
            format!(
                "`{}` takes class dictionaries, which are outside the domain",
                info.name
            ),
        );
    }

    let body_expr = peel_body_params(body, &contract.params, contract.decl_span)?;

    let spec = build_spec(module_name, contract, info, program, lang)?;
    let ty_env: Vec<(Rc<str>, PrimHint)> = spec
        .params
        .iter()
        .map(|(name, ty)| (name.clone(), prim_hint_of_ty(ty, lang)))
        .collect();
    let body_term = extract(body_expr, &ty_env, program, lang).map_err(ContractFailure::Refused)?;

    verified.prove_and_record(&spec, &body_term, &info.data, program, lang)?;
    Ok(())
}

fn build_spec(
    module_name: &str,
    contract: &LoweredContract,
    info: &ContractInfo,
    program: &dyn Program,
    lang: &LangTypes,
) -> Result<ContractSpec, ContractFailure> {
    let params: Vec<(Rc<str>, logic::Ty)> = contract
        .params
        .iter()
        .zip(&info.params)
        .map(|(name, ty)| (Rc::<str>::from(name.as_str()), ty.clone()))
        .collect();
    let ty_env: Vec<(Rc<str>, PrimHint)> = params
        .iter()
        .map(|(name, ty)| (name.clone(), prim_hint_of_ty(ty, lang)))
        .collect();

    let mut requires = Vec::with_capacity(contract.requires.len());
    for (req_expr, req_span) in &contract.requires {
        let req_term =
            extract(req_expr, &ty_env, program, lang).map_err(ContractFailure::Refused)?;
        requires.push((req_term, *req_span));
    }

    let result_hint = prim_hint_of_ty(&info.result, lang);
    let mut ensures = Vec::with_capacity(contract.ensures.len());
    for (ens_expr, ens_span) in &contract.ensures {
        let ens_term = extract_ensure(ens_expr, &ty_env, result_hint, program, lang)
            .map_err(ContractFailure::Refused)?;
        ensures.push((ens_term, *ens_span));
    }

    Ok(ContractSpec {
        module: module_name.to_owned(),
        name: info.name.clone(),
        decl_span: contract.decl_span,
        contract_span: contract.contract_span,
        params,
        result_ty: info.result.clone(),
        requires,
        ensures,
    })
}

/// Peels the outer lambda parameters corresponding to the declaration's written
/// parameters from `body`, returning the inner body expression evaluated in the
/// environment binding `expected_params`.
fn peel_body_params<'a>(
    body: &'a Expr,
    expected_params: &[String],
    decl_span: Span,
) -> Result<&'a Expr, ContractFailure> {
    if expected_params.is_empty() {
        return Ok(body);
    }
    let mut cur = body;
    let mut matched = 0;
    while matched < expected_params.len() {
        let ExprKind::Lambda { params, body } = &cur.kind else {
            return Err(ContractFailure::Refused(Refused {
                span: decl_span,
                why: "the function body shape is outside the domain".into(),
            }));
        };
        if params.is_empty() || matched + params.len() > expected_params.len() {
            return Err(ContractFailure::Refused(Refused {
                span: decl_span,
                why: "the function body parameters do not match the contract".into(),
            }));
        }
        for param in params {
            if param.stage != Stage::Pure {
                return Err(ContractFailure::Refused(Refused {
                    span: decl_span,
                    why: "a function with effects is outside the domain".into(),
                }));
            }
            match &param.pat {
                IrPat::Bind(name) if name == &expected_params[matched] => {}
                IrPat::Wild | IrPat::Lit(Lit::Unit) => {}
                _ => {
                    return Err(ContractFailure::Refused(Refused {
                        span: decl_span,
                        why: "a parameter pattern is not supported in a contract".into(),
                    }));
                }
            }
            matched += 1;
        }
        cur = body;
    }
    Ok(cur)
}

/// Extracts an `ensure` predicate (`Result -> Bool`), seeding an untyped lambda
/// parameter's [`PrimHint`] from the function's known result type.
fn extract_ensure(
    expr: &Expr,
    ty_env: &[(Rc<str>, PrimHint)],
    result_hint: PrimHint,
    program: &dyn Program,
    lang: &LangTypes,
) -> Result<Term, Refused> {
    if let ExprKind::Lambda { params, body } = &expr.kind
        && let [param] = params.as_slice()
        && param.stage == Stage::Pure
        && param.ty.is_none()
        && let IrPat::Bind(bound) = &param.pat
    {
        let mut extended = ty_env.to_vec();
        let rc: Rc<str> = bound.as_str().into();
        extended.push((rc.clone(), result_hint));
        let body_term = extract(body, &extended, program, lang)?;
        return Ok(Term::new(TermKind::Lam(rc, Rc::new(body_term)), expr.span));
    }
    extract(expr, ty_env, program, lang)
}
