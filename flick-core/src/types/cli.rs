//! `cli "name" { … }`: the block is checked as the pipeline of `Cli.*` calls
//! it stands for (`Cli.command "name" |> Cli.flag … |> Cli.handle f`), so the
//! options record, the rows and the constraints come from the combinators'
//! signatures. What the types cannot state is checked first: names, aliases,
//! defaults and the shape of the command.

use std::collections::BTreeMap;

use super::Checker;
use super::ty::{Row, Type};
use crate::diagnostics::codes;
use crate::iface::{Entity, Namespace};
use crate::source::Span;
use crate::syntax::cli::{self, Clause, Multi, Param};
use crate::syntax::cst::{Node, Token};

/// The module of the combinators.
pub const CLI: &str = "Cli";

/// Long and short names Effect's CLI reserves.
const RESERVED: [&str; 4] = ["help", "h", "version", "completions"];

/// An argument of a combinator: a node of the block or a value the
/// elaboration makes, of the given type.
pub enum Arg<'n> {
    Node(&'n Node),
    Made(Type),
}

/// The keys under which the elaboration records the calls it makes: the
/// lowering looks them up the same way. A key of a made argument starts at
/// its call's head, where no expression of the block starts.
pub fn made_key(head: (u32, u32), index: usize) -> (u32, u32) {
    (head.0, head.1 + 1 + u32::try_from(index).unwrap_or(0))
}

/// The key of the call ending in argument `index` of a call headed at `head`.
pub fn arg_key(head: (u32, u32), index: usize, arg: &Arg) -> (u32, u32) {
    match arg {
        Arg::Node(n) => (head.0, n.range().1),
        Arg::Made(_) => made_key(head, index),
    }
}

/// The head of `Cli.group` of a block without `handle`.
pub fn group_key(keyword: Token) -> (u32, u32) {
    (keyword.start, keyword.start + 1)
}

