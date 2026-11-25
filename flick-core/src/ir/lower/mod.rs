//! Lowering of a checked module to the typed IR. The checker has already
//! accepted the module, so every name has a recorded meaning; locals are
//! renamed apart within each top-level definition, so a name never needs
//! shadowing in the generated code.

mod abstraction;
mod applicative_do;
mod comprehension;
#[cfg(feature = "contracts")]
pub mod contracts;
#[cfg(not(feature = "contracts"))]
#[path = "contracts_absent.rs"]
pub mod contracts;
mod debug;
mod defer;
mod derived;
mod do_notation;
mod enums;
mod expr;
mod foreign;
mod generic;
mod handle;
mod holes;
mod interpolation;
mod lambda;
mod lazy;
mod literals;
#[cfg(not(target_arch = "wasm32"))]
mod local_schemes;
mod loops;
mod numeric;
mod owner_descriptors;
mod owners;
mod pattern;
pub mod proofs;
mod question;
mod ranges;
mod type_items;
mod types;
mod wire;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::iter;
use std::mem;

#[cfg(not(target_arch = "wasm32"))]
use super::TypeRef;
use super::{Expr, ExprKind, Item, ItemKind, Lit, Module, Param, Stage};
use crate::diagnostics::{Diagnostic, codes};
#[cfg(not(target_arch = "wasm32"))]
use crate::iface::LocalScheme;
use crate::resolve;
use crate::source::{FileId, Span};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{Dict, Range, StageKey, Typing, class_defaults, default_method, let_parts};

/// Lowers `module`, which checked without errors, with what its check recorded.
pub fn lower(module: &resolve::Module, typing: &Typing) -> Result<Module, Vec<Diagnostic>> {
    lower_inner(module, typing).map(|(ir, _)| ir)
}

/// [`lower`], alongside the module's lowered local `let` / `let rec` schemes
/// keyed by enclosing top-level item and unique local name (for `flick-model`).
#[cfg(not(target_arch = "wasm32"))]
pub fn lower_with_local_schemes(
    module: &resolve::Module,
    typing: &Typing,
) -> Result<(Module, Vec<LocalScheme>), Vec<Diagnostic>> {
    lower_inner(module, typing).map(|(ir, lower)| (ir, lower.local_schemes))
}

fn lower_inner<'a>(
    module: &'a resolve::Module,
    typing: &'a Typing,
) -> Result<(Module, Lower<'a>), Vec<Diagnostic>> {
    let pending = typing.pending_foreign_diagnostics();
    if !pending.is_empty() {
        return Err(pending);
    }
    let mut lower = Lower::new(module, typing);
    let mut items = Vec::new();
    for decl in module.tree.nodes() {
        lower.item(decl, &mut items);
    }
    for info in &typing.instances {
        let span = Span::new(module.file, info.span.0, info.span.1);
        let methods = match (&info.derived, &info.generic, &info.enumerates) {
            (Some((dicts, constructors)), _, _) => {
                derived::methods(&info.class.1, dicts, constructors.as_deref(), span)
            }
            (None, Some(generic), _) => generic::methods(generic, span),
            (None, None, Some(ty)) => enums::methods(&info.module, ty, span),
            (None, None, None) => module
                .tree
                .nodes()
                .find(|n| n.kind == K::InstanceDecl && n.range() == info.span)
                .map(|decl| lower.methods(decl, &info.name))
                .unwrap_or_default(),
        };
        items.push(Item {
            kind: ItemKind::Instance {
                info: Box::new(info.clone()),
                methods,
            },
            span,
        });
    }
    if lower.diagnostics.is_empty() {
        let records = mem::take(&mut lower.records);
        let preambles = mem::take(&mut lower.preambles);
        let exports = super::exports::host_exports(typing, &items);
        Ok((
            Module {
                name: module.name.to_owned(),
                items,
                preambles,
                types: lower.ir_type_decls(),
                labels: typing.labelled.clone(),
                initialization: flick_ir::Initialization::default(),
                records,
                package: None,
                exports,
            },
            lower,
        ))
    } else {
        Err(lower.diagnostics)
    }
}

