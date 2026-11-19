//! The children of a call: in a block that opens a module and names the type
//! of a child, a function whose last parameter is a list of that type takes
//! the list element by element. Each argument from that parameter on is an
//! element, on the line of the call or on a line of its own under it, and
//! none is the empty list. The call then stands for the function applied to
//! the list of them, which is what lowering writes. A block that also names
//! a function of its siblings gives it the list of its expression lines,
//! when there is more than one.

use std::ptr;

use super::comprehension::comprehension;
use super::lang::Lang;
use super::ranges::range_list;
use super::sugar_lints::unparen;
use super::ty::{Effects, Norm, Type};
use super::ui::Opened;
use super::{Checker, let_parts};
use crate::diagnostics::{Diagnostic, Edit, codes};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::heads::SIBLINGS;
use crate::syntax::kind::SyntaxKind as K;

/// Whether `target`, somewhere under `node`, stands inside `( … )`, `[ … ]`,
/// a record or the hole of a string with no block of lines in between:
/// there a line break means nothing. `None` when it is not under `node`.
fn bracketed(node: &Node, target: &Node, inside: bool) -> Option<bool> {
    if ptr::eq(node, target) {
        return Some(inside);
    }
    let inside = match node.kind {
        K::ParenExpr
        | K::ListExpr
        | K::TupleExpr
        | K::RecordExpr
        | K::RecordUpdate
        | K::InterpExpr => true,
        K::Block | K::Children => false,
        _ => inside,
    };
    node.nodes()
        .find_map(|child| bracketed(child, target, inside))
}

/// Whether `node` needs no parentheses to be an argument on a line.
fn atomic(node: &Node) -> bool {
    matches!(
        node.kind,
        K::NameExpr | K::Literal | K::ParenExpr | K::UnitExpr | K::RecordExpr | K::ListExpr
    )
}

/// The arguments of a call as written: those on its line, then each line
/// under it.
pub fn arguments<'n>(written: &[&'n Node]) -> Vec<&'n Node> {
    let line = |n: &&'n Node| {
        if n.kind == K::Children {
            n.nodes().collect()
        } else {
            vec![*n]
        }
    };
    written.iter().flat_map(line).collect()
}

