//! Structural methods use ordinary owned code and the borrowed Structure view.
use flick_ir::{Arith, Constructor, Num, Rep};

use super::{
    Arm, BinOp, Code, Dict, Env, Expr, ExprKind, Lit, Local, Lower, LowerError, Operation, Param,
    Pat, Span, Stage, Stmt, dictionary, memory, unsupported,
};

const VIEW: &str = "$structure$view";
const LEFT: &str = "$structure$left";
const RIGHT: &str = "$structure$right";
const IS_RECORD: &str = "$structure$record";

impl Lower<'_> {
    pub(super) fn structural(
        &mut self,
        class: &str,
        fields: &[Dict],
        env: &Env,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        if !matches!(class, "Eq" | "Ord" | "Hash" | "Show") {
            return Err(unsupported(format!("structural class {class}"), span));
        }
        let mut inside = Env::new();
        for (index, field) in fields.iter().enumerate() {
            inside.insert(field_name(index), self.dictionary(field, env, block, span)?);
        }
        let code = self.reserve(span)?;
        let mut view = Code::new(vec![0, 1]);
        let result = view.emit(
            Operation::Structure {
                object: 1,
                expected: fields.len(),
            },
            span,
        )?;
        self.finish(code, &view.finish(result))?;
        let view = block.emit(
            Operation::Closure {
                code,
                captures: vec![],
            },
            span,
        )?;
        inside.insert(VIEW.into(), view);
        let make = Builder {
            span,
            count: fields.len(),
        };
        let (name, body, binary) = match class {
            "Eq" => ("eq", make.equal(), true),
            "Ord" => ("compare", make.compare(), true),
            "Hash" => ("hash", make.hash(), false),
            "Show" => ("show", make.show(), false),
            _ => unreachable!("validated structural class"),
        };
        let parameters = if binary {
            vec![parameter(LEFT), parameter(RIGHT)]
        } else {
            vec![parameter(LEFT)]
        };
        let method = self.lambda(&parameters, &body, &inside, None, block, span)?;
        let mut entries = vec![(name.into(), memory::DictionaryEntry::Value(method))];
        if matches!(class, "Ord" | "Hash") {
            let eq = Dict::Structural {
                class: "Eq".into(),
                fields: (0..fields.len())
                    .map(|index| Dict::Super {
                        dict: Box::new(Dict::Field(field_name(index))),
                        class: "Eq".into(),
                    })
                    .collect(),
            };
            let body = dictionary(&eq, span)?;
            let getter = self.lambda(
                &[parameter("$structure$self")],
                &body,
                &inside,
                None,
                block,
                span,
            )?;
            entries.push(("$Eq".into(), memory::DictionaryEntry::Getter(getter)));
        }
        block.emit(Operation::Dictionary { entries }, span)
    }
}

fn parameter(name: &str) -> Param {
    Param {
        pat: Pat::Bind(name.into()),
        stage: Stage::Pure,
        ty: None,
        binder_types: Vec::new(),
    }
}

fn field_name(index: usize) -> String {
    format!("$structure$field{index}")
}

