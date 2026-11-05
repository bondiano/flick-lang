//! Applications, operators, parentheses, functions, `if`, `match` and `try`.

use std::{mem, ptr};

use flick_pretty::{NextBreakFitsMode, docvec};

use super::block::Value;
use super::printer::{Doc, INDENT, Printer, nodes, position, token};
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

/// Binding strength of a binary operator, weakest first.
fn precedence(kind: K) -> u8 {
    match kind {
        K::PipeGt => 1,
        K::OrOr => 2,
        K::AndAnd => 3,
        K::EqEq | K::NotEq | K::Lt | K::Le | K::Gt | K::Ge => 4,
        K::PlusPlus => 5,
        K::Plus | K::Minus => 6,
        K::Star | K::Slash | K::Percent => 7,
        _ => 8,
    }
}

fn operator(n: &Node) -> Option<Token> {
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
        K::TaskExpr | K::UiExpr | K::ScopeExpr | K::ProvideExpr | K::DoExpr | K::SqlExpr => true,
        K::PluginExpr => n.has_token(K::LBrace),
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

impl<'a> Printer<'a> {
    /// `f a b`: on one line, or the arguments one per line one level in. A
    /// last argument that hugs keeps the call on its line when what comes
    /// before its first break fits.
    pub fn app(&mut self, n: &'a Node, suffix: &[Part]) -> Doc<'a> {
        let a = self.a;
        let parts = nodes(n);
        let Some((head, args)) = parts.split_first() else {
            return self.generic(n);
        };
        let mut commented = false;
        let mut head = self.node(head);
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
        let hug = !commented && args.last().is_some_and(|l| hugs(l));
        let count = docs.len();
        let args = a.concat(docs.into_iter().enumerate().map(|(i, (comments, doc))| {
            let doc = if hug && i + 1 == count {
                doc.group(a).next_break_fits(a, NextBreakFitsMode::Enabled)
            } else {
                doc
            };
            docvec![a, a.break_("", " "), comments, doc]
        }));
        let suffix = if inside {
            a.empty()
        } else {
            self.suffix(suffix)
        };
        let doc = docvec![a, head, args.nest_if_broken(a, INDENT), suffix];
        if hug {
            // outer groups measure the call in full, only its own group lets
            // the last argument hug
            doc.group(a).next_break_fits(a, NextBreakFitsMode::Disabled)
        } else {
            doc.group(a)
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
        let mut operands = Vec::new();
        let mut operators = Vec::new();
        chain(n, precedence(op.kind), &mut operands, &mut operators);
        if operands.len() != operators.len() + 1 {
            return self.generic(n);
        }
        let pipe = op.kind == K::PipeGt;
        let broken = pipe && operators.iter().any(|&o| starts_line(self.text, o));
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
            K::MatchExpr | K::TryExpr | K::IfExpr => {
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
            K::TaskExpr | K::UiExpr | K::ScopeExpr | K::ProvideExpr | K::DoExpr | K::SqlExpr => {
                self.braced(n, suffix)
            }
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

    /// `if c then a else b` on one line, or each branch on its own lines.
    pub fn if_(&mut self, n: &'a Node) -> Doc<'a> {
        let a = self.a;
        let inline = mem::take(&mut self.tail_inline);
        let mut docs = Vec::new();
        let mut forced = false;
        let mut current = n;
        loop {
            let parts = nodes(current);
            // the condition is the first child node, the `then` branch the second
            let (Some(then), Some(&block)) = (position(current, K::ThenKw), parts.get(1)) else {
                return self.generic(n);
            };
            let head = self.elements(current, &current.children[..=then]);
            let otherwise = token(current, K::ElseKw);
            let end = otherwise.map_or(current.range().1, |t| t.start);
            let (branch, broken) = self.branch(block, end, inline && otherwise.is_none());
            forced |= broken;
            docs.push(head);
            docs.push(branch);
            let Some(otherwise) = otherwise else {
                break;
            };
            let comments = self.comments.leading(otherwise.start);
            if comments.is_empty() {
                docs.push(a.break_("", " "));
            } else {
                forced = true;
                docs.push(a.line());
                docs.push(self.comment_lines(&comments));
            }
            docs.push(self.tok(otherwise));
            match parts.get(2) {
                Some(next) if next.kind == K::IfExpr => {
                    docs.push(a.text(" "));
                    current = next;
                }
                Some(block) => {
                    let (branch, broken) = self.branch(block, current.range().1, inline);
                    forced |= broken;
                    docs.push(branch);
                    break;
                }
                None => return self.generic(n),
            }
        }
        let doc = a.concat(docs);
        if forced {
            doc.force_break(a).group(a)
        } else {
            doc.group(a)
        }
    }

    /// A branch of an `if`, one level in; whether it must break.
    /// `inline` keeps it on the line of its `then` or `else`.
    fn branch(&mut self, block: &'a Node, end: u32, inline: bool) -> (Doc<'a>, bool) {
        let a = self.a;
        let statements = nodes(block);
        let first = statements.first().map_or(end, |s| s.range().0);
        let after = self.comments.trailing(first, None);
        // `else` and an `if` on the next line differ from `else if`
        let nested_if = |one: &Node| one.kind == K::IfExpr && !inline;
        if let ([one], None) = (statements.as_slice(), after)
            && one.kind != K::LetDecl
            && one.kind != K::Signature
            && !nested_if(one)
            && !self.comments.any_before(first)
        {
            // what ends the line of an inline branch ends the branch's value too
            self.tail_inline =
                inline && matches!(one.kind, K::IfExpr | K::MatchExpr | K::TryExpr | K::FunExpr);
            let mut doc = self.statement(one);
            self.tail_inline = false;
            let trailing = self.comments.trailing(end, None);
            if let Some(c) = trailing {
                doc = docvec![a, doc, " ", c];
            }
            if inline {
                // the lines of the value go deeper than its `else`
                return (docvec![a, " ", doc].nest(a, INDENT), trailing.is_some());
            }
            let doc = docvec![a, a.break_("", " "), doc].nest(a, INDENT);
            return (doc, trailing.is_some());
        }
        let items = self.items(&statements, end, Some(';'), &mut |p, s| p.statement(s));
        let after = after.map_or(a.empty(), |c| docvec![a, " ", c]);
        let lines = docvec![a, a.line(), self.join_lines(&items)].nest(a, INDENT);
        (docvec![a, after, lines], true)
    }

    /// `match e with` or `try e with`, then one arm per line.
    pub fn match_(&mut self, n: &'a Node) -> Doc<'a> {
        let a = self.a;
        let inline = mem::take(&mut self.tail_inline);
        let Some(with) = position(n, K::WithKw) else {
            return self.generic(n);
        };
        let head = if n.kind == K::TryExpr {
            self.try_head(n, with)
        } else {
            let (Some(keyword), Some(subject)) = (token(n, K::MatchKw), n.nodes().next()) else {
                return self.generic(n);
            };
            let keyword = self.tok(keyword);
            let subject = self.node(subject);
            let subject = docvec![a, a.break_("", " "), subject].nest(a, INDENT);
            let Element::Token(with) = n.children[with] else {
                unreachable!("a token position")
            };
            let with = self.tok(with);
            docvec![a, keyword, subject, a.break_("", " "), with].group(a)
        };
        let arms: Vec<&Node> = n.nodes().filter(|c| c.kind == K::MatchArm).collect();
        let last = arms.last().copied();
        let items = self.items(&arms, n.range().1, None, &mut |p, arm| {
            p.inline_body = inline && last.is_some_and(|l| ptr::eq(l, arm));
            p.arm(arm)
        });
        docvec![a, head, a.line(), self.join_lines(&items)].force_break(a)
    }

    /// `try body with`: one line when the body is one expression that fits.
    fn try_head(&mut self, n: &'a Node, with: usize) -> Doc<'a> {
        let a = self.a;
        let (Some(keyword), Some(block)) = (token(n, K::TryKw), n.nodes().next()) else {
            return self.generic(n);
        };
        let keyword = self.tok(keyword);
        let Element::Token(with) = n.children[with] else {
            unreachable!("a token position")
        };
        let (branch, broken) = self.branch(block, with.start, false);
        let with = self.tok(with);
        let separator = if broken { a.line() } else { a.break_("", " ") };
        let doc = docvec![a, keyword, branch, separator, with];
        if broken {
            doc.force_break(a)
        } else {
            doc.group(a)
        }
    }

    /// `| pattern when guard -> body`.
    fn arm(&mut self, n: &'a Node) -> Doc<'a> {
        let (Some(arrow), Some(block)) = (position(n, K::Arrow), n.nodes().last()) else {
            return self.generic(n);
        };
        if block.kind != K::Block {
            return self.generic(n);
        }
        let head = self.elements(n, &n.children[..=arrow]);
        self.body(head, Value::Block(block), n.range().1, &[])
    }
}
