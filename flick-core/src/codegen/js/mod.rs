//! ESM for one module. Expressions become JS expressions; what needs
//! statements (`match`, blocks, branches with statements) is emitted into the
//! current statement list, earlier operands spilled into constants first so
//! that evaluation stays left to right.

use crate::build::bindings;
macro_rules! mark {
    ($gen:expr, $span:expr) => {{
        #[cfg(feature = "native-project")]
        {
            ($gen).mark($span)
        }
        #[cfg(not(feature = "native-project"))]
        {
            let _ = &$span;
            String::new()
        }
    }};
}

mod binary;
#[cfg(not(target_arch = "wasm32"))]
mod bridge;
#[cfg(target_arch = "wasm32")]
#[path = "bridge_stub.rs"]
mod bridge;
mod control;
mod exports;
mod expr;
mod foreign;
mod function;
mod generator;
mod group;
mod handle;
mod imports;
mod items;
mod kernel;
mod labels;
mod lift;
mod literal;
mod paths;
mod pattern;
pub use paths::relative;
mod release;
mod schema;
mod services;
mod tidy;
mod view;

pub use release::{direct_methods, exported_worker};

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::mem;

use super::exports::reexports;
use super::tail::{Callee, Target};
use super::ts::Projection;
use super::{Options, Output, names};
use crate::iface::Namespace;
use crate::ir::{Item, ItemKind, Label, Module};
#[cfg(feature = "native-project")]
use crate::source::Span;
use crate::types::Dict;
use exports::annotations;
use exports::sync_exports;

const RUNTIME: &str = bindings::ADAPTER_PREFIX;
const MARK_START: char = '\u{E000}';
const MARK_END: char = '\u{E001}';

/// Compiles `module` to ESM (or TypeScript, see [`Options::typescript`]).
pub fn js(module: &Module, options: &Options) -> Output {
    js_at(module, options, &module.name, &|_, _| None)
}

pub(crate) fn js_at(
    module: &Module,
    options: &Options,
    physical: &str,
    location: &dyn Fn(&str, &str) -> Option<String>,
) -> Output {
    let mut g = Gen::new(module, options);
    g.physical = physical;
    g.location = location;
    let projection =
        Projection::for_runtime(physical, options.interface, options.package, options.rt);
    let mut declarations = Vec::new();
    let interface = (options.interface)(&module.name);
    if let Some(interface) = interface.as_ref().filter(|_| !options.rt) {
        g.sync_exports = sync_exports(interface, &module.name);
        // the wrapper of a Sync export is the runtime's
        g.runtime |= !g.sync_exports.is_empty();
    }
    if options.typescript
        && let Some(interface) = &interface
    {
        let mut interface = interface.clone();
        if physical != module.name {
            for export in &mut interface.exports {
                if export.entity.module == module.name && export.entity.namespace != Namespace::Type
                {
                    export.entity.module = physical.into();
                }
            }
        }
        declarations = projection.declarations(&interface).0;
        g.annotations = annotations(&projection, &interface);
    }
    let body = g.items(&module.items);
    let mut code = String::new();
    let typed = RefCell::new(Vec::new());
    if options.typescript {
        let here = (options.path)(physical);
        let path = |m: &str| {
            typed.borrow_mut().push(m.to_owned());
            relative(&here, &(options.path)(m))
        };
        code.push_str(&projection.imports(&path));
    }
    let here = (options.path)(physical);
    g.write_imports(&mut code, &here);
    if !code.is_empty() {
        code.push('\n');
    }
    for d in &declarations {
        code.push_str(d);
        code.push_str("\n\n");
    }
    code.push_str(&body);
    // a module exposing what it imports: JS consumers find it here too
    let mut barrel = Vec::new();
    if let Some(interface) = (options.interface)(&module.name)
        && physical == module.name
    {
        let path = |m: &str| relative(&here, &(options.path)(m));
        let (lines, modules) = reexports(&module.name, &interface, &path, &|_| true, &|_| {
            options.typescript()
        });
        code.push_str(&lines);
        barrel = modules;
    }
    let mut code = tidy::parens(&code);
    if !options.release {
        code = tidy::layout(&code);
    }
    let mut output = finish(
        #[cfg(feature = "native-project")]
        &code,
        #[cfg(not(feature = "native-project"))]
        code,
        #[cfg(feature = "native-project")]
        &g.spans,
        #[cfg(feature = "native-project")]
        &g.names,
    );
    let imports = g.imports.into_iter();
    let imports = imports.chain(typed.into_inner());
    output.imports = imports.chain(barrel).collect();
    output
}