fn element(side: &str, index: usize) -> String {
    format!("{side}${index}")
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

struct Builder {
    span: Span,
    count: usize,
}

impl Builder {
    fn at(&self, kind: ExprKind) -> Expr {
        Expr::new(kind, self.span)
    }
    fn local(&self, name: &str) -> Expr {
        self.at(ExprKind::Local(name.into()))
    }
    fn text(&self, text: &str) -> Expr {
        self.at(ExprKind::Lit(Lit::String(text.into())))
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
    fn method(&self, index: usize, name: &str, binary: bool) -> Expr {
        let method = self.at(ExprKind::Method {
            dict: Dict::Field(field_name(index)),
            name: name.into(),
            type_args: Vec::new(),
        });
        let method = self.apply(method, self.local(&element(LEFT, index)));
        if binary {
            self.apply(method, self.local(&element(RIGHT, index)))
        } else {
            method
        }
    }
    fn view(&self, side: &str, show: bool) -> Stmt {
        let values = Pat::Tuple(
            (0..self.count)
                .map(|index| Pat::Bind(element(side, index)))
                .collect(),
        );
        let labels = if show {
            Pat::Tuple(
                (0..self.count)
                    .map(|index| Pat::Bind(element("$structure$label", index)))
                    .collect(),
            )
        } else {
            Pat::Wild
        };
        Stmt::let_bind(
            Pat::Tuple(vec![
                if show {
                    Pat::Bind(IS_RECORD.into())
                } else {
                    Pat::Wild
                },
                values,
                labels,
            ]),
            self.apply(self.local(VIEW), self.local(side)),
        )
    }
    fn body(&self, result: Expr, binary: bool) -> Expr {
        let mut statements = vec![self.view(LEFT, false)];
        if binary {
            statements.push(self.view(RIGHT, false));
        }
        self.at(ExprKind::Block(statements, Box::new(result)))
    }
    fn equal(&self) -> Expr {
        let mut result = self.at(ExprKind::Lit(Lit::Bool(true)));
        for index in (0..self.count).rev() {
            result = self.at(ExprKind::If(
                Box::new(self.method(index, "eq", true)),
                Box::new(result),
                Box::new(self.at(ExprKind::Lit(Lit::Bool(false)))),
            ));
        }
        self.body(result, true)
    }
    fn compare(&self) -> Expr {
        let mut result = self.at(ExprKind::Con(ordering("EQ")));
        for index in (0..self.count).rev() {
            result = self.at(ExprKind::Match(
                Box::new(self.method(index, "compare", true)),
                vec![
                    Arm {
                        pat: Pat::Con(ordering("EQ"), vec![]),
                        binder_types: Vec::new(),
                        guard: None,
                        body: result,
                    },
                    Arm {
                        pat: Pat::Bind("$structure$order".into()),
                        binder_types: Vec::new(),
                        guard: None,
                        body: self.local("$structure$order"),
                    },
                ],
            ));
        }
        self.body(result, true)
    }
    fn hash(&self) -> Expr {
        let mut statements = vec![self.view(LEFT, false)];
        let mut result = self.at(ExprKind::Lit(Lit::Int(11)));
        for index in 0..self.count {
            let value = self.binary(
                BinOp::Arith(Arith::Mul, Num::Int),
                result,
                self.at(ExprKind::Lit(Lit::Int(31))),
            );
            let value = self.binary(
                BinOp::Arith(Arith::Add, Num::Int),
                value,
                self.method(index, "hash", false),
            );
            let name = element("$structure$hash", index);
            statements.push(Stmt::let_bind(Pat::Bind(name.clone()), value));
            result = self.local(&name);
        }
        self.at(ExprKind::Block(statements, Box::new(result)))
    }
    fn show(&self) -> Expr {
        let mut statements = vec![self.view(LEFT, true)];
        for index in 0..self.count {
            statements.push(Stmt::let_bind(
                Pat::Bind(element("$structure$shown", index)),
                self.method(index, "show", false),
            ));
        }
        let mut record = self.text("{ ");
        let mut tuple = self.text("(");
        for index in 0..self.count {
            if index != 0 {
                record = self.binary(BinOp::Concat, record, self.text(", "));
                tuple = self.binary(BinOp::Concat, tuple, self.text(", "));
            }
            record = self.binary(
                BinOp::Concat,
                record,
                self.local(&element("$structure$label", index)),
            );
            record = self.binary(BinOp::Concat, record, self.text(" = "));
            record = self.binary(
                BinOp::Concat,
                record,
                self.local(&element("$structure$shown", index)),
            );
            tuple = self.binary(
                BinOp::Concat,
                tuple,
                self.local(&element("$structure$shown", index)),
            );
        }
        record = self.binary(BinOp::Concat, record, self.text(" }"));
        tuple = self.binary(BinOp::Concat, tuple, self.text(")"));
        let result = self.at(ExprKind::If(
            Box::new(self.local(IS_RECORD)),
            Box::new(record),
            Box::new(tuple),
        ));
        self.at(ExprKind::Block(statements, Box::new(result)))
    }
}
