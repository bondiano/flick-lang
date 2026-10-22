//! Names, application and operators. Operators are fixed: arithmetic on
//! `Number` or `BigInt`, `++` on `String` or `List`, comparisons on the
//! three primitive domains; operands never mix types or coerce.

use super::ty::Type;
use super::{Checker, Local, Operator};
use crate::diagnostics::{Diagnostic, Edit, Severity, codes};
use crate::iface::Namespace;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

const NUMERIC: &[&str] = &["Number", "BigInt"];
const ORDERED: &[&str] = &["Number", "BigInt", "String"];
const CONCAT: &[&str] = &["String", "List"];

impl Checker<'_> {
    pub fn push_local(&mut self, name: &str, ty: Type) {
        self.locals.push(Local {
            name: name.to_owned(),
            ty,
            rec: None,
        });
    }

    /// The type of a value or constructor reference, instantiated.
    pub fn name(&mut self, node: &Node) -> Type {
        let parts: Vec<&str> = node
            .child_tokens()
            .filter(|t| t.kind != K::Dot)
            .map(|t| t.text(self.text))
            .collect();
        let Some((&name, qualifier)) = parts.split_last() else {
            return Type::Error;
        };
        let span = self.span(node);
        let namespace = if name.starts_with(|c: char| c.is_ascii_uppercase()) {
            Namespace::Constructor
        } else {
            Namespace::Value
        };
        if !qualifier.is_empty() {
            return self.qualified(&qualifier.join("."), namespace, name, span);
        }
        if namespace == Namespace::Value
            && let Some(i) = self.locals.iter().rposition(|l| l.name == name)
        {
            return self.use_local(i, span);
        }
        match self.env.find(namespace, name) {
            super::env::Found::Entity(entity) => {
                if entity.module == self.env.module && namespace == Namespace::Value {
                    return self.top_value(name, span);
                }
                let scheme = self.env.value(&entity).cloned();
                scheme.map_or(Type::Error, |s| self.instantiate(&s))
            }
            super::env::Found::Unknown => Type::Error,
            super::env::Found::Missing if namespace == Namespace::Constructor => {
                self.service_tag(name).unwrap_or_else(|| {
                    self.error(
                        codes::T001,
                        span,
                        format!("cannot find constructor `{name}`"),
                    );
                    Type::Error
                })
            }
            super::env::Found::Missing => {
                let what = match namespace {
                    Namespace::Constructor => "constructor",
                    _ => "value",
                };
                self.error(codes::T001, span, format!("cannot find {what} `{name}`"));
                Type::Error
            }
        }
    }

    pub fn use_local(&mut self, index: usize, span: Span) -> Type {
        let Local { ty, rec, .. } = self.locals[index].clone();
        let ty = self.instantiate(&ty);
        if let Some(rec) = rec {
            self.recs[rec].uses.push((ty.clone(), span));
        }
        ty
    }

    /// `Module.name`, `Module.Con`, or `Service.operation`.
    fn qualified(&mut self, qualifier: &str, namespace: Namespace, name: &str, span: Span) -> Type {
        if let Some((_, interface)) = self.env.qualified_module(qualifier) {
            // a name the module does not export is reported by name resolution
            let entity = interface
                .and_then(|i| i.find(namespace, name))
                .map(|e| e.entity.clone());
            let scheme = entity.and_then(|e| self.env.value(&e).cloned());
            return scheme.map_or(Type::Error, |s| self.instantiate(&s));
        }
        let service = match self.env.find(Namespace::Type, qualifier) {
            super::env::Found::Entity(e) => self
                .env
                .type_def(&e.module, &e.name)
                .cloned()
                .map(|d| (e.module, d)),
            super::env::Found::Unknown => return Type::Error,
            super::env::Found::Missing => None,
        };
        match service.map(|(module, d)| (module, d.form)) {
            Some((module, super::env::Form::Service { params, operations })) => {
                if let Some(op) = operations.iter().find(|o| o.name == name) {
                    self.operation_type(&module, qualifier, params.len(), &op.ty)
                } else {
                    self.error(
                        codes::T001,
                        span,
                        format!("service `{qualifier}` has no operation `{name}`"),
                    );
                    Type::Error
                }
            }
            Some(_) => {
                self.error(
                    codes::T001,
                    span,
                    format!("`{qualifier}` is a type, not a module or a service"),
                );
                Type::Error
            }
            None => {
                self.error(codes::T001, span, format!("unknown module `{qualifier}`"));
                Type::Error
            }
        }
    }

    /// `f a b` is `(f a) b`: each argument is one stage, and a runtime arrow
    /// at a stage makes the application a runtime call.
    pub fn apply(&mut self, head: &Node, args: &[&Node]) -> Type {
        let mut ty = self.infer(head);
        let mut params = Vec::new();
        for (i, arg) in args.iter().enumerate() {
            let span = Span::new(self.file, head.range().0, arg.range().1);
            match self.whnf(&ty) {
                Type::Fun {
                    param,
                    effects,
                    result,
                } => {
                    let calls = self.stage.calls;
                    self.check(arg, &param);
                    if self.stage.calls > calls && self.eff_parts(&param).is_some() {
                        self.early_argument(head, arg);
                    }
                    if let Some(e) = effects {
                        self.perform(&e.services, &e.errors, span);
                    }
                    params.push(*param);
                    ty = self.instantiate(&result);
                }
                Type::Error => {
                    self.infer(arg);
                }
                meta @ Type::Meta(_) => {
                    let (param, result) = (self.fresh(), self.fresh());
                    let _ = self.unify(&meta, &Type::pure_fun(param.clone(), result.clone()));
                    self.check(arg, &param);
                    ty = result;
                }
                other => {
                    let shown = self.show(&other);
                    self.error(
                        codes::T003,
                        span,
                        format!("this is not a function: it has type `{shown}`, so it cannot take another argument"),
                    );
                    for rest in &args[i..] {
                        self.infer(rest);
                    }
                    return Type::Error;
                }
            }
        }
        if let [first, second] = &params[..]
            && self.is_builtin(head, "Layer", "merge")
        {
            let span = Span::new(self.file, head.range().0, args[1].range().1);
            self.merge_conflicts(first, second, span);
        }
        ty
    }

    /// A runtime call in an argument runs before the function it is passed
    /// to; a function that takes an `Eff` wants the computation unstarted.
    fn early_argument(&mut self, head: &Node, arg: &Node) {
        let span = self.span(arg);
        let name = self.text_of(head);
        self.diagnostics.push(
            Diagnostic::new(
                codes::E003,
                Severity::Warning,
                format!(
                    "this argument runs a runtime call before `{name}` is applied, although `{name}` takes an `Eff` to run itself"
                ),
            )
            .with_span(span)
            .with_fix(
                "pass the computation unstarted with `task { … }`",
                vec![
                    Edit {
                        span: Span::new(span.file, span.start, span.start),
                        replacement: "task { ".to_owned(),
                    },
                    Edit {
                        span: Span::new(span.file, span.end, span.end),
                        replacement: " }".to_owned(),
                    },
                ],
            ),
        );
    }

    /// `head` names the value `name` of the built-in module `module`.
    fn is_builtin(&self, head: &Node, module: &str, name: &str) -> bool {
        let parts: Vec<&str> = head
            .child_tokens()
            .filter(|t| t.kind != K::Dot)
            .map(|t| t.text(self.text))
            .collect();
        head.kind == K::NameExpr
            && parts.len() == 2
            && parts[1] == name
            && self
                .env
                .qualified_module(parts[0])
                .is_some_and(|(m, _)| m == module)
    }

    pub fn binary(&mut self, node: &Node, children: &[&Node]) -> Type {
        let Some(op) = node.child_tokens().find(|t| t.kind.is_binary_op()) else {
            return Type::Error;
        };
        let (Some(lhs), Some(rhs)) = (children.first(), children.get(1)) else {
            return Type::Error;
        };
        let span = self.span(node);
        let bool_ = Type::prim("Bool");
        match op.kind {
            K::PipeGt => self.pipe(lhs, rhs),
            K::OrOr | K::AndAnd => {
                self.check(lhs, &bool_);
                self.check(rhs, &bool_);
                bool_
            }
            K::EqEq | K::NotEq => {
                let ty = self.infer(lhs);
                self.check(rhs, &ty);
                bool_
            }
            kind => {
                let ty = self.infer(lhs);
                self.check(rhs, &ty);
                let (name, allowed) = match kind {
                    K::Lt => ("<", ORDERED),
                    K::Le => ("<=", ORDERED),
                    K::Gt => (">", ORDERED),
                    K::Ge => (">=", ORDERED),
                    K::PlusPlus => ("++", CONCAT),
                    K::Plus => ("+", NUMERIC),
                    K::Minus => ("-", NUMERIC),
                    K::Star => ("*", NUMERIC),
                    K::Slash => ("/", NUMERIC),
                    K::Percent => ("%", NUMERIC),
                    _ => ("**", NUMERIC),
                };
                self.operators.push(Operator {
                    ty: ty.clone(),
                    span,
                    op: name,
                    allowed,
                });
                if allowed == ORDERED { bool_ } else { ty }
            }
        }
    }

    /// `x |> f` applies `f` to `x`, evaluating `x` first.
    fn pipe(&mut self, lhs: &Node, rhs: &Node) -> Type {
        let arg = self.infer(lhs);
        let function = self.infer(rhs);
        match self.whnf(&function) {
            Type::Fun {
                param,
                effects,
                result,
            } => {
                self.expect(&arg, &param, self.span(lhs));
                if let Some(e) = effects {
                    let (start, _) = lhs.range();
                    let (_, end) = rhs.range();
                    self.perform(&e.services, &e.errors, Span::new(self.file, start, end));
                }
                self.instantiate(&result)
            }
            Type::Error => Type::Error,
            meta @ Type::Meta(_) => {
                let result = self.fresh();
                let _ = self.unify(&meta, &Type::pure_fun(arg, result.clone()));
                result
            }
            other => {
                let shown = self.show(&other);
                self.error(
                    codes::T003,
                    self.span(rhs),
                    format!("the right side of `|>` must be a function, found `{shown}`"),
                );
                Type::Error
            }
        }
    }

    pub fn prefix(&mut self, node: &Node, children: &[&Node]) -> Type {
        let Some(op) = node.child_tokens().next() else {
            return Type::Error;
        };
        let Some(operand) = children.first() else {
            return Type::Error;
        };
        match op.kind {
            K::NotKw => {
                let bool_ = Type::prim("Bool");
                self.check(operand, &bool_);
                bool_
            }
            K::IgnoreKw => {
                self.infer(operand);
                Type::prim("Unit")
            }
            K::AwaitKw => {
                let ty = self.infer(operand);
                let (services, errors, result) = (self.fresh(), self.fresh(), self.fresh());
                let shape = super::infer::eff(services.clone(), errors.clone(), result.clone());
                if self.unify(&ty, &shape).is_err() {
                    let shown = self.show(&ty);
                    self.error(
                        codes::T003,
                        self.span(operand),
                        format!("`await` runs an `Eff` value, but this has type `{shown}`"),
                    );
                    return Type::Error;
                }
                self.perform(&services, &errors, self.span(node));
                result
            }
            _ => {
                let ty = self.infer(operand);
                self.operators.push(Operator {
                    ty: ty.clone(),
                    span: self.span(node),
                    op: "-",
                    allowed: NUMERIC,
                });
                ty
            }
        }
    }

    /// Checks the operand types of operators once the declaration is
    /// inferred; an operand type nothing determined becomes the first
    /// allowed type (`Number`, or `String` for `++`).
    pub fn settle_operators(&mut self) {
        for op in std::mem::take(&mut self.operators) {
            let ty = self.whnf(&op.ty);
            let (head, _) = ty.spine();
            let fits = match head {
                Type::Error => true,
                Type::Meta(_) => {
                    let _ = self.unify(&ty, &Type::prim(op.allowed[0]));
                    true
                }
                head => op.allowed.iter().any(|name| head.is_prim(name)),
            };
            // an operand already reported inside the operator explains it better
            let reported = || {
                self.diagnostics.iter().any(|d| {
                    d.span
                        .is_some_and(|s| s.start >= op.span.start && s.end <= op.span.end)
                })
            };
            if !fits && !reported() {
                let shown = self.show(&ty);
                let allowed = op
                    .allowed
                    .iter()
                    .map(|a| format!("`{a}`"))
                    .collect::<Vec<_>>()
                    .join(" or ");
                let (code, detail) = match ty {
                    Type::Skolem(_) => (
                        codes::T008,
                        format!("`{shown}` stands for any type the caller chooses"),
                    ),
                    _ => (codes::T003, format!("not on `{shown}`")),
                };
                self.error(
                    code,
                    op.span,
                    format!("`{}` works on {allowed}, {detail}", op.op),
                );
            }
        }
    }
}