impl Checker<'_> {
    pub fn cli_expr(&mut self, node: &Node) -> Type {
        let block = cli::block(node, self.text);
        self.cli_shape(node, &block.clauses);
        let span = |t: Token| (t.start, t.end);
        let name_ty = Type::prim("String");
        let mut spec = self.cli_call("command", span(block.keyword), &[Arg::Made(name_ty)]);
        let start = block.keyword.start;
        let mut handler = None;
        let mut fields: Vec<&str> = Vec::new();
        let mut record: Vec<(String, Type)> = Vec::new();
        for clause in &block.clauses {
            // a repeated field is reported once, without what would follow
            if let Clause::Param(p) = clause {
                let field = p.name.text(self.text);
                if fields.contains(&field) {
                    continue;
                }
                fields.push(field);
            }
            let end = match clause {
                Clause::Description(_, e) | Clause::Subcommand(_, e, _) | Clause::Handle(_, e) => {
                    e.range().1
                }
                Clause::Param(p) => p.default.map_or(p.ty.range().1, |d| d.range().1),
            };
            let call = match clause {
                Clause::Description(w, e) => {
                    self.cli_call("description", span(*w), &[Arg::Node(e)])
                }
                Clause::Subcommand(w, e, _) => self.cli_call(
                    "subcommand",
                    span(*w),
                    &[Arg::Node(e), Arg::Made(Type::prim("String"))],
                ),
                Clause::Param(p) => {
                    let (call, field) = self.cli_param(p);
                    record.push((p.name.text(self.text).to_owned(), field));
                    call
                }
                Clause::Handle(_, _) => {
                    handler = Some(clause);
                    continue;
                }
            };
            spec = self.cli_pipe(&spec, &call, (start, end));
        }
        let end = node.range().1;
        record.sort_by(|a, b| a.0.cmp(&b.0));
        let options = Some(Type::Record(Row {
            entries: record,
            tail: None,
        }));
        let last = match handler {
            Some(Clause::Handle(w, f)) => {
                // the handler is checked against the options the clauses made
                let mut known = |this: &mut Self, _: usize, param: &Type| {
                    if let (Type::Fun { param, .. }, Some(o)) = (this.whnf(param), &options) {
                        let _ = this.unify(&param, o);
                    }
                };
                self.cli_call_with("handle", span(*w), &[Arg::Node(f)], &mut known)
            }
            _ => self.cli_call("group", group_key(block.keyword), &[]),
        };
        self.cli_pipe(&spec, &last, (start, end))
    }

    /// `prev |> call`, recorded at `key`.
    fn cli_pipe(&mut self, prev: &Type, call: &Type, key: (u32, u32)) -> Type {
        let span = Span::new(self.file, key.0, key.1);
        match self.whnf(call) {
            Type::Fun { param, result, .. } => {
                self.expect(prev, &param, span);
                self.instantiate(&result, span)
            }
            _ => Type::Error,
        }
    }

    /// `Cli.<name> args…` with the head at `head`.
    pub(super) fn cli_call(&mut self, name: &str, head: (u32, u32), args: &[Arg]) -> Type {
        self.cli_call_with(name, head, args, &mut |_, _, _| {})
    }

    /// [`Self::cli_call`], with `before` seeing each parameter type before
    /// its argument is checked, so what the clause states is known first.
    fn cli_call_with(
        &mut self,
        name: &str,
        head: (u32, u32),
        args: &[Arg],
        before: &mut dyn FnMut(&mut Self, usize, &Type),
    ) -> Type {
        let entity = Entity {
            module: CLI.to_owned(),
            namespace: Namespace::Value,
            name: name.to_owned(),
        };
        let span = Span::new(self.file, head.0, head.1);
        let Some(scheme) = self.env.value(&entity).cloned() else {
            return Type::Error;
        };
        let mut ty = self.instantiate(&scheme, span);
        self.record_entity(&entity, &ty, span);
        for (i, arg) in args.iter().enumerate() {
            let key = arg_key(head, i, arg);
            let at = Span::new(self.file, key.0, key.1);
            let Type::Fun { param, result, .. } = self.whnf(&ty) else {
                return Type::Error;
            };
            before(self, i, &param);
            match arg {
                Arg::Node(n) => self.check(n, &param),
                Arg::Made(t) => {
                    self.expect(t, &param, span);
                }
            }
            ty = self.instantiate(&result, at);
        }
        ty
    }

    /// `Cli.flag label aliases help multi` or `Cli.arg label help multi`,
    /// its field of the written type.
    fn cli_param(&mut self, p: &Param) -> (Type, Type) {
        let declared = self.annotation(p.ty);
        let value = self.fresh();
        // the values are of the element type of `Option`, `List` and `NonEmpty`
        let element = match (p.multi, self.whnf(&declared)) {
            (Multi::Optional | Multi::Many | Multi::Some, Type::App { arg, .. }) => *arg,
            _ => declared.clone(),
        };
        let _ = self.unify(&value, &element);
        let field = declared.clone();
        let element = value.clone();
        let ty_range = p.ty.range();
        let multi = match p.default {
            Some(d) if p.multi == Multi::Default => {
                let mut written = |this: &mut Self, _: usize, param: &Type| {
                    let _ = this.unify(param, &element);
                };
                self.cli_call_with("default", ty_range, &[Arg::Node(d)], &mut written)
            }
            _ => self.cli_call(p.multi.function(), ty_range, &[]),
        };
        let expected = Type::apps(Type::con(CLI, "Multi"), [value, field]);
        self.expect(&multi, &expected, self.span(p.ty));
        let name = p.name.text(self.text).to_owned();
        let label = Type::app(
            Type::con(super::classes::RECORD, "Label"),
            Type::Symbol(name),
        );
        let strings = Type::app(Type::prim("List"), Type::prim("String"));
        let mut args = vec![Arg::Made(label)];
        if p.flag {
            args.push(Arg::Made(strings));
        }
        args.push(Arg::Made(Type::prim("String")));
        args.push(Arg::Made(multi));
        let function = if p.flag { "flag" } else { "arg" };
        (
            self.cli_call(function, (p.word.start, p.word.end), &args),
            declared,
        )
    }

    /// The rules of the block the signatures cannot state.
    fn cli_shape(&mut self, node: &Node, clauses: &[Clause]) {
        let mut names: BTreeMap<String, Span> = BTreeMap::new();
        let mut taken = |this: &mut Self, name: String, span: Span, what: &str| {
            if RESERVED.contains(&name.as_str()) {
                this.error(
                    codes::L001,
                    span,
                    format!("`{}` is reserved by the CLI", dashed(&name)),
                );
            } else if names.insert(name.clone(), span).is_some() {
                this.error(
                    codes::L001,
                    span,
                    format!(
                        "{what} `{}` is already taken in this command",
                        dashed(&name)
                    ),
                );
            }
        };
        let mut fields: Vec<String> = Vec::new();
        let mut seen: BTreeMap<&str, Span> = BTreeMap::new();
        let params: Vec<&Param> = clauses
            .iter()
            .filter_map(|c| match c {
                Clause::Param(p) => Some(p),
                _ => None,
            })
            .collect();
        for clause in clauses {
            let word = clause.word();
            let span = self.token_span(word);
            if let Clause::Description(..) | Clause::Handle(..) = clause {
                let w = if matches!(clause, Clause::Handle(..)) {
                    "handle"
                } else {
                    "description"
                };
                if seen.insert(w, span).is_some() {
                    self.error(codes::L004, span, format!("`{w}` is given twice"));
                }
            }
            let Clause::Param(p) = clause else { continue };
            let field = p.name.text(self.text).to_owned();
            if fields.contains(&field) {
                self.error(
                    codes::L001,
                    self.token_span(p.name),
                    format!("the options already have a field `{field}`"),
                );
                continue;
            }
            fields.push(field.clone());
            if p.flag {
                taken(
                    self,
                    cli::long_name(&field),
                    self.token_span(p.name),
                    "the name",
                );
                for alias in &p.aliases {
                    let text = cli::string(*alias, self.text);
                    taken(self, text, self.token_span(*alias), "the alias");
                }
            } else if let Some(alias) = p.aliases.first() {
                self.error(
                    codes::L007,
                    self.token_span(*alias),
                    "a positional `arg` has no other names",
                );
            }
            if let (Multi::Optional, Some(d)) = (p.multi, p.default) {
                self.error(
                    codes::L002,
                    self.span(d),
                    "an `Option` parameter is `None` when absent: it takes no `= default`",
                );
            }
        }
        self.cli_command_shape(node, clauses, &params, seen.contains_key("handle"));
    }

    /// Positional arguments, subcommands and the handler of the command.
    fn cli_command_shape(
        &mut self,
        node: &Node,
        clauses: &[Clause],
        params: &[&Param],
        handled: bool,
    ) {
        let args: Vec<&&Param> = params.iter().filter(|p| !p.flag).collect();
        for p in args.iter().rev().skip(1) {
            if matches!(p.multi, Multi::Many | Multi::Some) {
                self.error(
                    codes::L003,
                    self.span(p.ty),
                    "an `arg` of many values takes the rest of the line: only the last one can",
                );
            }
        }
        let subcommand = clauses.iter().find(|c| matches!(c, Clause::Subcommand(..)));
        if let (Some(s), Some(a)) = (subcommand, args.first()) {
            self.error(
                codes::L006,
                self.token_span(a.word),
                "a command with subcommands takes no `arg`: it could not be told from a subcommand's name",
            );
            let _ = s;
        }
        if subcommand.is_none() && !handled {
            self.error(
                codes::L005,
                self.span(node),
                "a command needs `handle` or at least one `subcommand`",
            );
        }
    }
}

fn dashed(name: &str) -> String {
    if name.chars().count() == 1 {
        format!("-{name}")
    } else {
        format!("--{name}")
    }
}
