//! Exact pure collection adapters, discharged by closed ordinary Flick code.
//!
//! No host export occurs: elements and callback captures remain owned language
//! values. Native JS also skips defensive copies for trusted `flick:rt` imports
//! (`codegen/js/foreign.rs`); nevertheless, every declared copy and callback
//! field is pinned below, never treated as a generic FFI exemption. Materialized
//! tails and `ListConcat` are a quadratic-copy baseline; this makes no linear-time
//! or FBIP claim.
use flick_ir::{
    Adapter, Arith, Constructor, Copies, Foreign, ForeignMode, Num, Rep, ResultAdapter, Shape,
};

use crate::ir::{Arm, BinOp, Expr, ExprKind, Lit, Param, Pat, Stage, Stmt};
use crate::source::Span;

#[path = "list_flat_map.rs"]
mod flat_map;
#[path = "list_map2.rs"]
mod map2;
#[path = "list_predicates.rs"]
mod predicates;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Collection {
    Map,
    Map2,
    FlatMap,
    Fold,
    FoldRight,
    Equals,
    Compare,
    Hash,
}

fn kind(value: &Foreign) -> Option<Collection> {
    match value.binding.name.as_str() {
        "listMap" => Some(Collection::Map),
        "listMap2" => Some(Collection::Map2),
        "listFlatMap" => Some(Collection::FlatMap),
        "listFold" => Some(Collection::Fold),
        "listFoldRight" => Some(Collection::FoldRight),
        "listEquals" => Some(Collection::Equals),
        "listCompare" => Some(Collection::Compare),
        "listHash" => Some(Collection::Hash),
        _ => None,
    }
}

fn adapter(kind: Collection, map_variant: usize) -> Adapter {
    let binary = matches!(
        kind,
        Collection::Map2
            | Collection::Fold
            | Collection::FoldRight
            | Collection::Equals
            | Collection::Compare
    );
    let arity = if binary { 3 } else { 2 };
    let copied_result = matches!(
        kind,
        Collection::Map
            | Collection::FlatMap
            | Collection::Map2
            | Collection::Fold
            | Collection::FoldRight
            | Collection::Compare
    );
    let mut callbacks = vec![None; arity];
    callbacks[0] = Some(if binary { 2 } else { 1 });
    let mut args = vec![true; arity];
    args[0] = false;
    let mut callback_copies = vec![None; arity];
    callback_copies[0] = Some((vec![true; if binary { 2 } else { 1 }], copied_result));
    let mut result = copied_result;
    if matches!(kind, Collection::Map2 | Collection::FlatMap) {
        callback_copies[0] = Some((vec![], false));
        if kind == Collection::FlatMap && map_variant == 1 {
            callback_copies[0] = Some((vec![true], true));
        }
    }
    if kind == Collection::Map {
        // Instances' rank-two callback does not export its argument. NonEmpty's
        // nominal input/output are not copied, unlike List's structural values.
        match map_variant {
            0 => {}
            1 => callback_copies[0] = Some((vec![], false)),
            2 => {
                args[1] = false;
                result = false;
            }
            _ => unreachable!("three audited builtin map contracts"),
        }
    }
    Adapter {
        receiver: false,
        cancel: false,
        optional: vec![],
        variadic: false,
        result: ResultAdapter {
            nullable: false,
            unit: false,
        },
        callbacks,
        copies: Copies {
            args,
            result,
            callbacks: callback_copies,
        },
        shapes: vec![Shape::Data; arity],
        result_shape: Shape::Data,
    }
}

fn supports(value: &Foreign, kind: Collection) -> bool {
    let arity = if matches!(
        kind,
        Collection::Map2
            | Collection::Fold
            | Collection::FoldRight
            | Collection::Equals
            | Collection::Compare
    ) {
        3
    } else {
        2
    };
    value.binding.from.as_deref() == Some("flick:rt")
        && value.mode == ForeignMode::Pure
        && value.dicts == 0
        && value.arity == arity
        && value.stages == vec![false; arity]
        && value.catch.is_none()
        && (0..match kind {
            Collection::Map => 3,
            Collection::FlatMap => 2,
            _ => 1,
        })
            .any(|variant| value.adapter == adapter(kind, variant))
}

pub(super) fn expression(value: &Foreign, span: Span) -> Option<Expr> {
    let kind = kind(value)?;
    supports(value, kind).then(|| Builder(span).collection(kind))
}

/// Templates whose readonly-array ABI needs a shallow `ArrayToList` boundary.
pub(super) fn array_expression(value: &Foreign, span: Span) -> Option<Expr> {
    predicates::expression(value, span).or_else(|| {
        (kind(value) == Some(Collection::FlatMap) && supports(value, Collection::FlatMap))
            .then(|| Builder(span).flat_map())
    })
}

