//! Applications, operators, parentheses, and functions.

use std::mem;

use flick_pretty::{NextBreakFitsMode, docvec};

use super::block::Value;
use super::lines::{child, children, commented};
use super::printer::{Calls, Doc, INDENT, Printer, nodes, position, token};
use crate::syntax::cst::{Element, Node, Token};
use crate::syntax::kind::SyntaxKind as K;

/// A piece of what follows an expression on its line: closing
/// parentheses, a separator. It goes inside the expression's group, so
/// that the group makes room for it.
#[derive(Clone, Copy)]
pub(super) enum Part {
    Token(Token),
    Text(&'static str),
}

/// Binding strength of a binary operator: the parser's left power.
fn precedence(kind: K) -> u8 {
    kind.binding_power().map_or(0, |(left, _)| left)
}

pub(super) fn operator(n: &Node) -> Option<Token> {
    n.child_tokens().find(|t| t.kind.is_binary_op())
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

/// A last argument that may stay on the line of the call and break inside:
/// `f a (fun x ->`, `f a [`, `f a {`, `f task {`.
fn hugs(n: &Node) -> bool {
    match n.kind {
        K::ParenExpr => n.nodes().next().is_some_and(|c| c.kind == K::FunExpr),
        K::ListExpr | K::RecordExpr | K::RecordUpdate | K::TupleExpr => n.nodes().next().is_some(),
        K::TaskExpr | K::ScopeExpr | K::ProvideExpr | K::ResultExpr | K::DoExpr => true,
        K::PluginExpr | K::LazyExpr => n.has_token(K::LBrace),
        _ => false,
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
    /// `f a b`: on one line, or the arguments one per line one level in. A
    /// last argument that hugs keeps the call on its line when what comes
    /// before its first break fits.
    pub fn app(&mut self, n: &'a Node, suffix: &[Part]) -> Doc<'a> {
        let a = self.a;
        let parts = nodes(n);
        // the lines under a call, each a child: the call on its line, then
        // a line for each, one level in
        if let Some((lines, call)) = parts
            .split_last()
            .filter(|(last, _)| last.kind == K::Children)
        {
            // the parser reads lines here, whatever is around
            let outer = mem::replace(&mut self.calls, Calls::Lines);
            let call = match call {
                [head] => self.node(head),
                _ => self.call(n, call, &[], None),
            };
            let children = nodes(lines);
            let count = children.len();
            let mut at = 0;
            let items = self.items(&children, n.range().1, None, &mut |p, line| {
                at += 1;
                // a call in parentheses on a line of its own is that call
                let suffix = if at == count { suffix } else { &[] };
                let doc = p.with_suffix(line, suffix);
                let (start, end) = line.range();
                match child(line).filter(|_| !commented(line, start, end)) {
                    Some(call) => {
                        let bare = p.bare(call, suffix, doc);
                        // its `)` is passed: a comment after it ends the line
                        p.comments.cursor = p.comments.cursor.max(end);
                        bare
                    }
                    None => doc,
                }
            });
            self.calls = outer;
            let under = docvec![a, a.line(), self.join_lines(&items)].nest(a, INDENT);
            return docvec![a, call, under].force_break(a);
        }
        self.call(n, &parts, suffix, None)
    }

    /// A function and its arguments: on one line, or an argument a line.
    /// Where the lines under a call are its arguments, its arguments stay
    /// on its line, but the calls in parentheses that end them go on lines
    /// of their own under it, without the parentheses, when it does not fit.
    /// `fallback` is laid out instead where the first line of the call
    /// would not fit either.
    fn call(
        &mut self,
        n: &'a Node,
        parts: &[&'a Node],
        suffix: &[Part],
        fallback: Option<Doc<'a>>,
    ) -> Doc<'a> {
        let a = self.a;
        let Some((head_node, args)) = parts.split_first() else {
            return self.generic(n);
        };
        let mut commented = false;
        let mut head = self.node(head_node);
        if let Some(c) = args
            .first()
            .and_then(|f| self.comments.trailing(f.range().0, None))
        {
            head = docvec![a, head, " ", c].force_break(a);
            commented = true;
        }
        // the last argument holds the suffix: it ends the line both when the
        // call fits and when its arguments go one per line
        let inside = !args.is_empty();
        let mut docs = Vec::with_capacity(args.len());
        for (i, arg) in args.iter().enumerate() {
            let comments = self.comments.leading(arg.range().0);
            let comments = if comments.is_empty() {
                a.empty()
            } else {
                commented = true;
                self.comment_lines(&comments)
            };
            let mut doc = if inside && i + 1 == args.len() {
                self.with_suffix(arg, suffix)
            } else {
                self.node(arg)
            };
            if let Some(next) = args.get(i + 1)
                && let Some(c) = self.comments.trailing(next.range().0, None)
            {
                doc = docvec![a, doc, " ", c].force_break(a);
                commented = true;
            }
            docs.push((comments, doc));
        }
        // where a line under a call is a child of it, its arguments stay on
        // its line
        let fixed = self.calls == Calls::Lines;
        let between = if fixed {
            a.text(" ")
        } else {
            a.break_("", " ")
        };
        let hugging = |doc: Doc<'a>| doc.group(a).next_break_fits(a, NextBreakFitsMode::Enabled);
        // the call with its first `count` arguments, the last of them
        // hugging when it can; `whole`, groups around measure it in full
        let line = |count: usize, closing: Doc<'a>, whole: bool| {
            let hug = !commented && count > 0 && hugs(args[count - 1]);
            let args = docs[..count]
                .iter()
                .enumerate()
                .map(|(i, &(comments, doc))| {
                    let doc = if hug && i + 1 == count {
                        hugging(doc)
                    } else {
                        doc
                    };
                    docvec![a, between, comments, doc]
                });
            let doc = docvec![a, head, a.concat(args).nest_if_broken(a, INDENT), closing].group(a);
            if hug && whole {
                // outer groups measure the call in full, only its own group
                // lets the last argument hug
                doc.next_break_fits(a, NextBreakFitsMode::Disabled)
            } else {
                doc
            }
        };
        let closing = if inside {
            a.empty()
        } else {
            self.suffix(suffix)
        };
        let moved = if fixed {
            children(n, head_node, args)
        } else {
            0
        };
        // what measures a line of its own measures up to where it hugs
        let doc = line(docs.len(), closing, fallback.is_none() || moved > 0);
        if moved == 0 {
            return fallback.map_or(doc, |other| a.either(doc, other));
        }
        // the lines are laid out once more, without comments to take twice
        let kept = docs.len() - moved;
        let (read, misplaced) = (self.comments.save(), self.comments.misplaced);
        let mut lines = Vec::with_capacity(2 * moved);
        for i in kept..docs.len() {
            let last = if i + 1 == docs.len() { suffix } else { &[] };
            if let Some(call) = child(args[i]) {
                lines.push(a.line());
                lines.push(self.bare(call, last, docs[i].1));
            }
        }
        self.comments.restore(read);
        self.comments.misplaced = misplaced;
        let lines = a.concat(lines).nest(a, INDENT);
        // only this layout measures its first line, up to where it hugs
        let broken = docvec![a, line(kept, a.empty(), false), lines];
        let broken = fallback.map_or(broken, |other| a.either(broken, other));
        a.either(doc, broken)
    }

    /// `call`, which `paren` lays out in its parentheses, on a line of its
    /// own: without them where its first line fits, in them, an argument a
    /// line, otherwise.
    fn bare(&mut self, call: &'a Node, suffix: &[Part], paren: Doc<'a>) -> Doc<'a> {
        let parts = nodes(call);
        self.call(call, &parts, suffix, Some(paren))
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
        let mut operands = Vec::new();
        let mut operators = Vec::new();
        chain(n, precedence(op.kind), &mut operands, &mut operators);
        if operands.len() != operators.len() + 1 {
            return self.generic(n);
        }
        let pipe = matches!(op.kind, K::PipeGt | K::LtPipe);
        // a step on a line of its own, whichever end of a line its operator is on
        let on_lines = |&o: &Token| starts_line(self.text, o) || ends_line(self.text, o);
        let broken = pipe && operators.iter().any(on_lines);
        let mut forced = broken;
        let mut first = self.node(operands[0]);
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
                self.with_suffix(operand, suffix)
            } else {
                self.node(operand)
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

    /// `(e)`: hugged by its parentheses. A function in parentheses closes
    /// on its own line when its body breaks; a `match`, `try` or broken `if`
    /// goes on lines of its own between them.
    pub fn paren(&mut self, n: &'a Node, suffix: &[Part]) -> Doc<'a> {
        let outer = self.calls;
        if outer == Calls::Lines {
            self.calls = Calls::Bracketed;
        }
        let doc = self.parenthesized(n, suffix);
        self.calls = outer;
        doc
    }

    fn parenthesized(&mut self, n: &'a Node, suffix: &[Part]) -> Doc<'a> {
        let a = self.a;
        let inner = nodes(n);
        let (Some(open), Some(close), [inner]) =
            (token(n, K::LParen), token(n, K::RParen), inner.as_slice())
        else {
            let doc = self.generic(n);
            let suffix = self.suffix(suffix);
            return docvec![a, doc, suffix];
        };
        let open = self.tok(open);
        let closing: Vec<Part> = [Part::Token(close)]
            .into_iter()
            .chain(suffix.iter().copied())
            .collect();
        match inner.kind {
            K::FunExpr => {
                let fun = self.fun(inner, &closing);
                docvec![a, open, fun]
            }
            K::MatchExpr | K::TryExpr | K::CatchExpr | K::IfExpr => {
                let inner = self.statement(inner);
                let inner = docvec![a, a.break_("", ""), inner].nest(a, INDENT);
                let close = self.suffix(&closing);
                docvec![a, open, inner, a.break_("", ""), close].group(a)
            }
            _ => {
                let inner = self.with_suffix(inner, &closing);
                docvec![a, open, inner]
            }
        }
    }

    /// `n` followed by `suffix`, inside its group when it has one, so that
    /// the group makes room for it.
    pub fn with_suffix(&mut self, n: &'a Node, suffix: &[Part]) -> Doc<'a> {
        if suffix.is_empty() {
            return self.node(n);
        }
        match n.kind {
            K::AppExpr | K::AppType => self.app(n, suffix),
            K::BinExpr => self.binary(n, false, suffix),
            K::ParenExpr => self.paren(n, suffix),
            K::ListExpr | K::TupleExpr | K::ListPat | K::TuplePat | K::TupleType => {
                self.bracket(n, suffix)
            }
            K::RecordExpr | K::RecordUpdate | K::RecordType | K::RecordPat => {
                self.record(n, suffix)
            }
            K::TaskExpr
            | K::LazyExpr
            | K::ScopeExpr
            | K::ProvideExpr
            | K::ResultExpr
            | K::DoExpr => self.braced(n, suffix),
            _ => {
                let doc = self.node(n);
                let suffix = self.suffix(suffix);
                docvec![self.a, doc, suffix]
            }
        }
    }

    pub fn suffix(&mut self, suffix: &[Part]) -> Doc<'a> {
        let a = self.a;
        let mut docs = Vec::with_capacity(suffix.len());
        for part in suffix {
            docs.push(match *part {
                Part::Token(t) => self.tok(t),
                Part::Text(text) => a.text(text),
            });
        }
        a.concat(docs)
    }

    /// `fun params -> body`, then the `)` and the rest of `closing` when it
    /// is in parentheses.
    pub fn fun(&mut self, n: &'a Node, closing: &[Part]) -> Doc<'a> {
        let (Some(arrow), Some(body)) = (position(n, K::Arrow), n.nodes().last()) else {
            let doc = self.generic(n);
            let closing = self.suffix(closing);
            return docvec![self.a, doc, closing];
        };
        if body.kind != K::Block {
            let doc = self.generic(n);
            let closing = self.suffix(closing);
            return docvec![self.a, doc, closing];
        }
        self.inline_body = mem::take(&mut self.tail_inline);
        // the parameters are measured whole when the function hugs, so that a
        // tuple pattern does not end the line the hug must fit in
        let head = self
            .elements(n, &n.children[..=arrow])
            .next_break_fits(self.a, NextBreakFitsMode::Disabled);
        self.body(head, Value::Block(body), n.range().1, closing)
    }
}
