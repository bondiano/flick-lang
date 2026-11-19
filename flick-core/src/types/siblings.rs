//! The lines of a block that opens a module and names a function of its
//! siblings, as `ui { … }` names `Ui.fragment`. More than one expression
//! line, after the statements, is that function of the list of them; a
//! statement after the first child is `T085`, and one line that is the
//! function of children written one by one is `T086`, since the lines say
//! the same.

use super::children::arguments;
use super::sugar_lints::unparen;
use super::typing::Ref;
use super::{Checker, let_parts};
use crate::diagnostics::{Diagnostic, Edit, Severity, codes};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::ty::Type;

impl Checker<'_> {
    /// `body`, the lines of a block that opens a module and names `function`
    /// of them. More than one expression line, after the statements, is the
    /// function of the list of them, each line one child; a statement after
    /// a child is `T085`.
    pub(super) fn siblings(&mut self, body: &Node, function: &Node) -> Type {
        let Some((reference, scheme)) = self.through(function) else {
            return self.infer(body);
        };
        let block = unparen(body);
        let lines: Vec<&Node> = block.nodes().filter(|n| !is_statement(n.kind)).collect();
        let Some(first) = lines
            .first()
            .filter(|_| block.kind == K::Block && lines.len() > 1)
        else {
            let ty = self.infer(body);
            let line = if block.kind == K::Block {
                lines.first()
            } else {
                Some(&block)
            };
            if let Some(line) = line {
                self.wrapped(line, &reference);
            }
            return ty;
        };
        let span = self.span(block);
        let function = self.instantiate(&scheme, span);
        let Some(parts) = self.of_children(&function) else {
            return self.infer(body);
        };
        let mark = self.cx.locals.len();
        let statements: Vec<&Node> = block.nodes().collect();
        let mut i = 0;
        while i < statements.len() {
            let statement = statements[i];
            if statement.range().0 > first.range().0 && is_statement(statement.kind) {
                self.late_statement(statement, &statements[..i], first);
            }
            match statement.kind {
                K::Signature | K::LetDecl => i += self.local_declaration(&statements, i),
                K::AssignStmt => self.statement(statement),
                _ => {}
            }
            i += 1;
        }
        let ty = self.applied_to_children(&function, &parts, &lines, None, span);
        self.cx.locals.truncate(mark);
        self.typing.siblings.insert(block.range(), reference);
        ty
    }

    /// `T085` on `statement`, which follows `first`, the first child of its
    /// block, among `before`. A fix moves a `let` of a name above that child
    /// where nothing changes meaning: no line between names what it binds,
    /// which it would then hide, and none binds a name it uses or declares.
    fn late_statement(&mut self, statement: &Node, before: &[&Node], first: &Node) {
        let text = self.text;
        let words = |n: &Node| -> Vec<String> {
            let names = n.tokens().into_iter().filter(|t| t.kind == K::Ident);
            names.map(|t| t.text(text).to_owned()).collect()
        };
        let bound = |n: &Node| {
            (n.kind == K::LetDecl)
                .then(|| let_parts(n, text).name)
                .flatten()
        };
        let between = before.iter().filter(|n| n.range().0 >= first.range().0);
        let fixable = bound(statement).is_some_and(|name| {
            let used = words(statement);
            !between.into_iter().any(|n| {
                n.kind == K::Signature
                    || words(n).contains(&name)
                    || bound(n).is_some_and(|other| used.contains(&other))
            })
        });
        let diagnostic = Diagnostic::error(
            codes::T085,
            "a statement after the children of this block: its `let`s and assignments come before the first child",
            self.span(statement),
        );
        // its lines move as they are, before the line of the first child,
        // when each of the two stands alone on its lines
        let offset = |at: usize| u32::try_from(at).expect("offsets fit in u32");
        let line_start = |at: u32| text[..at as usize].rfind('\n').map_or(0, |i| i + 1);
        let ((start, end), child) = (statement.range(), first.range().0);
        let to = text[end as usize..]
            .find('\n')
            .map(|i| end as usize + i + 1);
        let (from, at) = (line_start(start), line_start(child));
        let alone = to.is_some_and(|to| text[end as usize..to].trim().is_empty())
            && text[from..start as usize].trim().is_empty()
            && text[at..child as usize].trim().is_empty();
        let (Some(to), true, true) = (to, fixable, alone) else {
            return (self.diagnostics).push(diagnostic);
        };
        let at = offset(at);
        let edits = vec![
            Edit {
                span: Span::new(self.file, at, at),
                replacement: text[from..to].to_owned(),
            },
            Edit {
                span: Span::new(self.file, offset(from), offset(to)),
                replacement: String::new(),
            },
        ];
        let fix = || ("move it above the first child", edits);
        (self.diagnostics).push(diagnostic.with_safe_fix(fix));
    }

    /// `T086` on `line`, the one line of a block whose lines are the
    /// children of `function`, when it is that function of two or more
    /// children written one by one: the lines say it without the call. The
    /// fix writes the children as the lines, where each stands alone.
    fn wrapped(&mut self, line: &Node, function: &Ref) {
        let call = unparen(line);
        let nodes: Vec<&Node> = call.nodes().collect();
        let Some((head, written)) = nodes.split_first() else {
            return;
        };
        let args = arguments(written);
        let (Some(first), Some(last)) = (args.first(), args.last()) else {
            return;
        };
        let gathered = self.typing.gathered.get(&(head.range().0, last.range().1));
        let named = self.typing.names.get(&head.range());
        if call.kind != K::AppExpr
            || named != Some(function)
            || gathered != Some(&0)
            || args.len() < 2
        {
            return;
        }
        let text = self.text;
        let name = head.text(text);
        let message = format!(
            "the lines of this block are the children of one `{name}` already: write them without it"
        );
        let diagnostic =
            Diagnostic::new(codes::T086, Severity::Warning, message).with_span(self.span(head));
        let Some(edits) = self.unwrapped(head, written, (first, last)) else {
            return self.diagnostics.push(diagnostic);
        };
        let fix = || (format!("write the children without `{name}`"), edits);
        self.diagnostics.push(diagnostic.with_safe_fix(fix));
    }

    /// The edits that write the children `first` to `last`, the arguments
    /// `written` of `head`, as the lines of its block: lines under the call
    /// move to its column, and children on its line go each on a line of
    /// its own, or after a `;` on the line of the block's head. `None` when
    /// the call mixes the two or a child on its line takes more than one.
    fn unwrapped(
        &self,
        head: &Node,
        written: &[&Node],
        (first, last): (&Node, &Node),
    ) -> Option<Vec<Edit>> {
        let text = self.text;
        let line_start = |at: u32| text[..at as usize].rfind('\n').map_or(0, |i| i + 1);
        let offset = |at: usize| u32::try_from(at).expect("offsets fit in u32");
        let edit = |start: usize, end: usize, replacement: String| Edit {
            span: Span::new(self.file, offset(start), offset(end)),
            replacement,
        };
        let ((start, end), from) = (head.range(), line_start(head.range().0));
        let indent = &text[from..start as usize];
        let alone = indent.trim().is_empty();
        if written.iter().all(|n| n.kind == K::Children) {
            // the line of the head goes; the lines under it move left
            let next = end as usize + text[end as usize..].find('\n')? + 1;
            let delta = first.range().0 as usize - line_start(first.range().0) - indent.len();
            if !alone || !text[end as usize..next].trim().is_empty() {
                return None;
            }
            let mut edits = vec![edit(from, next, String::new())];
            let mut at = next;
            while at < last.range().1 as usize {
                let spaces = text[at..]
                    .bytes()
                    .take(delta)
                    .take_while(|b| *b == b' ')
                    .count();
                if spaces == delta {
                    edits.push(edit(at, at + delta, String::new()));
                }
                at += text[at..].find('\n').map_or(text.len() - at, |i| i + 1);
            }
            return Some(edits);
        }
        if written.iter().any(|n| n.kind == K::Children) {
            return None;
        }
        let pieces: Vec<String> = written.iter().map(|n| unparen(n).text(text)).collect();
        if pieces.iter().any(|p| p.contains('\n')) {
            return None;
        }
        let between = if alone {
            format!("\n{indent}")
        } else {
            "; ".to_owned()
        };
        Some(vec![edit(
            start as usize,
            last.range().1 as usize,
            pieces.join(&between),
        )])
    }
}

/// Whether a line of a block of this kind is a statement rather than an
/// expression.
fn is_statement(kind: K) -> bool {
    matches!(kind, K::Signature | K::LetDecl | K::AssignStmt)
}
