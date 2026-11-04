//! The code a block stands for: `Cli.command name`, each clause applied to
//! the spec before it, and `Cli.handled spec f` or `Cli.group` last. Each
//! call stands for the word of its clause, where errors of it land; the last
//! stands for the whole block.

use flick_plugin::{Arena, Block, Builder, Name, NodeId, Wildcard, flick};

use crate::read::{Clause, Command, Multi, Param};

pub fn arena(block: &Block, command: &Command) -> Arena {
    let whole = block.whole();
    let mut b = Builder::new(whole);
    let mut spec = flick!(b @ command.name, Cli.command #(command.name));
    let mut handler = None;
    for clause in &command.clauses {
        spec = match clause {
            Clause::Description { word, value } => {
                flick!(b @ word, Cli.description #value #spec)
            }
            Clause::Subcommand { word, value, help } => {
                flick!(b @ word, Cli.subcommand #value #help #spec)
            }
            Clause::Param(p) => param(&mut b, block, p, spec),
            Clause::Handle { value, .. } => {
                handler = Some(*value);
                continue;
            }
        };
    }
    let root = match handler {
        None => flick!(b @ whole, Cli.group #spec),
        // the spec first, of exactly the options its clauses name: the
        // handler is checked against them
        Some(f) => {
            let fields: Vec<_> = (command.params())
                .map(|p| (block.text_of(p.name), Wildcard))
                .collect();
            if fields.is_empty() {
                // `Spec none none {}` already
                flick!(b @ whole, Cli.handled #spec #f)
            } else {
                flick!(b @ whole, Cli.handled (#spec : Cli.Spec _ _ { #(..fields) }) #f)
            }
        }
    };
    b.finish(root)
}

/// `Cli.flag label aliases help multi spec` or `Cli.arg label help multi
/// spec`: the label names the field, and the multiplicity is of the type
/// written, `Cli.default (d : T)` or `(Cli.many : Cli.Multi _ T)`.
fn param(b: &mut Builder, block: &Block, p: &Param, spec: NodeId) -> NodeId {
    b.at(p.word);
    let field = block.text_of(p.name);
    let multi = match p.default {
        Some(d) if p.multi == Multi::Default => flick!(b, Cli.default (#d : #(p.ty))),
        _ => flick!(b, (#(Name(p.multi.function())) : Cli.Multi _ #(p.ty))),
    };
    let label = flick!(b, (Record.label : Record.Label #field));
    if p.flag {
        flick!(b, Cli.flag #label [#(..&p.aliases)] #(p.help) #multi #spec)
    } else {
        flick!(b, Cli.arg #label #(p.help) #multi #spec)
    }
}
