//! Chains of binary operators and pipelines.

use flick_pretty::docvec;

use super::expr::Part;
use super::printer::{Doc, INDENT, Printer};
use crate::syntax::cst::{Element, Node, Token};
use crate::syntax::kind::SyntaxKind as K;

/// Binding strength of a binary operator: the parser's left power.
fn precedence(kind: K) -> u8 {
    kind.binding_power().map_or(0, |(left, _)| left)
}

fn operator(n: &Node) -> Option<Token> {
    n.child_tokens().find(|t| t.kind.is_binary_op())
}

/// Whether `operand` under binary operator `parent` mixes `orelse` with a
/// pipeline operator (`|>` or `<|`) so explicit parentheses should display
/// the parsed grouping.
fn needs_mixed_parens(parent: K, operand: &Node) -> bool {
    if operand.kind != K::BinExpr {
        return false;
    }
    let Some(child_op) = operator(operand) else {
        return false;
    };
    matches!(
        (parent, child_op.kind),
        (K::OrElse, K::PipeGt) | (K::LtPipe, K::OrElse)
    )
}

/// The operands and operators of a chain of one precedence, in order.
fn chain<'n>(n: &'n Node, level: u8, operands: &mut Vec<&'n Node>, operators: &mut Vec<Token>) {
    for child in &n.children {
        match child {
            Element::Node(c)
                if c.kind == K::BinExpr
                    && operator(c).is_some_and(|o| precedence(o.kind) == level) =>
            {
                chain(c, level, operands, operators);
            }
            Element::Node(c) => operands.push(c),
            Element::Token(t) if t.kind.is_binary_op() => operators.push(*t),
            Element::Token(_) => {}
        }
    }
}

/// Whether the token starts its line.
fn starts_line(text: &str, t: Token) -> bool {
    let before = &text[..t.start as usize];
    before
        .rsplit('\n')
        .next()
        .is_some_and(|l| l.trim().is_empty())
}

/// Whether the token ends its line.
fn ends_line(text: &str, t: Token) -> bool {
    let after = &text[t.end as usize..];
    after
        .split('\n')
        .next()
        .is_some_and(|l| l.trim().is_empty())
}

impl<'a> Printer<'a> {
    fn binary_operand(&mut self, parent: K, operand: &'a Node, suffix: &[Part]) -> Doc<'a> {
        if needs_mixed_parens(parent, operand) {
            let closing: Vec<Part> = [Part::Text(")")]
                .into_iter()
                .chain(suffix.iter().copied())
                .collect();
            let inner = self.with_suffix(operand, &closing);
            docvec![self.a, "(", inner]
        } else {
            self.with_suffix(operand, suffix)
        }
    }

    /// A chain of operators of one precedence: on one line, or broken before
    /// each operator. A pipeline breaks one step per line, stays broken when
    /// written so, and lines its steps up under the first operand when it
    /// starts its line (`aligned`); other chains go one level in.
    pub fn binary(&mut self, n: &'a Node, aligned: bool, suffix: &[Part]) -> Doc<'a> {
        let a = self.a;
        let Some(op) = operator(n) else {
            return self.generic(n);
        };
        let parent_op = op.kind;
        let mut operands = Vec::new();
        let mut operators = Vec::new();
        chain(n, precedence(parent_op), &mut operands, &mut operators);
        if operands.len() != operators.len() + 1 {
            return self.generic(n);
        }
        let pipe = matches!(parent_op, K::PipeGt | K::LtPipe);
        // a step on a line of its own, whichever end of a line its operator is on
        let on_lines = |&o: &Token| starts_line(self.text, o) || ends_line(self.text, o);
        let broken = pipe && operators.iter().any(on_lines);
        let mut forced = broken;
        let mut first = self.binary_operand(parent_op, operands[0], &[]);
        if let Some(c) = self.comments.trailing(operators[0].start, None) {
            first = docvec![a, first, " ", c];
            forced = true;
        }
        let mut steps = Vec::with_capacity(operators.len());
        for (i, (&op, &operand)) in operators.iter().zip(&operands[1..]).enumerate() {
            let comments = self.comments.leading(op.start);
            let comments = if comments.is_empty() {
                a.empty()
            } else {
                forced = true;
                self.comment_lines(&comments)
            };
            let op = self.tok(op);
            let last = i + 1 == operators.len();
            let mut operand = if last {
                self.binary_operand(parent_op, operand, suffix)
            } else {
                self.binary_operand(parent_op, operand, &[])
            };
            if let Some(next) = operators.get(i + 1)
                && let Some(c) = self.comments.trailing(next.start, None)
            {
                operand = docvec![a, operand, " ", c];
                forced = true;
            }
            let separator = if broken { a.line() } else { a.break_("", " ") };
            steps.push(docvec![a, separator, comments, op, " ", operand]);
        }
        let steps = a.concat(steps);
        let steps = if pipe && aligned {
            steps
        } else {
            steps.nest(a, INDENT)
        };
        let doc = docvec![a, first, steps];
        if forced {
            doc.force_break(a).group(a)
        } else {
            doc.group(a)
        }
    }
}