impl<'a> Lower<'a> {
    fn new(module: &'a resolve::Module, typing: &'a Typing) -> Self {
        Lower {
            module: module.name,
            text: module.text,
            file: module.file,
            generated: module.generated,
            typing,
            scope: Vec::new(),
            used: HashMap::new(),
            vars: HashSet::new(),
            loops: Vec::new(),
            consumed: HashSet::new(),
            reuse: None,
            owners: HashMap::new(),
            entry_owners: HashMap::new(),
            hole_params: HashMap::new(),
            records: BTreeMap::new(),
            preambles: Vec::new(),
            #[cfg(not(target_arch = "wasm32"))]
            collecting_binders: None,
            #[cfg(not(target_arch = "wasm32"))]
            current_item: String::new(),
            #[cfg(not(target_arch = "wasm32"))]
            local_schemes: Vec::new(),
            diagnostics: Vec::new(),
        }
    }
}

/// A local in scope: its source name, whether a plugin generated it, and
/// its unique name.
type Bound = (String, bool, String);

struct Lower<'a> {
    module: &'a str,
    text: &'a str,
    file: FileId,
    /// Where generated code starts in `text`.
    generated: u32,
    typing: &'a Typing,
    /// Locals in scope.
    scope: Vec<Bound>,
    /// How often each source name was bound in the current definition.
    used: HashMap<String, u32>,
    /// The unique names of the `let mut` variables of the current
    /// definition: a read of one is `ExprKind::Var`.
    vars: HashSet<String>,
    /// Enclosing control loops: `(optional_label, loop_id)`.
    loops: Vec<(Option<String>, String)>,
    /// Ranges whose lift or dictionaries are applied already: a block and
    /// its only expression share one.
    consumed: HashSet<(Range, bool)>,
    /// While a later alternative of an or-pattern is lowered: the names
    /// the first one bound.
    reuse: Option<Vec<Bound>>,
    /// The owner local of each source `scope` used by a call or value plan.
    owners: HashMap<Range, String>,
    entry_owners: HashMap<StageKey, String>,
    /// Active expression-hole parameters keyed by `K::HoleExpr` range.
    hole_params: HashMap<Range, String>,
    /// The constructors over a record written in their declaration, with
    /// its labels (`Module::records`).
    records: BTreeMap<String, Vec<String>>,
    /// Inline foreign module preambles (`foreign module inline "..."`).
    preambles: Vec<flick_ir::InlineCode>,
    /// Active pattern binder-type collector for [`Self::pattern_typed`].
    #[cfg(not(target_arch = "wasm32"))]
    collecting_binders: Option<Vec<(String, TypeRef)>>,
    /// Key of the top-level definition currently being lowered.
    #[cfg(not(target_arch = "wasm32"))]
    current_item: String,
    /// Lowered local `let` / `let rec` schemes for `declarations`.
    #[cfg(not(target_arch = "wasm32"))]
    local_schemes: Vec<LocalScheme>,
    diagnostics: Vec<Diagnostic>,
}