// Private, hygienic binders: these closed templates never mention user locals
// or global/host functions. All callback applications retain their pure stages.
struct Builder(Span);
impl Builder {
    fn at(&self, kind: ExprKind) -> Expr {
        Expr::new(kind, self.0)
    }
    fn local(&self, name: &str) -> Expr {
        self.at(ExprKind::Local(format!("$collection.{name}")))
    }
    fn bind(name: &str) -> Pat {
        Pat::Bind(format!("$collection.{name}"))
    }
    fn lit(&self, value: Lit) -> Expr {
        self.at(ExprKind::Lit(value))
    }
    fn apply(&self, name: &str, args: Vec<Expr>) -> Expr {
        args.into_iter().fold(self.local(name), |func, arg| {
            self.at(ExprKind::App {
                func: Box::new(func),
                arg: Box::new(arg),
                stage: Stage::Pure,
                arg_first: false,
            })
        })
    }
    fn lambda(&self, names: &[&str], body: Expr) -> Expr {
        self.at(ExprKind::Lambda {
            params: names
                .iter()
                .map(|name| Param {
                    pat: Self::bind(name),
                    stage: Stage::Pure,
                    ty: None,
                    binder_types: Vec::new(),
                })
                .collect(),
            body: Box::new(body),
        })
    }
    fn arm(pat: Pat, body: Expr) -> Arm {
        Arm {
            pat,
            binder_types: Vec::new(),
            guard: None,
            body,
        }
    }
    fn empty() -> Pat {
        Pat::List(vec![], None)
    }
    fn cons(head: &str, tail: &str) -> Pat {
        Pat::List(vec![Self::bind(head)], Some(Box::new(Self::bind(tail))))
    }
    fn branch(&self, cond: Expr, yes: Expr, no: Expr) -> Expr {
        self.at(ExprKind::If(Box::new(cond), Box::new(yes), Box::new(no)))
    }
    fn binary(&self, op: BinOp, left: Expr, right: Expr) -> Expr {
        self.at(ExprKind::Binary(op, Box::new(left), Box::new(right)))
    }
    fn function(&self, name: &str, params: &[&str], body: Expr) -> Stmt {
        Stmt::Fun {
            name: format!("$collection.{name}"),
            rec: true,
            type_params: vec![],
            dicts: vec![],
            dict_constraints: vec![],
            ty: None,
            value: self.lambda(params, body),
        }
    }
    fn block(&self, stmts: Vec<Stmt>, result: Expr) -> Expr {
        self.at(ExprKind::Block(stmts, Box::new(result)))
    }
    fn ordering(name: &str) -> Constructor {
        Constructor {
            module: "Prelude".into(),
            ty: "Ordering".into(),
            name: name.into(),
            arity: 0,
            dicts: 0,
            rep: Rep::Tagged,
            fields: Vec::new(),
            type_args: Vec::new(),
        }
    }
    fn order(&self, name: &str) -> Expr {
        self.at(ExprKind::Con(Self::ordering(name)))
    }
    fn pair_match(&self, arms: Vec<Arm>) -> Expr {
        self.at(ExprKind::Match(
            Box::new(self.at(ExprKind::Tuple(vec![
                self.local("left"),
                self.local("right"),
            ]))),
            arms,
        ))
    }
    fn pair(left: Pat, right: Pat) -> Pat {
        Pat::Tuple(vec![left, right])
    }
    fn collection(&self, kind: Collection) -> Expr {
        match kind {
            Collection::Map => self.map(),
            Collection::Map2 => self.map2(),
            Collection::FlatMap => self.flat_map(),
            Collection::Fold => self.fold(),
            Collection::FoldRight => self.fold_right(),
            Collection::Hash => self.hash(),
            Collection::Equals | Collection::Compare => self.compare(kind),
        }
    }
    fn map(&self) -> Expr {
        // Explicit lets lock callback-before-recursion and once-per-element.
        let more = self.block(
            vec![
                Stmt::let_bind(
                    Self::bind("mapped"),
                    self.apply("callback", vec![self.local("head")]),
                ),
                Stmt::let_bind(
                    Self::bind("rest"),
                    self.apply("loop", vec![self.local("tail")]),
                ),
            ],
            self.binary(
                BinOp::ListConcat,
                self.at(ExprKind::List(vec![self.local("mapped")])),
                self.local("rest"),
            ),
        );
        let body = self.at(ExprKind::Match(
            Box::new(self.local("items")),
            vec![
                Self::arm(Self::empty(), self.at(ExprKind::List(vec![]))),
                Self::arm(Self::cons("head", "tail"), more),
            ],
        ));
        self.lambda(
            &["callback"],
            self.block(
                vec![self.function("loop", &["items"], body)],
                self.lambda(&["input"], self.apply("loop", vec![self.local("input")])),
            ),
        )
    }
    fn hash(&self) -> Expr {
        let next = self.binary(
            BinOp::Arith(Arith::Add, Num::Int),
            self.binary(
                BinOp::Arith(Arith::Mul, Num::Int),
                self.local("acc"),
                self.lit(Lit::Int(31)),
            ),
            self.local("hashed"),
        );
        let more = self.block(
            vec![Stmt::let_bind(
                Self::bind("hashed"),
                self.apply("callback", vec![self.local("head")]),
            )],
            self.apply("loop", vec![next, self.local("tail")]),
        );
        let body = self.at(ExprKind::Match(
            Box::new(self.local("items")),
            vec![
                Self::arm(Self::empty(), self.local("acc")),
                Self::arm(Self::cons("head", "tail"), more),
            ],
        ));
        self.lambda(
            &["callback"],
            self.block(
                vec![self.function("loop", &["acc", "items"], body)],
                self.lambda(
                    &["input"],
                    self.apply("loop", vec![self.lit(Lit::Int(7)), self.local("input")]),
                ),
            ),
        )
    }
    fn fold(&self) -> Expr {
        // Transfer the accumulator through the callback exactly once, before
        // visiting the next element. Empty input returns the seed untouched.
        let more = self.block(
            vec![Stmt::let_bind(
                Self::bind("next"),
                self.apply("callback", vec![self.local("acc"), self.local("head")]),
            )],
            self.apply("loop", vec![self.local("next"), self.local("tail")]),
        );
        let body = self.at(ExprKind::Match(
            Box::new(self.local("items")),
            vec![
                Self::arm(Self::empty(), self.local("acc")),
                Self::arm(Self::cons("head", "tail"), more),
            ],
        ));
        self.lambda(
            &["callback"],
            self.block(
                vec![self.function("loop", &["acc", "items"], body)],
                self.lambda(
                    &["acc", "input"],
                    self.apply("loop", vec![self.local("acc"), self.local("input")]),
                ),
            ),
        )
    }
    fn fold_right(&self) -> Expr {
        // Recurse before even the first curried application of the callback:
        // a pure callback may already diverge or defect at that first stage.
        let more = self.block(
            vec![Stmt::let_bind(
                Self::bind("rest"),
                self.apply("loop", vec![self.local("acc"), self.local("tail")]),
            )],
            self.apply("callback", vec![self.local("head"), self.local("rest")]),
        );
        let body = self.at(ExprKind::Match(
            Box::new(self.local("items")),
            vec![
                Self::arm(Self::empty(), self.local("acc")),
                Self::arm(Self::cons("head", "tail"), more),
            ],
        ));
        self.lambda(
            &["callback"],
            self.block(
                vec![self.function("loop", &["acc", "items"], body)],
                self.lambda(
                    &["acc", "input"],
                    self.apply("loop", vec![self.local("acc"), self.local("input")]),
                ),
            ),
        )
    }
    fn compare(&self, kind: Collection) -> Expr {
        let equal = kind == Collection::Equals;
        let recur = self.apply("loop", vec![self.local("ltail"), self.local("rtail")]);
        let callback = self.apply("callback", vec![self.local("lhead"), self.local("rhead")]);
        let more = if equal {
            self.branch(callback, recur, self.lit(Lit::Bool(false)))
        } else {
            self.at(ExprKind::Match(
                Box::new(callback),
                vec![
                    Self::arm(Pat::Con(Self::ordering("EQ"), vec![]), recur),
                    Self::arm(Self::bind("order"), self.local("order")),
                ],
            ))
        };
        let body = self.pair_match(vec![
            Self::arm(
                Self::pair(Self::empty(), Self::empty()),
                if equal {
                    self.lit(Lit::Bool(true))
                } else {
                    self.order("EQ")
                },
            ),
            Self::arm(
                Self::pair(Self::empty(), Pat::Wild),
                if equal {
                    self.lit(Lit::Bool(false))
                } else {
                    self.order("LT")
                },
            ),
            Self::arm(
                Self::pair(Pat::Wild, Self::empty()),
                if equal {
                    self.lit(Lit::Bool(false))
                } else {
                    self.order("GT")
                },
            ),
            Self::arm(
                Self::pair(Self::cons("lhead", "ltail"), Self::cons("rhead", "rtail")),
                more,
            ),
        ]);
        let mut functions = vec![self.function("loop", &["left", "right"], body)];
        let call = self.apply("loop", vec![self.local("left"), self.local("right")]);
        let result = if equal {
            // Native listEquals checks length before invoking ANY callback.
            // A pure length-only pass preserves this even for divergent callbacks.
            let lengths = self.pair_match(vec![
                Self::arm(
                    Self::pair(Self::empty(), Self::empty()),
                    self.lit(Lit::Bool(true)),
                ),
                Self::arm(
                    Self::pair(
                        Self::cons("unusedLeft", "ltail"),
                        Self::cons("unusedRight", "rtail"),
                    ),
                    self.apply("sameLength", vec![self.local("ltail"), self.local("rtail")]),
                ),
                Self::arm(Pat::Wild, self.lit(Lit::Bool(false))),
            ]);
            functions.push(self.function("sameLength", &["left", "right"], lengths));
            self.branch(
                self.apply("sameLength", vec![self.local("left"), self.local("right")]),
                call,
                self.lit(Lit::Bool(false)),
            )
        } else {
            call
        };
        self.lambda(
            &["callback"],
            self.block(functions, self.lambda(&["left", "right"], result)),
        )
    }
}

include!("list_collections/contract_tests.rs");
