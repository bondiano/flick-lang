//! `cli "name" { … }`: the pipeline of `Cli.*` calls the checker typed it
//! as, under the same keys, so the block and the combinators written out
//! lower to the same IR.

use super::Lower;
use super::expr::with_dicts;
use crate::ir::{Expr, ExprKind, Lit};
use crate::source::Span;
use crate::syntax::cli::{self, Clause, Multi, Param};
use crate::syntax::cst::{Node, Token};
use crate::types::cli::{Arg, arg_key, group_key};
use crate::types::{Ref, Type};

/// An argument as the lowering makes it.
enum Made<'n> {
    Node(&'n Node),
    Expr(Expr),
}

impl Lower<'_> {
    pub(super) fn cli_block(&mut self, node: &Node) -> Expr {
        let block = cli::block(node, self.text);
        let range = |t: Token| (t.start, t.end);
        let name = block
            .name
            .map(|t| cli::string(t, self.text))
            .unwrap_or_default();
        let span = self.span(node);
        let name = Expr::new(ExprKind::Lit(Lit::String(name)), span);
        let mut spec = self.cli_call(range(block.keyword), vec![Made::Expr(name)]);
        let start = block.keyword.start;
        let mut handler = None;
        for clause in &block.clauses {
            let (call, end) = match clause {
                Clause::Description(w, e) => {
                    (self.cli_call(range(*w), vec![Made::Node(e)]), e.range().1)
                }
                Clause::Subcommand(w, e, help) => {
                    let help = self.cli_string(help, *w);
                    (
                        self.cli_call(range(*w), vec![Made::Node(e), Made::Expr(help)]),
                        e.range().1,
                    )
                }
                Clause::Param(p) => {
                    let end = p.default.map_or(p.ty.range().1, |d| d.range().1);
                    (self.cli_param(p), end)
                }
                Clause::Handle(..) => {
                    handler = Some(clause);
                    continue;
                }
            };
            spec = self.cli_pipe(spec, call, (start, end));
        }
        let last = match handler {
            Some(Clause::Handle(w, f)) => self.cli_call(range(*w), vec![Made::Node(f)]),
            _ => self.cli_call(group_key(block.keyword), Vec::new()),
        };
        self.cli_pipe(spec, last, (start, node.range().1))
    }

    fn cli_param(&mut self, p: &Param) -> Expr {
        let ty = p.ty.range();
        let multi = match p.default {
            Some(d) if p.multi == Multi::Default => self.cli_call(ty, vec![Made::Node(d)]),
            _ => self.cli_call(ty, Vec::new()),
        };
        let at = self.cli_span((p.name.start, p.name.end));
        let label = Expr::new(ExprKind::Label(p.name.text(self.text).to_owned()), at);
        let mut args = vec![Made::Expr(label)];
        if p.flag {
            let aliases = p
                .aliases
                .iter()
                .map(|t| {
                    Expr::new(
                        ExprKind::Lit(Lit::String(cli::string(*t, self.text))),
                        self.cli_span((t.start, t.end)),
                    )
                })
                .collect();
            args.push(Made::Expr(Expr::new(ExprKind::List(aliases), at)));
        }
        args.push(Made::Expr(self.cli_string(&p.help, p.word)));
        args.push(Made::Expr(multi));
        self.cli_call((p.word.start, p.word.end), args)
    }

    fn cli_string(&self, text: &str, at: Token) -> Expr {
        Expr::new(
            ExprKind::Lit(Lit::String(text.to_owned())),
            self.cli_span((at.start, at.end)),
        )
    }

    fn cli_span(&self, key: (u32, u32)) -> Span {
        Span::new(self.file, key.0, key.1)
    }

    /// The combinator the checker recorded at `head`, applied to `args`.
    fn cli_call(&mut self, head: (u32, u32), args: Vec<Made>) -> Expr {
        let span = self.cli_span(head);
        let r = self.typing.names.get(&head).cloned();
        let kind = match r {
            Some(Ref::Value { module, name }) => ExprKind::Global { module, name },
            _ => panic!("the checker typed the `cli` block"),
        };
        let dicts = self.typing.dicts.get(&head).cloned().unwrap_or_default();
        let mut func = with_dicts(Expr::new(kind, span), dicts);
        for (i, arg) in args.into_iter().enumerate() {
            let key = match &arg {
                Made::Node(n) => arg_key(head, i, &Arg::Node(n)),
                Made::Expr(_) => arg_key(head, i, &Arg::Made(Type::Error)),
            };
            let arg = match arg {
                Made::Node(n) => self.expr(n),
                Made::Expr(e) => e,
            };
            func = self.cli_app(func, arg, key, false);
        }
        func
    }

    /// `prev |> call`.
    fn cli_pipe(&mut self, prev: Expr, call: Expr, key: (u32, u32)) -> Expr {
        self.cli_app(call, prev, key, true)
    }

    fn cli_app(&self, func: Expr, arg: Expr, key: (u32, u32), arg_first: bool) -> Expr {
        let app = ExprKind::App {
            func: Box::new(func),
            arg: Box::new(arg),
            stage: self.call_stage(key),
            arg_first,
        };
        let dicts = self.typing.dicts.get(&key).cloned().unwrap_or_default();
        with_dicts(Expr::new(app, self.cli_span(key)), dicts)
    }
}

