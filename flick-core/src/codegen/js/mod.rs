//! ESM for one module. Expressions become JS expressions; what needs
//! statements (`match`, blocks, branches with statements) is emitted into the
//! current statement list, earlier operands spilled into constants first so
//! that evaluation stays left to right.

mod binary;
mod expr;
mod foreign;
mod function;
mod generator;
mod group;
mod items;
mod labels;
mod pattern;
mod release;
mod schema;
mod tidy;
mod view;

pub use release::{direct_methods, exported_worker};

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::fmt::Write;
use std::mem;

use super::ts::Projection;
use super::{Options, Output, names};
use crate::iface::{Interface, Namespace};
use crate::ir::{ItemKind, Label, Module};
use crate::source::Span;
use crate::types::Dict;

const RUNTIME: &str = "@flick-lang/runtime";
const MARK_START: char = '\u{E000}';
const MARK_END: char = '\u{E001}';

/// Compiles `module` to ESM (or TypeScript, see [`Options::typescript`]).
pub fn js(module: &Module, options: &Options) -> Output {
    let mut g = Gen::new(module, options);
    let projection = Projection::new(&module.name, options.interface, options.package);
    let mut declarations = Vec::new();
    if options.typescript
        && let Some(interface) = (options.interface)(&module.name)
    {
        declarations = projection.declarations(&interface).0;
        g.annotations = annotations(&projection, &interface);
    }
    let body = g.items(&module.items);
    let mut code = String::new();
    let typed = RefCell::new(Vec::new());
    if options.typescript {
        let here = (options.path)(&module.name);
        let path = |m: &str| {
            typed.borrow_mut().push(m.to_owned());
            relative(&here, &(options.path)(m))
        };
        code.push_str(&projection.imports(&path));
    }
    // the build may name another runtime for it, as for any foreign module
    let runtime = (options.foreign)(&module.name, RUNTIME);
    if g.effect {
        let _ = writeln!(code, "import {{ Effect }} from \"{runtime}\";");
    }
    // code inside a module is untyped: only exports carry types, so what it
    // imports is `any` in TypeScript
    let untyped = |code: &mut String, alias: &str| {
        if options.typescript {
            let _ = writeln!(code, "const {alias}: any = {alias}$;");
        }
    };
    let bound = |alias: &str| {
        if options.typescript {
            format!("{alias}$")
        } else {
            alias.to_owned()
        }
    };
    if g.runtime {
        let _ = writeln!(code, "import * as {} from \"{runtime}\";", bound("$rt"));
        untyped(&mut code, "$rt");
    }
    for (source, export, alias) in &g.foreign {
        let export = if names::is_property(export) {
            export.clone()
        } else {
            names::string(export)
        };
        let _ = writeln!(
            code,
            "import {{ {export} as {} }} from {};",
            bound(alias),
            names::string(source)
        );
        untyped(&mut code, alias);
    }
    let here = (options.path)(&module.name);
    for imported in &g.imports {
        let target = (options.path)(imported);
        let alias = g
            .aliases
            .get(imported)
            .cloned()
            .unwrap_or_else(|| names::module_alias(imported));
        let from = names::string(&relative(&here, &target));
        let _ = writeln!(code, "import * as {} from {from};", bound(&alias));
        untyped(&mut code, &alias);
    }
    if !code.is_empty() {
        code.push('\n');
    }
    for d in &declarations {
        code.push_str(d);
        code.push_str("\n\n");
    }
    code.push_str(&body);
    let mut code = tidy::parens(&code);
    if !options.release {
        code = tidy::layout(&code);
    }
    let mut output = finish(&code, &g.spans, &g.names);
    output.imports = g.imports.into_iter().chain(typed.into_inner()).collect();
    output
}