impl Checker<'_> {
    /// `body` with a module open, and maybe the type of a child.
    pub(super) fn open_module(&mut self, node: &Node) -> Type {
        let path = node.child_tokens().filter(|t| t.kind == K::Con);
        let module: Vec<&str> = path.map(|t| t.text(self.text)).collect();
        let child = node.nodes().find(|n| n.kind == K::NameType);
        let child = child.map(|written| self.annotation(written));
        // the body is the last node; before it the function of its lines
        // after its word, and before that the function values are applied
        // through
        let mut parts: Vec<&Node> = node.nodes().filter(|n| n.kind != K::NameType).collect();
        let body = parts.pop();
        let marked =
            (node.child_tokens()).any(|t| t.kind == K::Ident && t.text(self.text) == SIBLINGS);
        let siblings = if marked { parts.pop() } else { None };
        let through = parts.pop().and_then(|written| self.through(written));
        self.opened.push(Opened {
            module: module.join("."),
            child,
            through,
        });
        let ty = match (body, siblings) {
            (Some(body), Some(function)) => self.siblings(body, function),
            (Some(body), None) => self.infer(body),
            (None, _) => Type::Error,
        };
        self.opened.pop();
        ty
    }

    /// `body`, the lines of a block that opens a module and names `function`
    /// of them. More than one expression line, after the statements, is the
    /// function of the list of them, each line one child; a statement after
    /// a child is `T085`.
    fn siblings(&mut self, body: &Node, function: &Node) -> Type {
        let Some((reference, scheme)) = self.through(function) else {
            return self.infer(body);
        };
        let block = unparen(body);
        let lines: Vec<&Node> = block.nodes().filter(|n| !is_statement(n.kind)).collect();
        let Some(first) = lines
            .first()
            .filter(|_| block.kind == K::Block && lines.len() > 1)
        else {
            return self.infer(body);
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
        let to = text[end as usize..].find('\n').map(|i| end as usize + i + 1);
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

    /// The elements of `written`, a list literal where the children of
    /// `head` go one by one, refused with the fix that writes them so: the
    /// brackets and commas go, and an element that is no line of its own
    /// under a call starting its line gets parentheses.
    fn listed<'n>(&mut self, head: &Node, written: &'n Node) -> Option<Vec<&'n Node>> {
        let list = unparen(written);
        if list.kind != K::ListExpr
            || !ptr::eq(list, written)
            || range_list(list).is_some()
            || comprehension(list).is_some()
        {
            return None;
        }
        let elements: Vec<&Node> = list.nodes().collect();
        let starts_line = |at: u32| {
            let before = &self.text[..at as usize];
            before[before.rfind('\n').map_or(0, |i| i + 1)..]
                .trim()
                .is_empty()
        };
        // lines under the call are its children only outside brackets
        let lines = starts_line(head.range().0)
            && self
                .root
                .is_some_and(|root| bracketed(root, list, false) == Some(false));
        let at = |start: u32, end: u32| Span::new(self.file, start, end);
        let mut edits: Vec<Edit> = (list.child_tokens())
            .filter(|t| matches!(t.kind, K::LBracket | K::Comma | K::RBracket))
            .map(|t| Edit {
                span: at(t.start, t.end),
                replacement: String::new(),
            })
            .collect();
        // a list that opens on a line of its own, under the arguments before
        // it, has its elements one level further in: as lines under the call
        // they stand where the `[` stood
        let open = list.child_tokens().find(|t| t.kind == K::LBracket);
        let column = |at: u32| {
            let before = &self.text[..at as usize];
            before.len() - before.rfind('\n').map_or(0, |i| i + 1)
        };
        if let (true, Some(open), Some(first)) = (lines, open, elements.first())
            && starts_line(open.start)
            && starts_line(first.range().0)
            && column(first.range().0) > column(open.start)
        {
            let shift = column(first.range().0) - column(open.start);
            let (from, to) = (open.end as usize, list.range().1 as usize);
            let mut line = from + self.text[from..to].find('\n').map_or(to - from, |i| i + 1);
            while line < to {
                let rest = &self.text[line..to];
                let indent = rest.len() - rest.trim_start_matches(' ').len();
                if indent >= shift {
                    let start = u32::try_from(line).expect("source fits in u32");
                    let end = u32::try_from(line + shift).expect("source fits in u32");
                    edits.push(Edit {
                        span: at(start, end),
                        replacement: String::new(),
                    });
                }
                line += rest.find('\n').map_or(rest.len(), |i| i + 1);
            }
        }
        for element in &elements {
            let (start, end) = element.range();
            if atomic(element) || lines && starts_line(start) {
                continue;
            }
            edits.extend([(start, "("), (end, ")")].map(|(place, paren)| Edit {
                span: at(place, place),
                replacement: paren.to_owned(),
            }));
        }
        let name = self.text_of(head);
        let diagnostic = Diagnostic::error(
            codes::T073,
            format!(
                "the children of `{name}` stand one by one here, each an argument or a line of its own under the call, not in a list"
            ),
            self.span(list),
        );
        (self.diagnostics)
            .push(diagnostic.with_safe_fix(|| ("write the children one by one", edits)));
        Some(elements)
    }

    /// The type of `written`, the one argument where children go, and
    /// whether it is the list of them (`list`) rather than a child (`item`);
    /// `None` for an expression that is read by the type expected of it,
    /// which is a child.
    fn child_or_list(&mut self, written: &Node, list: &Type, item: &Type) -> Option<(Type, bool)> {
        let inner = unparen(written);
        if inner.kind == K::ListExpr {
            // a range or a comprehension: `listed` took the literal
            return Some((self.check_value(written, list), true));
        }
        // a function or a record is read by the type expected of it, and is
        // no list: a child, which the caller checks
        let by_expected = matches!(
            inner.kind,
            K::FunExpr | K::LoopExpr | K::WhileExpr | K::RecordExpr | K::DoExpr | K::TaskExpr
        );
        if by_expected {
            return None;
        }
        let actual = self.infer(inner);
        let listed = self.list_item(&actual).is_some();
        let expected = if listed { list } else { item };
        self.expect(&actual, expected, self.span(inner));
        self.note_head(inner, expected);
        Some((actual, listed))
    }

    /// What `function`, a function of the list of its children alone, gives
    /// for `children`, the arguments `head` has left from the `at`-th on; or
    /// `None` where no block around names the type of a child, or the
    /// function takes no list of it last. `end` is where the call ends.
    pub(super) fn gather(
        &mut self,
        head: &Node,
        function: &Type,
        children: &[&Node],
        (at, end): (usize, u32),
    ) -> Option<Type> {
        let parts = self.of_children(function)?;
        let (param, effects, result, item) = &parts;
        // the list of them as it was written before: its elements are the
        // children, which the fix writes one by one
        if let [one] = children
            && let Some(elements) = self.listed(head, one)
        {
            for element in &elements {
                self.check_value(element, item);
            }
            return Some(result.clone());
        }
        // one argument that is a list is the list of them: a value, a call
        // or a comprehension, as `List.map row items`
        let alone = match children {
            [one] => self.child_or_list(one, param, item),
            _ => None,
        };
        if let Some((argument, true)) = &alone {
            let span = Span::new(self.file, head.range().0, children[0].range().1);
            if let Some(effects) = effects {
                self.call_effects(&self.live_call(function, effects), span);
            }
            let result = self.instantiate(result, span);
            return Some(self.application_result(result, function, argument, false));
        }
        let start = head.range().0;
        // no child leaves the call no text of its own: its place is its end
        let span = match children.last() {
            Some(last) => Span::new(self.file, start, last.range().1),
            None => Span::new(self.file, end, end),
        };
        self.typing.gathered.insert((start, end), at);
        let first = alone.as_ref().map(|(actual, _)| actual);
        Some(self.applied_to_children(function, &parts, children, first, span))
    }

    /// The parameter, effects and result of `function` and the type of a
    /// child, when it is a function of the list of the children a block
    /// around names, alone.
    fn of_children(&mut self, function: &Type) -> Option<Children> {
        let child = (self.opened.iter().rev()).find_map(|o| o.child.clone())?;
        let Type::Fun {
            param,
            effects,
            result,
        } = self.whnf(function)
        else {
            return None;
        };
        let item = self.list_item(&param)?;
        let named = |t: &Type| match t.spine().0 {
            Type::Con { module, name } => Some((module.clone(), name.clone())),
            _ => None,
        };
        let of_child = named(&self.bare(&item)).is_some_and(|n| Some(n) == named(&child));
        if !of_child || matches!(self.whnf(&result), Type::Fun { .. }) {
            return None;
        }
        Some((*param, effects, *result, item))
    }

    /// `function`, of the parts `of_children` gives, applied at `span` to
    /// the list of `children`; `first` is the type of the first when the
    /// caller has checked it.
    fn applied_to_children(
        &mut self,
        function: &Type,
        (param, effects, result, item): &Children,
        children: &[&Node],
        first: Option<&Type>,
        span: Span,
    ) -> Type {
        let mut captures = Norm::default();
        for (i, written) in children.iter().enumerate() {
            let actual = match first {
                Some(actual) if i == 0 => actual.clone(),
                _ => self.check_value(written, item),
            };
            let kept = self.captures_of(&actual);
            captures.entries.extend(kept.entries);
            captures.tails.extend(kept.tails);
        }
        let list = Type::app(Type::lang(Lang::List), item.clone());
        let argument = self.data_value(&list, captures);
        self.expect(&argument, param, span);
        if let Some(effects) = effects {
            self.call_effects(&self.live_call(function, effects), span);
        }
        let result = self.instantiate(result, span);
        self.application_result(result, function, &argument, false)
    }
}

/// What [`Checker::of_children`] finds of a function of children.
type Children = (Type, Option<Box<Effects>>, Type, Type);

/// Whether a line of a block of this kind is a statement rather than an
/// expression.
fn is_statement(kind: K) -> bool {
    matches!(kind, K::Signature | K::LetDecl | K::AssignStmt)
}
