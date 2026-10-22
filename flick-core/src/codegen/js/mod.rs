//! ESM for one module. Expressions become JS expressions; what needs
//! statements (`match`, blocks, branches with statements) is emitted into the
//! current statement list, earlier operands spilled into constants first so
//! that evaluation stays left to right.

mod expr;
mod function;
mod items;
mod pattern;

use std::collections::BTreeSet;
use std::fmt::Write;
use std::mem;

use super::{Options, Output, names};
use crate::ir::Module;
use crate::source::Span;
use crate::types::Dict;

const RUNTIME: &str = "@flick-lang/runtime";
const MARK_START: char = '\u{E000}';
const MARK_END: char = '\u{E001}';

/// Compiles `module` to ESM (or TypeScript, see [`Options::typescript`]).
pub fn js(module: &Module, options: &Options) -> Output {
    let mut g = Gen {
        module: &module.name,
        options,
        imports: BTreeSet::new(),
        foreign: Vec::new(),
        runtime: false,
        effect: false,
        temps: 0,
        givens: Vec::new(),
        spans: Vec::new(),
        buf: Vec::new(),
        generator: false,
        tail: None,
    };
    let body = g.items(&module.items);
    let mut code = String::new();
    if g.effect {
        let _ = writeln!(code, "import {{ Effect }} from \"{RUNTIME}\";");
    }
    if g.runtime {
        let _ = writeln!(code, "import * as $rt from \"{RUNTIME}\";");
    }
    for (source, export, alias) in &g.foreign {
        let export = if names::is_property(export) {
            export.clone()
        } else {
            names::string(export)
        };
        let _ = writeln!(
            code,
            "import {{ {export} as {alias} }} from {};",
            names::string(source)
        );
    }
    let here = (options.path)(&module.name);
    for imported in &g.imports {
        let target = (options.path)(imported);
        let _ = writeln!(
            code,
            "import * as {} from {};",
            names::module_alias(imported),
            names::string(&relative(&here, &target))
        );
    }
    if !code.is_empty() {
        code.push('\n');
    }
    code.push_str(&body);
    finish(&code, &g.spans)
}

/// The import specifier of `to` from `from`, both relative to the output
/// root; `npm:` marks a module of a package.
fn relative(from: &str, to: &str) -> String {
    if let Some(bare) = to.strip_prefix("npm:") {
        return bare.to_owned();
    }
    let from_dir: Vec<&str> = from.split('/').collect();
    let from_dir = &from_dir[..from_dir.len() - 1];
    let to_parts: Vec<&str> = to.split('/').collect();
    let common = from_dir
        .iter()
        .zip(&to_parts)
        .take_while(|(a, b)| a == b)
        .count();
    let up = from_dir.len() - common;
    let rest = to_parts[common..].join("/");
    if up == 0 {
        format!("./{rest}")
    } else {
        format!("{}{rest}", "../".repeat(up))
    }
}

/// Removes the markers from `code`, recording where each one was.
fn finish(code: &str, spans: &[Span]) -> Output {
    let mut out = String::with_capacity(code.len());
    let mut mappings = Vec::new();
    let (mut line, mut col) = (0u32, 0u32);
    let mut chars = code.chars();
    while let Some(c) = chars.next() {
        match c {
            MARK_START => {
                let id: String = chars.by_ref().take_while(|&c| c != MARK_END).collect();
                if let Ok(id) = id.parse::<usize>() {
                    mappings.push((line, col, spans[id]));
                }
            }
            '\n' => {
                out.push(c);
                line += 1;
                col = 0;
            }
            c => {
                out.push(c);
                col += u32::try_from(c.len_utf16()).unwrap_or(1);
            }
        }
    }
    Output {
        code: out,
        mappings,
    }
}

/// Where a self tail call jumps: the function, its parameter count and the
/// loop variables.
struct Tail {
    target: Target,
    vars: Vec<String>,
}

#[derive(Clone, PartialEq, Eq)]
enum Target {
    Local(String),
    Global(String),
}