impl Lower<'_> {
    fn span(&self, node: &Node) -> Span {
        let (start, end) = node.range();
        Span::new(self.file, start, end)
    }

    fn token_text(&self, node: &Node, kind: K) -> Option<String> {
        node.child_tokens()
            .find(|t| t.kind == kind)
            .map(|t| t.text(self.text).to_owned())
    }

    /// Binds a source name, written at `at`, to a fresh unique name.
    fn bind(&mut self, name: &str, at: u32) -> String {
        let count = self.used.entry(name.to_owned()).or_insert(0);
        let unique = if *count == 0 {
            name.to_owned()
        } else {
            format!("{name}${count}")
        };
        *count += 1;
        let generated = at >= self.generated;
        self.scope
            .push((name.to_owned(), generated, unique.clone()));
        unique
    }

    /// A fresh unique name no source name refers to.
    fn hidden(&mut self, name: &str) -> String {
        let unique = self.bind(name, 0);
        self.scope.pop();
        unique
    }

    /// The unique name of the local `name` a name at `at` refers to: only
    /// generated names see generated locals.
    fn local(&self, name: &str, at: u32) -> Option<&str> {
        let generated = at >= self.generated;
        (self.scope.iter().rev())
            .find(|(n, g, _)| n == name && *g == generated)
            .map(|(_, _, u)| u.as_str())
    }

    fn dictionaries(&self, key: Range) -> Vec<Dict> {
        fn bind(lower: &Lower<'_>, key: Range, dict: &mut Dict) {
            match dict {
                Dict::Opened { binder, .. } => {
                    let unique = lower
                        .local(binder, key.0)
                        .unwrap_or_else(|| panic!("`{binder}` is an opened module in scope"));
                    *binder = unique.to_owned();
                }
                Dict::Instance { args, .. } => {
                    for arg in args {
                        bind(lower, key, arg);
                    }
                }
                Dict::Super { dict, .. } => bind(lower, key, dict),
                Dict::Structural { fields, .. } => {
                    for field in fields {
                        bind(lower, key, field);
                    }
                }
                _ => {}
            }
        }
        let mut dictionaries = self.typing.dicts.get(&key).cloned().unwrap_or_default();
        for dictionary in &mut dictionaries {
            bind(self, key, dictionary);
        }
        dictionaries
    }

    fn givens(&self, node: &Node) -> Vec<String> {
        self.typing
            .givens
            .get(&node.range())
            .cloned()
            .unwrap_or_default()
    }

    /// A definition starts with nothing in scope.
    fn fresh_definition(&mut self) {
        self.scope.clear();
        self.used.clear();
        self.vars.clear();
        self.hole_params.clear();
        #[cfg(not(target_arch = "wasm32"))]
        self.current_item.clear();
    }

    fn let_item(&mut self, decl: &Node, span: Span, items: &mut Vec<Item>) {
        self.fresh_definition();
        let parts = let_parts(decl, self.text);
        let kind = match (&parts.name, parts.pattern) {
            (Some(name), _) => {
                if self.try_native_override(decl, name, span, items) {
                    return;
                }
                #[cfg(not(target_arch = "wasm32"))]
                self.current_item.clone_from(name);
                let (type_params, dict_constraints, ty) = self.ir_top_scheme(name);
                let dicts = self.givens(decl);
                let dict_constraints = if dict_constraints.len() == dicts.len() {
                    dict_constraints
                } else {
                    Vec::new()
                };
                let body = self.definition(decl);
                let ty = ty.or_else(|| body.ty.clone());
                ItemKind::Value {
                    name: name.clone(),
                    type_params,
                    dicts,
                    dict_constraints,
                    ty,
                    body,
                }
            }
            (None, Some(pattern)) => {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    self.current_item = format!("destructure {}..{}", span.start, span.end);
                }
                let body = self.definition(decl);
                let (pattern, binder_types) = self.pattern_typed(pattern);
                ItemKind::Destructure {
                    pattern,
                    binder_types,
                    body,
                }
            }
            (None, None) => return,
        };
        items.push(Item { kind, span });
    }

    fn item(&mut self, decl: &Node, items: &mut Vec<Item>) {
        let span = self.span(decl);
        match decl.kind {
            K::LetDecl => self.let_item(decl, span, items),
            K::RecGroup => {
                for member in decl
                    .nodes()
                    .filter(|n| n.kind == K::Members)
                    .flat_map(Node::nodes)
                {
                    self.item(member, items);
                }
            }
            K::ForeignPreamble => {
                if let Some(preamble) = self.foreign_preamble(decl) {
                    self.preambles.push(preamble);
                }
            }
            K::ForeignDecl => {
                if let Some(foreign) = self.foreign(decl) {
                    let scoped = self
                        .typing
                        .foreigns
                        .get(&decl.range())
                        .map(|i| i.scoped_stages.clone())
                        .unwrap_or_default();
                    if scoped.iter().any(|&s| s)
                        || self
                            .typing
                            .foreigns
                            .get(&decl.range())
                            .is_some_and(|i| i.owner_result.is_some())
                    {
                        self.fresh_definition();
                        let result = self
                            .typing
                            .foreigns
                            .get(&decl.range())
                            .and_then(|i| i.owner_result.as_deref());
                        self.scoped_foreign(foreign, &scoped, result, span, items);
                    } else {
                        items.push(Item {
                            kind: ItemKind::Foreign(foreign),
                            span,
                        });
                    }
                }
            }
            K::TypeDecl | K::ErrorDecl => self.type_items(decl, span, items),
            K::ClassDecl => {
                let Some(class) = self.token_text(decl, K::Con) else {
                    return;
                };
                for (method, member) in class_defaults(decl, self.text) {
                    self.fresh_definition();
                    let name = default_method(&class, &method);
                    #[cfg(not(target_arch = "wasm32"))]
                    self.current_item.clone_from(&name);
                    let body = self.definition(member);
                    let ty = body.ty.clone();
                    let kind = ItemKind::Value {
                        name,
                        type_params: Vec::new(),
                        dicts: self.givens(member),
                        dict_constraints: Vec::new(),
                        ty,
                        body,
                    };
                    items.push(Item {
                        kind,
                        span: self.span(member),
                    });
                }
            }
            K::ServiceDecl => {
                if let Some(name) = self.token_text(decl, K::Con) {
                    let key = format!("{}.{name}", self.module);
                    items.push(Item {
                        kind: ItemKind::Service { name, key },
                        span,
                    });
                }
            }
            _ => {}
        }
    }

    /// The value a `let` defines: a function of its parameters, or its body.
    fn definition(&mut self, decl: &Node) -> Expr {
        let parts = let_parts(decl, self.text);
        let span = self.span(decl);
        let Some(body) = parts.body else {
            return Expr::new(ExprKind::Lit(Lit::Unit), span);
        };
        if parts.params.is_empty() {
            return self.expr(body);
        }
        let stages = self.typing.stages.get(&decl.range()).cloned();
        self.function(&parts.params, body, stages, span)
    }

    fn function(
        &mut self,
        params: &[&Node],
        body: &Node,
        stages: Option<Vec<Stage>>,
        span: Span,
    ) -> Expr {
        self.function_with(params, |l| l.expr(body), stages, span)
    }

    /// [`Self::function`] whose body `body` lowers, with the parameters in
    /// scope.
    fn function_with(
        &mut self,
        params: &[&Node],
        body: impl FnOnce(&mut Self) -> Expr,
        stages: Option<Vec<Stage>>,
        span: Span,
    ) -> Expr {
        let mark = self.scope.len();
        let stages = stages.unwrap_or_default();
        let key = (span.start, span.end);
        for index in 0..params.len() {
            self.register_owner_entry(StageKey::Arrow { node: key, index });
        }
        let params: Vec<_> = params
            .iter()
            .zip(stages.iter().copied().chain(iter::repeat(Stage::Pure)))
            .map(|(p, stage)| {
                let (pat, binder_types) = self.pattern_typed(p);
                Param {
                    pat,
                    stage,
                    ty: self.ir_type(p.range()),
                    binder_types,
                }
            })
            .collect();
        let mut body = body(self);
        self.scope.truncate(mark);
        let entries = (0..params.len()).any(|index| {
            self.entry_owners
                .contains_key(&StageKey::Arrow { node: key, index })
        });
        if entries {
            for (index, param) in params.into_iter().enumerate().rev() {
                body = self.owner_entry(StageKey::Arrow { node: key, index }, body);
                let single = vec![param];
                let ty = types::lambda_type(&single, &body);
                body = Expr::with_ty(
                    ExprKind::Lambda {
                        params: single,
                        body: Box::new(body),
                    },
                    span,
                    ty,
                );
            }
            body
        } else {
            let ty = types::lambda_type(&params, &body);
            Expr::with_ty(
                ExprKind::Lambda {
                    params,
                    body: Box::new(body),
                },
                span,
                ty,
            )
        }
    }

    /// The methods an instance defines.
    fn methods(&mut self, decl: &Node, instance: &str) -> Vec<(String, Expr)> {
        #[cfg(target_arch = "wasm32")]
        let _ = instance;
        decl.nodes()
            .filter(|n| n.kind == K::Members)
            .flat_map(Node::nodes)
            .filter(|n| n.kind == K::LetDecl)
            .filter_map(|member| {
                self.fresh_definition();
                let name = let_parts(member, self.text).name?;
                #[cfg(not(target_arch = "wasm32"))]
                {
                    self.current_item = format!("instance {instance}::{name}");
                }
                let dicts = self.givens(member);
                let value = self.definition(member);
                let value = if dicts.is_empty() {
                    value
                } else {
                    abstraction::abstract_value(dicts, value)
                };
                Some((name, value))
            })
            .collect()
    }

    fn unsupported(&mut self, node: &Node) {
        let what = match node.kind {
            K::TryExpr | K::CatchExpr | K::ProvideExpr => "handlers of effects",
            _ => "`schema` expressions",
        };
        self.diagnostics.push(Diagnostic::error(
            codes::P012,
            format!("{what} are checked, but not compiled yet"),
            self.span(node),
        ));
    }
}
