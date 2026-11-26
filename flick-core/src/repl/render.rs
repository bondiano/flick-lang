//! The text of a phrase module and where the input stands in it, and the
//! imports that give a phrase the newest definition of every earlier name.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use crate::iface::{Export, Namespace};
use crate::resolve::{Import, ItemKind};
use crate::source::{FileId, Span};
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::lexer::lex;

/// An `exposing` list names values one way and types, constructors and
/// classes another; a name of each kind is shadowed on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Space {
    Value,
    Con,
}

impl Space {
    pub(super) fn of(namespace: Namespace) -> Self {
        match namespace {
            Namespace::Value => Space::Value,
            // shortcut: a type and a constructor of one name are shadowed
            // together, as `exposing (T)` names both; separate them if a
            // session ever needs the older one of the two unqualified
            Namespace::Type | Namespace::Constructor => Space::Con,
        }
    }
}

pub(super) type Name = (Space, String);

/// Where each name of the session comes from: the newest phrase that
/// defined it, or the module a phrase imported it from.
#[derive(Debug, Clone, Default)]
pub(super) struct Scope {
    /// The phrase that brought each name in and the module it is imported from.
    pub(super) names: BTreeMap<Name, (usize, String)>,
    /// Each module a phrase imported, with the newest alias given to it.
    pub(super) modules: BTreeMap<String, Option<String>>,
}

impl Scope {
    /// The imports of a new phrase: each earlier phrase, exposing the names
    /// it still owns that the phrase does not define itself, then each
    /// module an earlier phrase imported, unless the phrase imports it again.
    /// Every name is exposed from one module only, so none is ambiguous.
    pub(super) fn imports(
        &self,
        phrases: &[String],
        own: &BTreeSet<Name>,
        again: &[String],
    ) -> String {
        let mut live: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for (name, (_, module)) in &self.names {
            if !own.contains(name) {
                live.entry(module).or_default().push(&name.1);
            }
        }
        let mut out = String::new();
        // a phrase without live names is still imported, for its instances
        for module in phrases {
            line(&mut out, module, None, live.get(module.as_str()));
        }
        for (module, alias) in &self.modules {
            if !again.contains(module) {
                line(
                    &mut out,
                    module,
                    alias.as_deref(),
                    live.get(module.as_str()),
                );
            }
        }
        out
    }
}

fn line(out: &mut String, module: &str, alias: Option<&str>, names: Option<&Vec<&str>>) {
    let _ = write!(out, "import {module}");
    if let Some(alias) = alias {
        let _ = write!(out, " as {alias}");
    }
    if let Some(names) = names {
        let _ = write!(out, " exposing ({})", names.join(", "));
    }
    out.push('\n');
}

/// The name of `export` in the session.
pub(super) fn name_of(export: &Export) -> Name {
    (
        Space::of(export.entity.namespace),
        export.entity.name.clone(),
    )
}

/// The names `import` brings in, each with its export when `exports`, the
/// interface of the imported module, has it.
pub(super) fn imported<'e>(
    import: &Import,
    exports: &'e [Export],
) -> Vec<(Name, Option<&'e Export>)> {
    let found = |name: &str, space: Space| {
        let export = (exports.iter())
            .find(|e| e.entity.name == name && Space::of(e.entity.namespace) == space);
        ((space, name.to_owned()), export)
    };
    let mut brought = Vec::new();
    for item in &import.exposing {
        let members: Vec<&Export> = match item.kind {
            ItemKind::Value => {
                brought.push(found(&item.name, Space::Value));
                continue;
            }
            ItemKind::Con => {
                brought.push(found(&item.name, Space::Con));
                continue;
            }
            ItemKind::ConAll => {
                brought.push(found(&item.name, Space::Con));
                (exports.iter())
                    .filter(|e| e.parent.as_deref() == Some(&item.name))
                    .collect()
            }
            ItemKind::All => exports.iter().collect(),
        };
        brought.extend(members.into_iter().map(|e| (name_of(e), Some(e))));
    }
    brought
}

/// Whether `input` starts as a declaration does, so that it is checked as
/// one even when it does not parse.
pub(super) fn starts_declaration(input: &str, file: FileId) -> bool {
    let (tokens, _) = lex(input, file);
    let mut significant = tokens.iter().filter(|t| !t.kind.is_trivia());
    let Some(first) = significant.next() else {
        return false;
    };
    match first.kind {
        K::ModuleKw
        | K::ImportKw
        | K::RecKw
        | K::KindKw
        | K::TypeKw
        | K::OpaqueKw
        | K::ServiceKw
        | K::ClassKw
        | K::InstanceKw
        | K::DerivingKw
        | K::ForeignKw
        | K::At
        | K::LetKw => true,
        K::Ident => {
            let next_kind = significant.next().map(|t| t.kind);
            if next_kind == Some(K::Colon) {
                return true;
            }
            let text = &input[first.start as usize..first.end as usize];
            text == "effect" && next_kind == Some(K::Con)
        }
        _ => false,
    }
}

/// A phrase module's text, with the runs of it copied from the input.
#[derive(Debug, Clone, Default)]
pub(super) struct Layout {
    pub(super) text: String,
    /// Each run as (module offset, input offset, length).
    runs: Vec<(u32, u32, u32)>,
}