#[cfg(test)]
mod tests {
    use crate::ir::print;
    use crate::ir::tests::check_text;

    const HANDLER: &str = "fun { loud; times; name } ->\n      let text = \"Hello, \" ++ name\n      let line = if loud then String.to_upper text else text\n      Platform.stdout_line (String.join \"\\n\" (List.repeat times line))";

    fn ir(body: &str) -> String {
        let text = format!(
            "module App.Cli exposing (greet, tool)\n\ngreet : Cli.Command none none\nlet greet =\n{body}\n\nversion : Cli.Command none none\nlet version = Cli.command \"version\" |> Cli.handle (fun {{}} -> Platform.stdout_line \"0.1.0\")\n"
        );
        let (module, _) = check_text("App.Cli", &text).unwrap_or_else(|d| panic!("{d:?}"));
        print(&module)
    }

    #[test]
    fn the_block_and_the_combinators_lower_alike() {
        let sugar = format!(
            "  cli \"greet\" {{\n    description \"Prints a greeting\"\n    /// Print in capitals\n    flag loud : Bool\n    /// How many times\n    flag times \"n\" : Number = 1\n    /// Who to greet\n    arg name : String\n    handle {HANDLER}\n  }}\n\ntool : Cli.Command none none\nlet tool =\n  cli \"tool\" {{\n    description \"Project tools\"\n    /// Verbose output\n    flag verbose \"v\" : Bool\n    subcommand greet\n    subcommand version\n  }}"
        );
        let plain = format!(
            "  Cli.command \"greet\"\n    |> Cli.description \"Prints a greeting\"\n    |> Cli.flag (Record.label : Record.Label \"loud\") [] \"Print in capitals\" Cli.switch\n    |> Cli.flag (Record.label : Record.Label \"times\") [\"n\"] \"How many times\" (Cli.default 1)\n    |> Cli.arg (Record.label : Record.Label \"name\") \"Who to greet\" (Cli.required : Cli.Multi String String)\n    |> Cli.handle ({HANDLER})\n\ntool : Cli.Command none none\nlet tool =\n  Cli.command \"tool\"\n    |> Cli.description \"Project tools\"\n    |> Cli.flag (Record.label : Record.Label \"verbose\") [\"v\"] \"Verbose output\" Cli.switch\n    |> Cli.subcommand greet \"\"\n    |> Cli.subcommand version \"\"\n    |> Cli.group"
        );
        assert_eq!(ir(&sugar), ir(&plain));
    }
}
