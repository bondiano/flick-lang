//! Derived code uses explicit counted constructor layouts, never host boxing.
use flick_ir::{Arith, Constructor, Num, Rep};

use super::{
    Arm, BinOp, Dict, Expr, ExprKind, Lit, LowerError, Param, Pat, Span, Stage, Stmt, dictionary,
    unsupported,
};

const LEFT: &str = "$derived$left";
const RIGHT: &str = "$derived$right";

pub(super) fn methods(
    class: &str,
    fields: &[Dict],
    constructors: Option<&[Constructor]>,
    span: Span,
) -> Result<Vec<(String, Expr)>, LowerError> {
    let (name, binary) = match class {
        "Eq" => ("eq", true),
        "Ord" => ("compare", true),
        "Hash" => ("hash", false),
        "Show" => ("show", false),
        _ => return Err(unsupported(format!("derived class {class}"), span)),
    };
    let make = Builder { span, fields };
    let body = if let Some(constructors) = constructors {
        let mut offset = 0_usize;
        let mut variants = Vec::new();
        for (index, constructor) in constructors.iter().enumerate() {
            let index = i32::try_from(index).map_err(|_| LowerError::Limit(span))?;
            variants.push(Variant {
                constructor,
                offset,
                index,
            });
            offset = offset
                .checked_add(constructor.arity)
                .ok_or(LowerError::Limit(span))?;
        }
        if offset != fields.len() {
            return Err(unsupported("derived field metadata", span));
        }
        match class {
            "Eq" => make.equal(&variants)?,
            "Ord" => make.compare(&variants)?,
            "Hash" => make.hash(&variants)?,
            "Show" => make.show(&variants)?,
            _ => unreachable!("validated class"),
        }
    } else {
        if fields.len() != 1 {
            return Err(unsupported("derived representation metadata", span));
        }
        make.prepare(make.call(0, name, make.local(LEFT), binary.then(|| make.local(RIGHT))))?
    };
    let params = if binary {
        vec![parameter(LEFT), parameter(RIGHT)]
    } else {
        vec![parameter(LEFT)]
    };
    Ok(vec![(
        name.into(),
        make.at(ExprKind::Lambda {
            params,
            body: Box::new(body),
        }),
    )])
}

fn parameter(name: &str) -> Param {
    Param {
        pat: Pat::Bind(name.into()),
        stage: Stage::Pure,
        ty: None,
        binder_types: Vec::new(),
    }
}

fn element(side: &str, index: usize) -> String {
    format!("{side}${index}")
}
fn evidence(index: usize) -> String {
    format!("$derived$dict{index}")
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

struct Variant<'a> {
    constructor: &'a Constructor,
    offset: usize,
    index: i32,
}
struct Builder<'a> {
    span: Span,
    fields: &'a [Dict],
}

impl Variant<'_> {
    fn pattern(&self, side: &str, bind: bool) -> Pat {
        let fields = (0..self.constructor.arity).map(|index| {
            if bind {
                Pat::Bind(element(side, self.offset + index))
            } else {
                Pat::Wild
            }
        });
        // Ordinary values precede the owned dictionary evidence suffix.
        let items = fields
            .chain((0..self.constructor.dicts).map(|_| Pat::Wild))
            .collect();
        Pat::Con(self.constructor.clone(), items)
    }
}