fn offset(n: usize) -> u32 {
    u32::try_from(n).expect("phrase offsets fit in u32")
}

impl Layout {
    pub(super) fn generated(&mut self, text: &str) {
        self.text.push_str(text);
    }

    /// `text`, copied from the input at `from`.
    pub(super) fn input(&mut self, text: &str, from: u32) {
        self.runs
            .push((offset(self.text.len()), from, offset(text.len())));
        self.text.push_str(text);
    }

    /// The declarations of `input` as they were written.
    pub(super) fn declarations(head: &str, input: &str) -> Self {
        let mut layout = Layout::default();
        layout.generated(head);
        layout.input(input, 0);
        layout.generated("\n");
        layout
    }

    /// The expression `input` as the value `it`, its lines a block under the
    /// `let`. The block first binds `it` to `previous`, the module of the
    /// earlier `it`, so `it * 2` reads that one and not itself.
    pub(super) fn expression(head: &str, input: &str, previous: Option<&str>) -> Self {
        Self::named_expression(head, "it", input, previous)
    }

    /// The expression `input` bound to `name`, optionally exposing `previous.it`.
    pub(super) fn named_expression(
        head: &str,
        name: &str,
        input: &str,
        previous: Option<&str>,
    ) -> Self {
        let mut layout = Layout::default();
        layout.generated(head);
        layout.generated(&format!("let {name} =\n"));
        if let Some(previous) = previous {
            layout.generated(&format!("  let it = {previous}.it\n"));
        }
        let mut from = 0;
        for line in input.split_inclusive('\n') {
            let body = line.trim_end_matches(['\n', '\r']);
            layout.generated("  ");
            layout.input(body, from);
            layout.generated("\n");
            from += offset(line.len());
        }
        layout
    }

    /// An effectful expression `input` wrapped in `task { … }` as `it`, so
    /// top-level runtime calls typecheck and can be auto-awaited by the evaluator.
    pub(super) fn task_expression(head: &str, input: &str, previous: Option<&str>) -> Self {
        let mut layout = Layout::default();
        layout.generated(head);
        layout.generated("let it =\n");
        if let Some(previous) = previous {
            layout.generated(&format!("  let it = {previous}.it\n"));
        }
        layout.generated("  task {\n");
        let mut from = 0;
        for line in input.split_inclusive('\n') {
            let body = line.trim_end_matches(['\n', '\r']);
            layout.generated("    ");
            layout.input(body, from);
            layout.generated("\n");
            from += offset(line.len());
        }
        layout.generated("  }\n");
        layout
    }

    /// Inline handler arms `| E.op ... -> ...` wrapped as a thunk handler function.
    pub(super) fn handle_arms(head: &str, name: &str, input: &str) -> Self {
        let mut layout = Layout::default();
        layout.generated(head);
        layout.generated(&format!(
            "let {name} __repl_body =\n  try __repl_body () with\n"
        ));
        let mut from = 0;
        for line in input.split_inclusive('\n') {
            let body = line.trim_end_matches(['\n', '\r']);
            layout.generated("  ");
            layout.input(body, from);
            layout.generated("\n");
            from += offset(line.len());
        }
        layout
    }

    /// The input offset of the module offset `pos`; `end` also takes the
    /// offset just past a run.
    fn to_input(&self, pos: u32, end: bool) -> Option<u32> {
        (self.runs.iter())
            .find(|&&(at, _, len)| pos >= at && (pos < at + len || (end && pos == at + len)))
            .map(|&(at, from, _)| from + pos - at)
    }

    /// `span` of the module placed in `input`; `None` when it starts in a
    /// generated part. An end past the run of the start is cut at its end.
    pub(super) fn place(&self, span: Span, input: FileId) -> Option<Span> {
        let &(at, from, len) = (self.runs.iter()).find(|&&(at, _, len)| {
            span.start >= at
                && (span.start < at + len || (span.start == span.end && span.start == at + len))
        })?;
        let start = from + span.start - at;
        let end = (self.to_input(span.end, true))
            .filter(|&end| end >= start)
            .unwrap_or(from + len);
        Some(Span::new(input, start, end))
    }

    /// The layout with each generated `(offset, text)` inserted; an insertion
    /// inside a run splits it.
    pub(super) fn inserted(&self, inserts: &[(u32, String)]) -> Self {
        let mut inserts: Vec<&(u32, String)> = inserts.iter().collect();
        inserts.sort_by_key(|(at, _)| *at);
        let mut text = String::new();
        let mut copied = 0;
        for (at, piece) in &inserts {
            text.push_str(&self.text[copied..*at as usize]);
            text.push_str(piece);
            copied = *at as usize;
        }
        text.push_str(&self.text[copied..]);
        // a position moves by what was inserted at or before it
        let shift = |pos: u32| -> u32 {
            (inserts.iter())
                .filter(|(at, _)| *at <= pos)
                .map(|(_, piece)| offset(piece.len()))
                .sum()
        };
        let mut runs = Vec::new();
        for &(at, from, len) in &self.runs {
            let mut start = at;
            for (cut, _) in inserts
                .iter()
                .filter(|(cut, _)| *cut > at && *cut < at + len)
            {
                runs.push((start + shift(start), from + start - at, cut - start));
                start = *cut;
            }
            runs.push((start + shift(start), from + start - at, at + len - start));
        }
        Layout { text, runs }
    }
}
