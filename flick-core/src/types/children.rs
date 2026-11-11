//! The children of a call: in a block that opens a module and names the type
//! of a child, a function whose last parameter is a list of that type takes
//! the list element by element. Each argument from that parameter on is an
//! element, on the line of the call or on a line of its own under it, and
//! none is the empty list. The call then stands for the function applied to
//! the list of them, which is what lowering writes.

use super::Checker;
use super::lang::Lang;
use super::ty::{Norm, Type};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

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
        let body = node.nodes().find(|n| n.kind != K::NameType);
        self.opened.push((module.join("."), child));
        let ty = body.map_or(Type::Error, |body| self.infer(body));
        self.opened.pop();
        ty
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
        let child = (self.opened.iter().rev()).find_map(|(_, child)| child.clone())?;
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
        let mut captures = Norm::default();
        for written in children {
            let actual = self.check_value(written, &item);
            let held = self.captures_of(&actual);
            captures.entries.extend(held.entries);
            captures.tails.extend(held.tails);
        }
        let start = head.range().0;
        // no child leaves the call no text of its own: its place is its end
        let span = match children.last() {
            Some(last) => Span::new(self.file, start, last.range().1),
            None => Span::new(self.file, end, end),
        };
        let list = Type::app(Type::lang(Lang::List), item);
        let argument = self.data_value(&list, captures);
        self.expect(&argument, &param, span);
        if let Some(effects) = effects {
            self.call_effects(&self.live_call(function, &effects), span);
        }
        self.typing.gathered.insert((start, end), at);
        let result = self.instantiate(&result, span);
        Some(self.application_result(result, function, &argument, false))
    }
}