/// The import specifier of `to` from `from`, both relative to the output
/// root; `npm:` marks a module of a package.
pub fn relative(from: &str, to: &str) -> String {
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
impl<'a> Gen<'a> {
    fn new(module: &'a Module, options: &'a Options<'a>) -> Self {
        let groups = group::groups(&module.name, &module.items);
        let grouped = group::grouped(&module.name, &module.items, &groups);
        Gen {
            module: &module.name,
            options,
            labels: &module.labels,
            names: HashMap::new(),
            imports: BTreeSet::new(),
            foreign: Vec::new(),
            runtime: false,
            effect: false,
            temps: 0,
            givens: Vec::new(),
            spans: Vec::new(),
            buf: Vec::new(),
            generator: false,
            collector: None,
            owner: None,
            tail: None,
            groups,
            grouped,
            views: HashMap::new(),
            rests: HashMap::new(),
            annotations: HashMap::new(),
            workers: if options.release {
                let interface = (options.interface)(&module.name);
                let exported =
                    |name: &str| interface.as_ref().is_none_or(|i| i.exports_value(name));
                let exported_arity = |name: &str| (options.workers)(&module.name, name);
                release::workers(&module.name, &module.items, &exported, &exported_arity)
            } else {
                HashMap::new()
            },
            aliases: HashMap::new(),
            constructors: module
                .items
                .iter()
                .filter_map(|item| match &item.kind {
                    ItemKind::Constructor(c) => Some(names::ident(&c.name)),
                    _ => None,
                })
                .collect(),
            direct_foreigns: if options.release && !options.typescript {
                foreign::plain_foreigns(&module.items)
            } else {
                HashMap::new()
            },
            direct: if options.release {
                release::direct_constants(&module.items)
            } else {
                HashMap::new()
            },
        }
    }
}

fn finish(code: &str, spans: &[Span], names: &HashMap<usize, String>) -> Output {
    let mut out = String::with_capacity(code.len());
    let mut mappings = Vec::new();
    let (mut line, mut col) = (0u32, 0u32);
    let mut chars = code.chars();
    while let Some(c) = chars.next() {
        match c {
            MARK_START => {
                let id: String = chars.by_ref().take_while(|&c| c != MARK_END).collect();
                if let Ok(id) = id.parse::<usize>() {
                    mappings.push((line, col, spans[id], names.get(&id).cloned()));
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
        imports: Vec::new(),
    }
}

/// Where a tail call jumps: the function, or the members of its group (see
/// [`group`]), with their arity; the loop variables; the local each
/// parameter of the member being emitted binds, if it is a name.
struct Tail {
    members: Vec<(Target, usize)>,
    /// The member being emitted.
    current: usize,
    vars: Vec<String>,
    binds: Vec<Option<String>>,
    /// In a group, the variable naming the member that runs next.
    state: Option<String>,
    /// The offset variable of each position holding a walked list (see
    /// [`view`]).
    offsets: Vec<Option<String>>,
}

/// A function a tail call may jump to: its name and the dictionaries of its
/// constraints, by the names `Dict::Given` gives them, which a call passes
/// on.
#[derive(Clone, PartialEq, Eq)]
struct Target {
    callee: Callee,
    dicts: Vec<String>,
}

#[derive(Clone, PartialEq, Eq)]
enum Callee {
    Local(String),
    Global(String),
}

struct Gen<'a> {
    module: &'a str,
    options: &'a Options<'a>,
    /// The module's labelled top-level values (see [`Gen::labelled`]).
    labels: &'a HashMap<String, Label>,
    /// The Flick name at a marker, by its index in `spans`.
    names: HashMap<usize, String>,
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
    /// The list a strict `[for …]` pushes its elements to; `None` in a lazy
    /// one, which yields them.
    collector: Option<String>,
    /// The owner the next runtime call acquires into, from `Within`.
    owner: Option<String>,
    tail: Option<Tail>,
    /// The groups of mutually tail-recursive functions of the module.
    groups: Vec<group::Group>,
    /// The group and member of each grouped function, by its global name.
    grouped: HashMap<String, (usize, usize)>,
    /// The walked lists in scope, by their JS name: the offset variable and
    /// the loop position (see [`view`]).
    views: HashMap<String, (String, usize)>,
    /// The `...rest` of a walked list, by its local: the loop position and
    /// the offset it starts at.
    rests: HashMap<String, (usize, String)>,
    /// Release: the top-level functions of the module with a worker taking
    /// all their arguments at once, by name, with its arity and whether its
    /// curried form is still needed.
    workers: HashMap<String, (usize, bool)>,
    /// Release: the methods of the module's canonical instances emitted as
    /// constants of their own, by instance and method.
    direct: HashMap<(String, String), String>,
    /// Release, JS: the plain pure foreigns of the module (see
    /// [`foreign::plain`]), by name, with their arity and import alias.
    direct_foreigns: HashMap<String, (usize, String)>,
    /// The name each imported module goes by (see [`Gen::alias`]).
    aliases: HashMap<String, String>,
    /// The constructors of the module, which a module alias must not shadow.
    constructors: Vec<String>,
    /// TypeScript types of the exported values, by their JS name.
    annotations: HashMap<String, String>,
}

/// The TypeScript type of each exported value and constructor that has
/// one; the others stay `any`.
fn annotations(projection: &Projection, interface: &Interface) -> HashMap<String, String> {
    interface
        .exports
        .iter()
        .filter(|e| {
            e.entity.module == projection.module
                && matches!(
                    e.entity.namespace,
                    Namespace::Value | Namespace::Constructor
                )
        })
        .filter_map(|e| {
            let ty = projection.value(e.scheme.as_ref()?).ok()?;
            Some((names::ident(&e.entity.name), ty))
        })
        .collect()
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

    /// An exported `const`: in TypeScript with its projected type.
    fn export(&self, name: &str, value: &str) -> String {
        let value = pure(value);
        match self.annotations.get(name) {
            Some(ty) => format!("export const {name}: {ty} = {value};"),
            None => format!("export {}", self.constant(name, &value)),
        }
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
            format!("{}.{name}", self.alias(module))
        }
    }

    /// The name `module` is imported under: in debug its own (`Ui`,
    /// `Ui_Event`) unless a constructor of this module, a JS global or
    /// another import has it; `$m_Ui` otherwise.
    fn alias(&mut self, module: &str) -> String {
        if let Some(alias) = self.aliases.get(module) {
            return alias.clone();
        }
        let readable = module.replace('.', "_");
        let free = !self.options.release
            && !names::is_global(&readable)
            && !self.constructors.contains(&readable)
            && !self.aliases.values().any(|a| *a == readable);
        let alias = if free {
            readable
        } else {
            names::module_alias(module)
        };
        self.aliases.insert(module.to_owned(), alias.clone());
        alias
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

/// A top-level `value` that is a call or an immediately run arrow, marked
/// `/*#__PURE__*/`: top-level initialization is pure (spec 11), so a bundler
/// may drop it when nothing uses it.
fn pure(value: &str) -> String {
    // a call starts with the marker of its span
    let code = expr::unmarked(value);
    let rest =
        code.trim_start_matches(|c: char| c.is_alphanumeric() || c == '_' || c == '$' || c == '.');
    let call = rest.len() < code.len() && rest.starts_with('(');
    if call || code.starts_with("((") {
        format!("/*#__PURE__*/ {value}")
    } else {
        value.to_owned()
    }
}
