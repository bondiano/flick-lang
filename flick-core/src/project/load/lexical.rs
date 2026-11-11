//! Source prefixes are checked as internal graph nodes. Their entity owner
//! remains the defining module; they never become separate runtime modules.
use std::path::PathBuf;
use std::sync::Arc;

use super::{LexicalScope, Loaded, Loader, StaticCapture, Virtual};
use crate::resolve::{self, Import, Item, ItemKind};
use crate::source::Span;
use crate::syntax::{
    Parse,
    cst::{Element, Node},
    kind::SyntaxKind as K,
};

pub(super) fn scope_unit(
    parent: &Loaded,
    scope: &LexicalScope,
    mut capture: StaticCapture,
) -> Option<Loaded> {
    let mut children = Vec::new();
    let mut declarations = false;
    for node in parent.parse.tree.nodes() {
        let header = parent.syntax.header.as_ref().is_some_and(|h| {
            node.nodes()
                .any(|n| n.kind == K::ModulePath && n.range() == (h.span.start, h.span.end))
        });
        if header {
            children.push(Element::Node(Node {
                kind: K::ModuleHeader,
                children: node
                    .children
                    .iter()
                    .filter(|e| match e {
                        Element::Node(n) => matches!(n.kind, K::ModulePath | K::ModuleParamGroup),
                        Element::Token(t) => t.kind == K::ModuleKw,
                    })
                    .cloned()
                    .collect(),
            }));
        } else if node.kind == K::ImportDecl || node.range().1 <= scope.before {
            declarations |= node.kind != K::ImportDecl;
            children.push(Element::Node(node.clone()));
        }
    }
    if !declarations
        && parent
            .syntax
            .header
            .as_ref()
            .is_none_or(|h| h.parameters.is_empty())
    {
        return None;
    }
    let tree = Node {
        kind: K::SourceFile,
        children,
    };
    // Parsing has already happened; the original text and spans are retained.
    capture.bindings.truncate(scope.bindings);
    capture.inherited_imports = scope.inherited_imports;
    capture.callee = None;
    capture.arguments.clear();
    capture.template = None;
    let prefix = capture
        .scopes
        .iter()
        .position(|known| known.key() == scope.key())
        .unwrap_or(capture.scopes.len());
    capture.scopes.truncate(prefix);
    Some(Loaded {
        path: PathBuf::from(format!("{}#scope:{}", parent.path.display(), scope.before)),
        file: parent.file,
        root: parent.root.clone(),
        name: parent.name.clone(),
        parse: Arc::new(Parse {
            tree,
            diagnostics: Vec::new(),
        }),
        syntax: parent.syntax.clone(),
        expansion: None,
        editing: Vec::new(),
        loader: None,
        virtual_of: Some(Virtual {
            at: Span::new(parent.file, scope.before, scope.before),
            capture,
        }),
        member_of: None,
        scope_of: Some(scope.clone()),
    })
}

impl Loader<'_> {
    pub(super) fn capture_scope(&mut self, parent: usize, before: u32) -> Option<LexicalScope> {
        let capture = self.modules[parent]
            .virtual_of
            .as_ref()
            .map_or_else(StaticCapture::default, |v| v.capture.clone());
        let scope = LexicalScope {
            module: self.modules[parent].name.clone(),
            before,
            bindings: capture.bindings.len(),
            inherited_imports: capture.inherited_imports,
        };
        if self.by_name.contains_key(&scope.key()) {
            return Some(scope);
        }
        let unit = scope_unit(&self.modules[parent], &scope, capture)?;
        self.add_scope(unit);
        Some(scope)
    }

    fn add_scope(&mut self, mut unit: Loaded) {
        let scope = unit.scope_of.as_ref().expect("scope node");
        let text = &self.sources.get(unit.file).text;
        let mut syntax = resolve::module_syntax(&unit.parse.tree, text, unit.file);
        if let Some(header) = &mut syntax.header {
            header.private = true;
            header.result = None;
            header.body = None;
            header.materialized_body = None;
            header.exposing = Some(vec![Item {
                kind: ItemKind::All,
                name: String::new(),
                span: header.span,
            }]);
        }
        unit.syntax = Arc::new(syntax);
        self.by_name.insert(scope.key(), self.modules.len());
        self.modules.push(unit);
    }

    pub(super) fn scope_imports(&mut self, module: usize) {
        let scopes = self.modules[module]
            .virtual_of
            .as_ref()
            .map_or_else(Vec::new, |v| v.capture.scopes.clone());
        for scope in scopes {
            if !self.by_name.contains_key(&scope.key()) {
                let at = self.modules[module]
                    .syntax
                    .header
                    .as_ref()
                    .map_or(Span::new(self.modules[module].file, 0, 0), |h| h.span);
                self.find_module(module, &scope.module, at);
                let capture = self.modules[module]
                    .virtual_of
                    .as_ref()
                    .expect("scope capture")
                    .capture
                    .clone();
                let unit = self
                    .by_name
                    .get(&scope.module)
                    .and_then(|&parent| scope_unit(&self.modules[parent], &scope, capture.clone()));
                #[cfg(feature = "native-project")]
                let unit = unit.or_else(|| {
                    self.scope_source(&scope.module)
                        .and_then(|parent| scope_unit(&parent, &scope, capture))
                });
                if let Some(unit) = unit {
                    self.add_scope(unit);
                }
            }
            let at = self.modules[module]
                .syntax
                .header
                .as_ref()
                .map_or(Span::new(self.modules[module].file, 0, 0), |h| h.span);
            let syntax = Arc::make_mut(&mut self.modules[module].syntax);
            if !syntax.imports.iter().any(|i| i.module == scope.key()) {
                syntax.imports.push(Import {
                    module: scope.key(),
                    alias: None,
                    exposing: Vec::new(),
                    plugins: Vec::new(),
                    span: at,
                });
            }
        }
    }
}
