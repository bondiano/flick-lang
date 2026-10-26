//! `p1 <- e1 and p2 <- e2 …` in `do`: bindings that run independently,
//! combined with the Prelude's `Applicative.map2`, one per `and`, into the
//! tuple of their values. When the group is followed only by `pure e`, the
//! last `map2` computes `e` itself, so the block needs `Applicative` alone;
//! otherwise the rest of the block is `flat_map` over the tuple. A value
//! of the group does not see what the others bind.

use super::Checker;
use super::ty::Type;
use crate::diagnostics::codes;
use crate::iface::Namespace;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::env::Found;

/// The key of the `map2` of each `and` of a group: its token.
pub fn and_keys(group: &Node) -> Vec<(u32, u32)> {
    group
        .child_tokens()
        .filter(|t| t.kind == K::AndKw)
        .map(|t| (t.start, t.end))
        .collect()
}

/// The key of the `flat_map` over a group's tuple: the empty range at its start.
pub fn group_key(group: &Node) -> (u32, u32) {
    (group.range().0, group.range().0)
}

/// The argument `e` of a last statement `pure e`, as written.
pub fn pure_argument(statement: &Node) -> Option<&Node> {
    let parts: Vec<&Node> = statement.nodes().collect();
    match (statement.kind, &parts[..]) {
        (K::AppExpr, [head, value]) if head.kind == K::NameExpr => Some(value),
        _ => None,
    }
}