/// Removes the markers from `code`, recording where each one was.
impl<'a> Gen<'a> {
    fn new(module: &'a Module, options: &'a Options<'a>) -> Self {
        let calls = kernel::calls(options.rt);
        let groups = group::groups(&module.name, &module.items, calls);
        let grouped = group::grouped(&module.name, &module.items, &groups, calls);
        Gen {
            module: &module.name,
            physical: &module.name,
            location: &|_, _| None,
            options,
            labels: &module.labels,
            #[cfg(feature = "native-project")]
            names: HashMap::new(),
            imports: BTreeSet::new(),
            foreign: Vec::new(),
            runtime: false,
            effect: false,
            kernel: options.rt,
            temps: 0,
            givens: Vec::new(),
            #[cfg(feature = "native-project")]
            spans: Vec::new(),
            buf: Vec::new(),
            generator: false,
            sync: false,
            limit: None,
            collector: None,
            owner: None,
            tail: None,
            groups,
            grouped,
            views: HashMap::new(),
            vars: HashSet::new(),
            rests: HashMap::new(),
            annotations: HashMap::new(),
            sync_exports: HashMap::new(),
            workers: if options.release {
                let interface = (options.interface)(&module.name);
                let exported = |name: &str| {
                    interface.as_ref().is_none_or(|i| i.exports_value(name))
                        || module
                            .initialization
                            .prefixes
                            .iter()
                            .any(|p| p.bindings.iter().any(|n| n == name))
                };
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
            direct_foreigns: if options.release && !options.typescript() {
                foreign::plain_foreigns(&module.items, &module.name, options)
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

include!("finish.rs");

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

#[allow(
    clippy::struct_excessive_bools,
    reason = "each flag is a separate fact about the code being emitted"
)]
struct Gen<'a> {
    module: &'a str,
    physical: &'a str,
    location: &'a dyn Fn(&str, &str) -> Option<String>,
    options: &'a Options<'a>,
    /// The module's labelled top-level values (see [`Gen::labelled`]).
    labels: &'a BTreeMap<String, Label>,
    #[cfg(feature = "native-project")]
    /// The Flick name at a marker, by its index in `spans`.
    names: HashMap<usize, String>,
    /// Modules whose exports are used.
    imports: BTreeSet<String>,
    /// Foreign imports: specifier, export, alias.
    foreign: Vec<(String, String, String)>,
    runtime: bool,
    effect: bool,
    /// Compiling for Flick's own runtime: the runtime stage is a generator
    /// the kernel drives, not an Effect (see [`kernel`]).
    kernel: bool,
    temps: u32,
    /// Dictionaries in scope: as `Dict::Given` names them, and the parameter.
    givens: Vec<(String, String)>,
    #[cfg(feature = "native-project")]
    spans: Vec<Span>,
    /// Statements of the block being emitted.
    buf: Vec<String>,
    /// Inside the generator of a runtime stage, where `yield*` runs a call.
    generator: bool,
    /// Inside a Sync stage, where calls run at once.
    sync: bool,
    /// The list a strict `[for …]` pushes its elements to; `None` in a lazy
    /// one, which yields them.
    collector: Option<String>,
    /// The limit of the lazy `for` being emitted and its count of elements.
    limit: Option<(String, String)>,
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
    /// The JS names of `let mut` variables: a read of one is a name that an
    /// assignment after it changes, so it is never left to read later.
    vars: HashSet<String>,
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
    /// Exported functions whose last stage is Sync, by their JS name, with
    /// how many arguments reach it.
    sync_exports: HashMap<String, usize>,
}

impl Gen<'_> {
    fn temp(&mut self) -> String {
        self.temps += 1;
        format!("$t{}", self.temps)
    }

    /// An arrow's parameter list: in TypeScript every parameter and result is `any`.
    fn arrow(&self, params: &str) -> String {
        if !self.options.typescript() {
            return format!("({params}) =>");
        }
        let typed: Vec<String> = params
            .split(", ")
            .filter(|p| !p.is_empty())
            .map(|p| format!("{p}: any"))
            .collect();
        format!("({}): any =>", typed.join(", "))
    }

    /// Native generators have the same internal `any` convention as arrows.
    /// Their public contracts are projected independently from checked Flick types.
    fn generator_function(&self, params: &str, stmts: &[String]) -> String {
        let (params, returns) = if self.options.typescript() {
            (
                params
                    .split(", ")
                    .filter(|p| !p.is_empty())
                    .map(|p| format!("{p}: any"))
                    .collect::<Vec<_>>()
                    .join(", "),
                ": globalThis.Generator<any, any, any>",
            )
        } else {
            (params.to_owned(), "")
        };
        format!("function* ({params}){returns} {{\n{}}}", indent(stmts))
    }

    /// `const name = value;`, `any` in TypeScript.
    fn constant(&self, name: &str, value: &str) -> String {
        let ty = if self.options.typescript() {
            ": any"
        } else {
            ""
        };
        format!("const {name}{ty} = {value};")
    }

    /// `let name = value;` of a `let mut` variable.
    fn variable(&self, name: &str, value: &str) -> String {
        let ty = if self.options.typescript() {
            ": any"
        } else {
            ""
        };
        format!("let {name}{ty} = {value};")
    }

    /// An exported `const`: in TypeScript with its projected type.
    fn export(&self, name: &str, value: &str) -> String {
        // called from TypeScript, outside a Flick sync context, it runs as an Effect
        let wrapped;
        let value = match self.sync_exports.get(name) {
            Some(stages) => {
                wrapped = format!("$rt.syncExport({value}, {stages})");
                wrapped.as_str()
            }
            None => value,
        };
        let value = pure(value);
        if let Some(ty) = self.annotations.get(name) {
            return format!("export const {name}: {ty} = {value};");
        }
        format!("export {}", self.constant(name, &value))
    }

    /// `let` of variables assigned later.
    fn declare(&self, names: &str) -> String {
        if self.options.typescript() {
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

    #[cfg(feature = "native-project")]
    /// A marker that maps the generated position to `span`.
    fn mark(&mut self, span: Span) -> String {
        self.spans.push(span);
        format!("{MARK_START}{}{MARK_END}", self.spans.len() - 1)
    }

    /// A top-level value of `module` by its JS name.
    fn global(&mut self, module: &str, name: &str) -> String {
        let relocated = (self.location)(module, name);
        let target = relocated.as_deref().unwrap_or(module);
        if target == self.physical {
            name.to_owned()
        } else {
            self.imports.insert(target.to_owned());
            format!("{}.{name}", self.alias(target))
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
            names::module_alias(module, 'm')
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
            Dict::Value { module, name } => self.global(module, &names::ident(name)),
            Dict::Opened { binder, field } => {
                format!("{}.{field}", names::ident(binder))
            }
            // bound by the pattern of the constructor whose value holds it
            Dict::Field(local) => names::ident(local),
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
            Dict::Site { place, definition } => format!(
                "{{ place: {}, definition: {} }}",
                names::string(place),
                names::string(definition)
            ),
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

/// Includes optional release workers and direct instance methods, whose
/// physical owner is the same as the binding that defines them.
pub(crate) fn runtime_exports(item: &Item) -> Vec<String> {
    let mut names = items::defined(item);
    if let Some((name, arity)) = release::exported_worker(item) {
        names.push(format!("{}${arity}", self::names::ident(name)));
    }
    names.extend(
        release::direct_methods(item)
            .into_iter()
            .map(|(instance, method)| format!("{}${method}", self::names::instance(instance))),
    );
    names
}