impl Builder<'_> {
    fn at(&self, kind: ExprKind) -> Expr {
        Expr::new(kind, self.span)
    }
    fn local(&self, name: &str) -> Expr {
        self.at(ExprKind::Local(name.into()))
    }
    fn text(&self, text: &str) -> Expr {
        self.at(ExprKind::Lit(Lit::String(text.into())))
    }
    fn int(&self, value: i32) -> Expr {
        self.at(ExprKind::Lit(Lit::Int(value)))
    }
    fn apply(&self, function: Expr, argument: Expr) -> Expr {
        self.at(ExprKind::App {
            func: Box::new(function),
            arg: Box::new(argument),
            stage: Stage::Pure,
            arg_first: false,
        })
    }
    fn binary(&self, op: BinOp, left: Expr, right: Expr) -> Expr {
        self.at(ExprKind::Binary(op, Box::new(left), Box::new(right)))
    }
    fn call(&self, index: usize, name: &str, left: Expr, right: Option<Expr>) -> Expr {
        let method = self.at(ExprKind::Method {
            dict: Dict::Field(evidence(index)),
            name: name.into(),
            type_args: Vec::new(),
        });
        let call = self.apply(method, left);
        if let Some(right) = right {
            self.apply(call, right)
        } else {
            call
        }
    }
    fn field(&self, index: usize, name: &str, binary: bool) -> Expr {
        self.call(
            index,
            name,
            self.local(&element(LEFT, index)),
            binary.then(|| self.local(&element(RIGHT, index))),
        )
    }
    fn prepare(&self, result: Expr) -> Result<Expr, LowerError> {
        let statements = self
            .fields
            .iter()
            .enumerate()
            .map(|(index, field)| {
                Ok(Stmt::let_bind(
                    Pat::Bind(evidence(index)),
                    dictionary(field, self.span)?,
                ))
            })
            .collect::<Result<_, LowerError>>()?;
        Ok(self.at(ExprKind::Block(statements, Box::new(result))))
    }
    fn with_fields(&self, variant: &Variant<'_>, result: Expr) -> Result<Expr, LowerError> {
        if variant.constructor.arity == 0 {
            Ok(result)
        } else {
            self.prepare(result)
        }
    }
    fn matching(&self, value: Expr, arms: Vec<Arm>) -> Expr {
        self.at(ExprKind::Match(Box::new(value), arms))
    }
    fn equal(&self, variants: &[Variant<'_>]) -> Result<Expr, LowerError> {
        let mut arms = Vec::new();
        for variant in variants {
            let mut result = self.at(ExprKind::Lit(Lit::Bool(true)));
            for index in (variant.offset..variant.offset + variant.constructor.arity).rev() {
                result = self.at(ExprKind::If(
                    Box::new(self.field(index, "eq", true)),
                    Box::new(result),
                    Box::new(self.at(ExprKind::Lit(Lit::Bool(false)))),
                ));
            }
            arms.push(Arm {
                pat: Pat::Tuple(vec![
                    variant.pattern(LEFT, true),
                    variant.pattern(RIGHT, true),
                ]),
                binder_types: Vec::new(),
                guard: None,
                body: self.with_fields(variant, result)?,
            });
        }
        arms.push(Arm {
            pat: Pat::Wild,
            binder_types: Vec::new(),
            guard: None,
            body: self.at(ExprKind::Lit(Lit::Bool(false))),
        });
        Ok(self.matching(
            self.at(ExprKind::Tuple(vec![self.local(LEFT), self.local(RIGHT)])),
            arms,
        ))
    }
    fn index(&self, side: &str, variants: &[Variant<'_>]) -> Expr {
        self.matching(
            self.local(side),
            variants
                .iter()
                .map(|variant| Arm {
                    pat: variant.pattern(side, false),
                    binder_types: Vec::new(),
                    guard: None,
                    body: self.int(variant.index),
                })
                .collect(),
        )
    }
    fn compare(&self, variants: &[Variant<'_>]) -> Result<Expr, LowerError> {
        let mut arms = Vec::new();
        for variant in variants {
            let mut result = self.at(ExprKind::Con(ordering("EQ")));
            for index in (variant.offset..variant.offset + variant.constructor.arity).rev() {
                result = self.matching(
                    self.field(index, "compare", true),
                    vec![
                        Arm {
                            pat: Pat::Con(ordering("EQ"), vec![]),
                            binder_types: Vec::new(),
                            guard: None,
                            body: result,
                        },
                        Arm {
                            pat: Pat::Bind("$derived$order".into()),
                            binder_types: Vec::new(),
                            guard: None,
                            body: self.local("$derived$order"),
                        },
                    ],
                );
            }
            arms.push(Arm {
                pat: Pat::Tuple(vec![
                    variant.pattern(LEFT, true),
                    variant.pattern(RIGHT, true),
                ]),
                binder_types: Vec::new(),
                guard: None,
                body: self.with_fields(variant, result)?,
            });
        }
        let different = self.at(ExprKind::If(
            Box::new(self.binary(
                BinOp::Lt,
                self.index(LEFT, variants),
                self.index(RIGHT, variants),
            )),
            Box::new(self.at(ExprKind::Con(ordering("LT")))),
            Box::new(self.at(ExprKind::Con(ordering("GT")))),
        ));
        arms.push(Arm {
            pat: Pat::Wild,
            binder_types: Vec::new(),
            guard: None,
            body: different,
        });
        Ok(self.matching(
            self.at(ExprKind::Tuple(vec![self.local(LEFT), self.local(RIGHT)])),
            arms,
        ))
    }
    fn hash(&self, variants: &[Variant<'_>]) -> Result<Expr, LowerError> {
        let mut arms = Vec::new();
        for variant in variants {
            let mut result = self.int(variant.index);
            for index in variant.offset..variant.offset + variant.constructor.arity {
                let times = self.binary(BinOp::Arith(Arith::Mul, Num::Int), result, self.int(31));
                result = self.binary(
                    BinOp::Arith(Arith::Add, Num::Int),
                    times,
                    self.field(index, "hash", false),
                );
            }
            arms.push(Arm {
                pat: variant.pattern(LEFT, true),
                binder_types: Vec::new(),
                guard: None,
                body: self.with_fields(variant, result)?,
            });
        }
        Ok(self.matching(self.local(LEFT), arms))
    }
    fn string_predicate(&self, name: &str, needle: &str, text: Expr) -> Expr {
        let function = self.at(ExprKind::Global {
            module: "String".into(),
            name: name.into(),
            type_args: Vec::new(),
        });
        self.apply(self.apply(function, self.text(needle)), text)
    }
    fn atom(&self, shown: Expr) -> Expr {
        let name = "$derived$shown";
        let mut atomic = self.at(ExprKind::Not(Box::new(self.string_predicate(
            "contains",
            " ",
            self.local(name),
        ))));
        for prefix in ["[", "(", "{", "\""] {
            atomic = self.binary(
                BinOp::Or,
                self.string_predicate("starts_with", prefix, self.local(name)),
                atomic,
            );
        }
        let parenthesized = self.binary(
            BinOp::Concat,
            self.binary(BinOp::Concat, self.text("("), self.local(name)),
            self.text(")"),
        );
        let result = self.at(ExprKind::If(
            Box::new(atomic),
            Box::new(self.local(name)),
            Box::new(parenthesized),
        ));
        self.at(ExprKind::Block(
            vec![Stmt::let_bind(Pat::Bind(name.into()), shown)],
            Box::new(result),
        ))
    }
    fn show(&self, variants: &[Variant<'_>]) -> Result<Expr, LowerError> {
        let mut arms = Vec::new();
        for variant in variants {
            let mut result = self.text(&variant.constructor.name);
            for index in variant.offset..variant.offset + variant.constructor.arity {
                result = self.binary(BinOp::Concat, result, self.text(" "));
                result = self.binary(
                    BinOp::Concat,
                    result,
                    self.atom(self.field(index, "show", false)),
                );
            }
            arms.push(Arm {
                pat: variant.pattern(LEFT, true),
                binder_types: Vec::new(),
                guard: None,
                body: self.with_fields(variant, result)?,
            });
        }
        Ok(self.matching(self.local(LEFT), arms))
    }
}