impl Checker<'_> {
    pub(super) fn do_group(
        &mut self,
        node: &Node,
        group: &Node,
        rest: &[&Node],
        expected: Option<&Type>,
    ) -> Type {
        let binds: Vec<&Node> = group.nodes().filter(|n| n.kind == K::BindStmt).collect();
        let parts: Vec<(Option<&Node>, Option<&Node>)> = binds
            .iter()
            .map(|b| {
                let mut children = b.nodes();
                (children.next(), children.next())
            })
            .collect();
        let since = self.diagnostics.len();
        let values: Vec<Type> = parts
            .iter()
            .map(|(_, v)| v.map_or(Type::Error, |v| self.infer(v)))
            .collect();
        self.unbound_in_group(since, &parts);
        let mut bound = Vec::new();
        for pattern in parts.iter().filter_map(|(p, _)| *p) {
            binders(pattern, self.text, &mut bound);
        }
        let applicative = match rest {
            [last] => self
                .prelude_pure(last, &bound)
                .then(|| pure_argument(last))
                .flatten(),
            _ => None,
        };
        // `map2` at each `and`, folding the values into one of their tuple
        let mut items: Vec<Type> = Vec::new();
        let mut acc = values.first().cloned().unwrap_or(Type::Error);
        let mut result = None;
        for (i, key) in and_keys(group).into_iter().enumerate() {
            let span = Span::new(self.file, key.0, key.1);
            let Some((a, b, x, fa, fb, fx)) = self.map2_parts(span) else {
                return Type::Error;
            };
            if i > 0 {
                let _ = self.unify(&fa, &acc);
            }
            if let Some(value) = values.get(i + 1) {
                let at = parts[i + 1].1.map_or(span, |v| self.span(v));
                self.expect(value, &fb, at);
            }
            // checked like the others, once the second shows the container
            if i == 0 && self.unify(&fa, &acc).is_err() {
                let at = parts[0].1.map_or(span, |v| self.span(v));
                self.expect(&acc, &fa, at);
            }
            if i == 0 {
                items.push(a);
            } else {
                let _ = self.unify(&a, &Type::Tuple(items.clone()));
            }
            items.push(b);
            let last = i + 2 == values.len();
            if last && applicative.is_some() {
                result = Some((x, fx.clone()));
            } else {
                let _ = self.unify(&x, &Type::Tuple(items.clone()));
            }
            acc = fx;
        }
        let patterns: Vec<Option<&Node>> = parts.iter().map(|(p, _)| *p).collect();
        if let (Some(value), Some((x, fx))) = (applicative, result) {
            self.typing.applicative.insert(group.range());
            let function = curried(&items, x);
            let body = |c: &mut Self, result: Option<Type>| match result {
                Some(result) => {
                    c.check(value, &result);
                    result
                }
                None => c.infer(value),
            };
            self.function_with(&patterns, body, Some(&function), group);
            if let Some(expected) = expected {
                self.expect(&fx, expected, self.span(group));
            }
            return fx;
        }
        self.group_rest(node, group, rest, (&items, acc, &patterns), expected)
    }

    /// The rest of the block after a group: `flat_map` over its tuple.
    fn group_rest(
        &mut self,
        node: &Node,
        group: &Node,
        rest: &[&Node],
        (items, acc, patterns): (&[Type], Type, &[Option<&Node>]),
        expected: Option<&Type>,
    ) -> Type {
        let span = {
            let (start, end) = group_key(group);
            Span::new(self.file, start, end)
        };
        let method = self.prelude_method("flat_map", span);
        let Type::Fun {
            param: continuation,
            result,
            ..
        } = self.whnf(&method)
        else {
            return Type::Error;
        };
        let applied = self.instantiate(&result, span);
        let Type::Fun {
            param: monadic,
            result,
            ..
        } = self.whnf(&applied)
        else {
            return Type::Error;
        };
        let ty = self.instantiate(&result, span);
        let _ = self.unify(&monadic, &acc);
        if let Some(expected) = expected {
            self.expect(&ty, expected, self.span(group));
        }
        let Type::Fun {
            param: tuple,
            result: rest_ty,
            ..
        } = self.whnf(&continuation)
        else {
            return Type::Error;
        };
        let _ = self.unify(&tuple, &Type::Tuple(items.to_vec()));
        let rest_ty = self.instantiate(&rest_ty, span);
        let function = curried(items, rest_ty);
        let body = |c: &mut Self, result: Option<Type>| c.do_rest(node, rest, result.as_ref());
        self.function_with(patterns, body, Some(&function), group);
        ty
    }

    /// `(a, b, x, f a, f b, f x)` of `map2 : (a -> b -> x) -> f a -> f b -> f x`
    /// instantiated at `span`, whose dictionary is recorded there.
    fn map2_parts(&mut self, span: Span) -> Option<(Type, Type, Type, Type, Type, Type)> {
        let method = self.prelude_method("map2", span);
        let Type::Fun { param, result, .. } = self.whnf(&method) else {
            return None;
        };
        let Type::Fun {
            param: a,
            result: bx,
            ..
        } = self.whnf_captured(&param).0
        else {
            return None;
        };
        let Type::Fun {
            param: b,
            result: x,
            ..
        } = self.whnf_captured(&bx).0
        else {
            return None;
        };
        let applied = self.instantiate(&result, span);
        let Type::Fun {
            param: fa,
            result: rest,
            ..
        } = self.whnf_captured(&applied).0
        else {
            return None;
        };
        let applied = self.instantiate(&rest, span);
        let Type::Fun {
            param: fb,
            result: fx,
            ..
        } = self.whnf_captured(&applied).0
        else {
            return None;
        };
        let fx = self.instantiate(&fx, span);
        Some((*a, *b, *x, *fa, *fb, fx))
    }

    /// Whether `statement` is `pure e` of the Prelude's `Applicative`,
    /// not of a `pure` the group binds in `bound`.
    fn prelude_pure(&self, statement: &Node, bound: &[String]) -> bool {
        let Some(head) = statement
            .nodes()
            .next()
            .filter(|_| pure_argument(statement).is_some())
        else {
            return false;
        };
        let name = &self.text[head.range().0 as usize..head.range().1 as usize];
        name == "pure"
            && !bound.iter().any(|b| b == "pure")
            && !self.locals.iter().any(|l| l.name == "pure")
            && matches!(self.env.find(Namespace::Value, "pure"),
                Found::Entity(e) if e.module == "Prelude")
    }

    /// A value of a group naming what another binding of it binds: the
    /// group runs them independently, so that name is not in scope.
    fn unbound_in_group(&mut self, since: usize, parts: &[(Option<&Node>, Option<&Node>)]) {
        let mut names = Vec::new();
        for (pattern, _) in parts {
            if let Some(pattern) = pattern {
                binders(pattern, self.text, &mut names);
            }
        }
        for d in &mut self.diagnostics[since..] {
            if d.code != codes::T001 {
                continue;
            }
            if let Some(name) = names.iter().find(|n| d.message.contains(&format!("`{n}`"))) {
                d.message = format!(
                    "`{name}` is bound by another binding of this `and` group, which runs independently of it: bind it in a statement before the group"
                );
            }
        }
    }
}

/// `a1 -> … -> an -> result`, pure.
fn curried(items: &[Type], result: Type) -> Type {
    items
        .iter()
        .rev()
        .fold(result, |r, a| Type::pure_fun(a.clone(), r))
}

/// The names `pattern` binds.
fn binders(pattern: &Node, text: &str, out: &mut Vec<String>) {
    if pattern.kind == K::VarPat {
        let (start, end) = pattern.range();
        out.push(text[start as usize..end as usize].trim().to_owned());
    }
    for child in pattern.nodes() {
        binders(child, text, out);
    }
}
