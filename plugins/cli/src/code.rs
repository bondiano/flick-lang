//! The code a block stands for: `Cli.command name`, each clause applied to
//! the spec before it, and `Cli.handled spec f` or `Cli.group` last. Each
//! call stands for the word of its clause, where errors of it land; the last
//! stands for the whole block.

use flick_plugin::{Arena, Block, Builder, Expr, Field, NodeId, Span, Ty};

use crate::read::{Clause, Command, Multi, Param};
use crate::{len, text};

pub fn arena(block: &Block, command: &Command) -> Arena {
    let whole = Span {
        start: 0,
        end: len(block),
    };
    let mut b = Builder::new(whole);
    b.at(command.name);
    let name = b.expr(Expr::Hole(command.name));
    let mut spec = call(&mut b, "Cli.command", vec![name]);
    let mut handler = None;
    for clause in &command.clauses {
        b.at(clause.word());
        let (function, mut arguments) = match clause {
            Clause::Description { value, .. } => {
                ("Cli.description", vec![b.expr(Expr::Hole(*value))])
            }
            Clause::Subcommand { value, help, .. } => {
                let value = b.expr(Expr::Hole(*value));
                (
                    "Cli.subcommand",
                    vec![value, b.expr(Expr::Text(help.clone()))],
                )
            }
            Clause::Param(p) => param(&mut b, block, p),
            Clause::Handle { value, .. } => {
                handler = Some(*value);
                continue;
            }
        };
        arguments.push(spec);
        spec = call(&mut b, function, arguments);
    }
    // the last call stands for the whole block
    b.at(whole);
    let Some(value) = handler else {
        let root = call(&mut b, "Cli.group", vec![spec]);
        return b.finish(root);
    };
    // the spec first, of the options its clauses name: the handler is
    // checked against them; without parameters it is `Spec none none {}`
    if let Some(options) = options(&mut b, block, command) {
        spec = b.expr(Expr::Annot {
            expr: spec,
            ty: options,
        });
    }
    let f = b.expr(Expr::Hole(value));
    let root = call(&mut b, "Cli.handled", vec![spec, f]);
    b.finish(root)
}

/// `Cli.Spec _ _ { field : _; … }`: a spec of exactly the fields of the
/// block's parameters, their types from the clauses; none without any.
fn options(b: &mut Builder, block: &Block, command: &Command) -> Option<NodeId> {
    let fields: Vec<Field> = (command.clauses.iter())
        .filter_map(|c| match c {
            Clause::Param(p) => Some(p),
            _ => None,
        })
        .map(|p| Field {
            label: text(block, p.name).to_owned(),
            value: b.ty(Ty::Wildcard),
        })
        .collect();
    if fields.is_empty() {
        return None;
    }
    let record = b.ty(Ty::Record(fields));
    let specs = b.ty(Ty::Name("Cli.Spec".into()));
    let (r, e) = (b.ty(Ty::Wildcard), b.ty(Ty::Wildcard));
    Some(b.ty(Ty::Apply {
        function: specs,
        arguments: vec![r, e, record],
    }))
}

fn call(b: &mut Builder, function: &str, arguments: Vec<NodeId>) -> NodeId {
    let function = b.expr(Expr::Name(function.into()));
    b.expr(Expr::Apply {
        function,
        arguments,
    })
}

/// `Cli.flag label aliases help multi` or `Cli.arg label help multi`, the
/// function and its arguments but the spec: the label names the field, and
/// the multiplicity is of the written type.
fn param(b: &mut Builder, block: &Block, p: &Param) -> (&'static str, Vec<NodeId>) {
    let label = b.expr(Expr::Name("Record.label".into()));
    let labels = b.ty(Ty::Name("Record.Label".into()));
    let field = b.ty(Ty::Text(text(block, p.name).to_owned()));
    let labelled = b.ty(Ty::Apply {
        function: labels,
        arguments: vec![field],
    });
    let mut arguments = vec![b.expr(Expr::Annot {
        expr: label,
        ty: labelled,
    })];
    if p.flag {
        let aliases = p.aliases.iter().map(|&a| b.expr(Expr::Hole(a))).collect();
        arguments.push(b.expr(Expr::Items(aliases)));
    }
    arguments.push(b.expr(Expr::Text(p.help.clone())));
    let written = b.ty(Ty::Hole(p.ty));
    // the field is of the type written: `Cli.default (d : T)`, else
    // `(Cli.many : Cli.Multi _ T)`
    let multi = match p.default {
        Some(d) if p.multi == Multi::Default => {
            let d = b.expr(Expr::Hole(d));
            let typed = b.expr(Expr::Annot {
                expr: d,
                ty: written,
            });
            call(b, p.multi.function(), vec![typed])
        }
        _ => {
            let multi = b.expr(Expr::Name(p.multi.function().into()));
            let multis = b.ty(Ty::Name("Cli.Multi".into()));
            let values = b.ty(Ty::Wildcard);
            let ty = b.ty(Ty::Apply {
                function: multis,
                arguments: vec![values, written],
            });
            b.expr(Expr::Annot { expr: multi, ty })
        }
    };
    arguments.push(multi);
    (if p.flag { "Cli.flag" } else { "Cli.arg" }, arguments)
}
