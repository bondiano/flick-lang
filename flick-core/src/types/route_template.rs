//! A route constructor where a `Symbol` is expected stands for its path:
//! with `type ApiRoute = Profiles Username (Option Follow) | …` deriving
//! `Url`, `Profiles Username (Some Follow)` is `"/profiles/:username/follow"`.
//! A field is written as its type, a parameter named after the type in
//! snake case; an `Option` field as `None` or `Some x`; a field of a type
//! whose constructors have no fields, or of a nested route type, as one of
//! its constructors. The URL is the one `Route.print` makes of such a value,
//! so `deriving HttpApi` paths and page links share one route type. The
//! route type comes from another module, whose constructors are known by
//! the time this one's types are.

use super::Checker;
use super::env::{Form, Found};
use super::http_api::snake;
use super::kinds::Kind;
use super::schemas::{companion_name, kebab};
use super::ty::Type;
use crate::diagnostics::codes;
use crate::iface::{Entity, Namespace};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Checker<'_> {
    /// The `Symbol` of a route template where one is expected, or the type
    /// error it is; `None` for any other type.
    pub(super) fn symbol_route(&mut self, node: &Node, expected: &Kind) -> Option<Type> {
        if self.kinds.resolve(expected) != Kind::Symbol {
            return None;
        }
        Some(match self.route_template(node)? {
            Ok(path) => Type::Symbol(path),
            Err(message) => {
                self.error(codes::D009, self.span(node), message);
                Type::Error
            }
        })
    }

    /// The path the route template `node` stands for, `None` when `node`
    /// names no constructor, or why it is no path.
    pub(super) fn route_template(&self, node: &Node) -> Option<Result<String, String>> {
        let (head, args) = applied(node)?;
        if head.child_tokens().any(|t| t.kind == K::String) {
            return None;
        }
        let constructor = self.template_constructor(head)?;
        let mut segments = Vec::new();
        Some(
            self.template_segments(&constructor, &args, &mut segments)
                .map(|()| format!("/{}", segments.join("/"))),
        )
    }

    /// The constructor a name node names, by an import's qualifier or in scope.
    fn template_constructor(&self, node: &Node) -> Option<Entity> {
        let names: Vec<&str> = node
            .child_tokens()
            .filter(|t| t.kind == K::Con)
            .map(|t| t.text(self.text))
            .collect();
        let (name, qualifier) = names.split_last()?;
        if qualifier.is_empty() {
            return match self.env.find(Namespace::Constructor, name) {
                Found::Entity(e) => Some(e),
                _ => None,
            };
        }
        let (_, interface) = self.env.qualified_module(&qualifier.join("."))?;
        Some(
            interface?
                .find(Namespace::Constructor, name)?
                .entity
                .clone(),
        )
    }

    fn template_segments(
        &self,
        constructor: &Entity,
        args: &[&Node],
        out: &mut Vec<String>,
    ) -> Result<(), String> {
        let name = &constructor.name;
        let (module, ty) = self
            .env
            .parent(constructor)
            .ok_or_else(|| format!("`{name}` is no constructor of a route type"))?;
        let url = Entity {
            module: module.clone(),
            namespace: Namespace::Value,
            name: companion_name(&ty, "url"),
        };
        if self.env.value(&url).is_none() {
            return Err(format!(
                "`{name}` stands for a path when `{ty}` derives `Url`, from another module"
            ));
        }
        let fields = self.entity_fields(constructor);
        if fields.len() != args.len() {
            return Err(format!(
                "`{name}` has {} field(s), each written as its type, `None` or `Some …`: here {}",
                fields.len(),
                args.len()
            ));
        }
        if !matches!(name.as_str(), "Home" | "Index") {
            out.push(kebab(name));
        }
        for (field, arg) in fields.iter().zip(args) {
            self.template_field(name, field, arg, out)?;
        }
        Ok(())
    }

    /// One field of a route constructor, written as `arg`.
    fn template_field(
        &self,
        name: &str,
        field: &Type,
        arg: &Node,
        out: &mut Vec<String>,
    ) -> Result<(), String> {
        let (head, args) = applied(arg).ok_or_else(|| {
            format!("a field of `{name}` is written as its type, `None` or `Some …`")
        })?;
        let mut field = field;
        while let Type::Captured { ty, .. } | Type::Bounded { ty, .. } = field {
            field = ty;
        }
        let (field_head, field_args) = field.spine();
        let Type::Con {
            module: field_module,
            name: field_ty,
        } = field_head
        else {
            return Err(format!("`{name}` has a field of no route form"));
        };
        if let Type::Record(_) = field {
            return Err(format!("`{name}` has a query, which is no part of a path"));
        }
        let constructor = self.template_constructor(head);
        if field_head.is_prim("Option") {
            let inner = field_args.first().copied().cloned().unwrap_or(Type::Error);
            return match constructor.as_ref().map(|c| c.name.as_str()) {
                Some("None") if args.is_empty() => Ok(()),
                Some("Some") if args.len() == 1 => self.template_field(name, &inner, args[0], out),
                _ => Err(format!(
                    "the `Option` field of `{name}` is written as `None` or `Some …`"
                )),
            };
        }
        // a constructor of the field's type: a word, or a nested route
        if let Some(c) = constructor
            && self.env.parent(&c) == Some((field_module.clone(), field_ty.clone()))
        {
            let words = matches!(
                self.env.type_def(field_module, field_ty).map(|d| &d.form),
                Some(Form::Adt { constructors, .. })
                    if constructors.iter().all(|k| self.entity_fields(&Entity {
                        module: field_module.clone(),
                        namespace: Namespace::Constructor,
                        name: k.clone(),
                    }).is_empty())
            );
            if words && args.is_empty() {
                out.push(kebab(&c.name));
                return Ok(());
            }
            return self.template_segments(&c, &args, out);
        }
        let written: Vec<&str> = head
            .child_tokens()
            .filter(|t| t.kind == K::Con)
            .map(|t| t.text(self.text))
            .collect();
        let same = args.is_empty()
            && match self.env.find_type(&written.join(".")) {
                Found::Entity(e) => e.module == *field_module && e.name == *field_ty,
                _ => false,
            };
        if !same {
            return Err(format!(
                "a field of `{name}` is `{field_ty}`: write it as `{field_ty}`, or as one of its constructors"
            ));
        }
        if field_module == super::ty::PRELUDE || !field_args.is_empty() {
            return Err(format!(
                "a parameter of a path is named after its type: give the `{field_ty}` field of `{name}` a type of its own, `refined type Username = String where …`"
            ));
        }
        out.push(format!(":{}", snake(field_ty)));
        Ok(())
    }
}

/// The head and arguments of a type application, through parentheses.
fn applied(node: &Node) -> Option<(&Node, Vec<&Node>)> {
    match node.kind {
        K::ParenType => applied(node.nodes().next()?),
        K::NameType => Some((node, Vec::new())),
        K::AppType => {
            let mut children = node.nodes();
            let head = children.next()?;
            (head.kind == K::NameType).then(|| (head, children.collect()))
        }
        _ => None,
    }
}