struct Gen<'a> {
    module: &'a str,
    options: &'a Options<'a>,
    /// Modules whose exports are used.
    imports: BTreeSet<String>,
    /// Foreign imports: specifier, export, alias.
    foreign: Vec<(String, String, String)>,
    runtime: bool,
    effect: bool,
    temps: u32,
    /// Dictionaries in scope: as `Dict::Given` names them, and the parameter.
    givens: Vec<(String, String)>,
    spans: Vec<Span>,
    /// Statements of the block being emitted.
    buf: Vec<String>,
    /// Inside the generator of a runtime stage, where `yield*` runs a call.
    generator: bool,
    tail: Option<Tail>,
}

impl Gen<'_> {
    fn temp(&mut self) -> String {
        self.temps += 1;
        format!("$t{}", self.temps)
    }

    /// An arrow's parameter list: in TypeScript every parameter and result is `any`.
    fn arrow(&self, params: &str) -> String {
        if !self.options.typescript {
            return format!("({params}) =>");
        }
        let typed: Vec<String> = params
            .split(", ")
            .filter(|p| !p.is_empty())
            .map(|p| format!("{p}: any"))
            .collect();
        format!("({}): any =>", typed.join(", "))
    }

    /// `const name = value;`, `any` in TypeScript.
    fn constant(&self, name: &str, value: &str) -> String {
        let ty = if self.options.typescript { ": any" } else { "" };
        format!("const {name}{ty} = {value};")
    }

    /// `let` of variables assigned later.
    fn declare(&self, names: &str) -> String {
        if self.options.typescript {
            let typed: Vec<String> = names.split(", ").map(|n| format!("{n}: any")).collect();
            format!("let {};", typed.join(", "))
        } else {
            format!("let {names};")
        }
    }

    fn rt(&mut self, name: &str) -> String {
        self.runtime = true;
        format!("$rt.{name}")
    }

    fn effect(&mut self, name: &str) -> String {
        self.effect = true;
        format!("Effect.{name}")
    }

    /// A marker that maps the generated position to `span`.
    fn mark(&mut self, span: Span) -> String {
        self.spans.push(span);
        format!("{MARK_START}{}{MARK_END}", self.spans.len() - 1)
    }

    /// A top-level value of `module` by its JS name.
    fn global(&mut self, module: &str, name: &str) -> String {
        if module == self.module {
            name.to_owned()
        } else {
            self.imports.insert(module.to_owned());
            format!("{}.{name}", names::module_alias(module))
        }
    }

    fn dict(&mut self, dict: &Dict) -> String {
        match dict {
            Dict::Instance { module, name, args } => {
                let head = self.global(module, &names::instance(name));
                if args.is_empty() {
                    head
                } else {
                    let args: Vec<String> = args.iter().map(|a| self.dict(a)).collect();
                    format!("{head}({})", args.join(", "))
                }
            }
            Dict::Given(shown) => self
                .givens
                .iter()
                .rev()
                .find(|(g, _)| g == shown)
                .map_or_else(
                    || panic!("dictionary `{shown}` is in scope"),
                    |(_, p)| p.clone(),
                ),
            Dict::Super { dict, class } => format!("{}.${class}", self.dict(dict)),
            Dict::Structural { class, fields } => {
                let fields: Vec<String> = fields.iter().map(|f| self.dict(f)).collect();
                let structural = self.rt("structural");
                format!(
                    "{structural}({}, [{}])",
                    names::string(class),
                    fields.join(", ")
                )
            }
            Dict::Primitive { labels, .. } => {
                let labels: Vec<String> = labels.iter().map(|l| names::string(l)).collect();
                format!("[{}]", labels.join(", "))
            }
            Dict::Error => "undefined".to_owned(),
        }
    }

    /// Binds the dictionaries `shown` to fresh parameters; the parameter list.
    fn bind_givens(&mut self, shown: &[String]) -> String {
        let params: Vec<String> = shown
            .iter()
            .map(|g| {
                let p = format!("$d{}", self.givens.len());
                self.givens.push((g.clone(), p.clone()));
                p
            })
            .collect();
        params.join(", ")
    }

    /// Runs `f` with an empty statement list; its statements and result.
    fn block<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> (Vec<String>, T) {
        let saved = mem::take(&mut self.buf);
        let result = f(self);
        (mem::replace(&mut self.buf, saved), result)
    }
}

/// Statements as the lines of a block, indented one level.
fn indent(stmts: &[String]) -> String {
    let mut out = String::new();
    for stmt in stmts {
        for line in stmt.lines() {
            out.push_str("  ");
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}
