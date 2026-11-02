//! `deriving HttpApi`: a record whose fields are endpoints,
//! `field : Get "/path" { path : P; query : Q; headers : H; payload : B } S {E…}`,
//! is a group of an API (see the built-in module `HttpApi.Derive`). Its
//! companions are `T$endpoints`, the record of its endpoints, and `T$api`,
//! the group or, with `Authed` and `MaybeAuthed` endpoints, a function of
//! the security scheme, the service and its two checks to the group. Their
//! types are made here from the written ones; lowering builds the values
//! from the `HttpApi` functions a program would call, so both targets run
//! them as they are.

use super::Checker;
use super::classes::Dict;
use super::classes::Outcome;
use super::env::Form;
use super::kinds::Kind;
use super::ty::{Binder, PRELUDE, Row, Type, error_label};
use super::typing::{Constructor, Range, range};
use crate::diagnostics::codes;
use crate::iface::{Entity, Namespace};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::literal::string_value;

/// The module of the endpoint types a record that derives `HttpApi` uses.
pub const HTTP_API_DERIVE: &str = "HttpApi.Derive";
const HTTP_API: &str = "HttpApi";

/// The parts of a request, as `HttpApi.Request` names them.
const PARTS: [&str; 4] = ["path", "query", "headers", "payload"];

/// A record that derives `HttpApi`, for lowering.
#[derive(Debug, Clone)]
pub struct HttpApiItem {
    pub ty: String,
    pub decl: Range,
    /// The group's name: the type's in snake case.
    pub group: String,
    pub endpoints: Vec<EndpointPlan>,
    /// Whether `T$api` takes the scheme, the service and its checks.
    pub secured: bool,
}

#[derive(Debug, Clone)]
pub struct EndpointPlan {
    pub label: String,
    pub method: Constructor,
    pub path: String,
    /// The parts of the request, as `with_<part>` takes them, and their codecs.
    pub parts: Vec<(&'static str, Codec)>,
    /// The status and codec of the success; none for `Unit`, sent as 204.
    pub success: Option<(i32, Codec)>,
    /// The `Problem` dictionary of each error.
    pub errors: Vec<Dict>,
    pub auth: Auth,
}

/// The codec of a part or a success.
#[derive(Debug, Clone)]
pub enum Codec {
    /// The value named after the type in its module: `update_user_body`.
    Value { module: String, name: String },
    /// `Codec.json` of the type's witness `T$schema`.
    Schema { module: String, ty: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Auth {
    None,
    Required,
    Optional,
}

/// What the companions' types are made of, per endpoint.
struct Shape {
    plan: EndpointPlan,
    /// `Request p q h b`, by part.
    parts: [Type; 4],
    success: Type,
    errors: Vec<(String, Type)>,
}

/// `UpdateUserBody` → `update_user_body`.
pub fn snake(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Whether `decl` derives `HttpApi`.
fn derives_api(decl: &Node, text: &str) -> bool {
    decl.nodes()
        .filter(|n| n.kind == K::DerivingClause)
        .flat_map(Node::child_tokens)
        .any(|t| t.kind == K::Con && t.text(text) == "HttpApi")
}

impl Checker<'_> {
    /// The companions of each record of the module that derives `HttpApi`.
    pub(super) fn declare_http_apis(&mut self, decls: &[&Node]) {
        for decl in decls
            .iter()
            .filter(|d| d.kind == K::TypeDecl && derives_api(d, self.text))
        {
            let Some(name) = self.con_token(decl) else {
                continue;
            };
            let at = decl
                .nodes()
                .find(|n| n.kind == K::DerivingClause)
                .map_or_else(|| self.span(decl), |c| self.span(c));
            let Some(record) = decl.nodes().find(|n| n.kind == K::RecordType) else {
                self.error(
                    codes::D011,
                    at,
                    format!("`{name}` derives `HttpApi`: it is a record of endpoints, `field : Get \"/path\" {{ … }} Response {{Errors}}`"),
                );
                continue;
            };
            let mut shapes = Vec::new();
            for field in record.nodes().filter(|n| n.kind == K::RecordTypeField) {
                match self.endpoint(field) {
                    Ok(shape) => shapes.push(shape),
                    Err(message) => self.error(codes::D011, self.span(field), message),
                }
            }
            if shapes.len()
                == record
                    .nodes()
                    .filter(|n| n.kind == K::RecordTypeField)
                    .count()
            {
                self.http_api_companions(&name, decl, shapes, at);
            }
        }
    }

    /// The types of `T$endpoints` and `T$api` and the plan lowering follows.
    fn http_api_companions(&mut self, name: &str, decl: &Node, shapes: Vec<Shape>, at: Span) {
        let r = Type::Var("r".to_owned());
        let a = Type::Var("a".to_owned());
        let row = |name: &str| Binder {
            name: name.to_owned(),
            kind: Kind::ServiceRow,
        };
        let secured: Vec<&Shape> = shapes
            .iter()
            .filter(|s| s.plan.auth != Auth::None)
            .collect();
        // `HttpApi.authenticated` serves a group whose every endpoint makes the service
        if !secured.is_empty() && secured.len() < shapes.len() {
            self.error(
                codes::D011,
                at,
                "every endpoint of a group with `Authed` endpoints is `Authed` or `MaybeAuthed`: put the others in a group of their own",
            );
            return;
        }
        let headers = secured.first().map(|s| s.parts[2].clone());
        if secured
            .iter()
            .any(|s| Some(&s.parts[2]) != headers.as_ref())
        {
            self.error(
                codes::D011,
                at,
                "the `Authed` and `MaybeAuthed` endpoints of a group have the same `headers`: its checks read them",
            );
            return;
        }
        // the checks fail only with errors every endpoint they guard declares
        let checked: Vec<(String, Type)> = secured.first().map_or_else(Vec::new, |first| {
            first
                .errors
                .iter()
                .filter(|(l, _)| secured.iter().all(|s| s.errors.iter().any(|(m, _)| m == l)))
                .cloned()
                .collect()
        });
        let mut endpoints = Vec::new();
        let mut handlers = Vec::new();
        for shape in &shapes {
            let label = shape.plan.label.clone();
            let services = if shape.plan.auth == Auth::None {
                r.clone()
            } else {
                Type::Union(Box::new(a.clone()), Box::new(r.clone()))
            };
            let endpoint = Type::apps(
                Type::con(HTTP_API, "Endpoint"),
                [Type::Symbol(label.clone()), self.handler(shape, r.clone())],
            );
            endpoints.push((label.clone(), endpoint));
            handlers.push((label, self.handler(shape, services)));
        }
        let group = Type::apps(
            Type::con(HTTP_API, "Group"),
            [
                Type::Symbol(snake(name)),
                r.clone(),
                Type::Record(Row::closed(handlers)),
            ],
        );
        let (api, binders) = match &headers {
            Some(h) => {
                let args = [a, r, h.clone(), Type::Row(Row::closed(checked)), group];
                (self.alias("Secured", &args), vec![row("a"), row("r")])
            }
            None => (group, vec![row("r")]),
        };
        let forall = |body: Type, vars: Vec<Binder>| Type::Forall {
            vars,
            constraints: Vec::new(),
            body: Box::new(body),
        };
        self.http_api_companion(
            name,
            "endpoints",
            forall(Type::Record(Row::closed(endpoints)), vec![row("r")]),
        );
        self.http_api_companion(name, "api", forall(api, binders));
        self.typing.http_apis.push(HttpApiItem {
            ty: name.to_owned(),
            decl: range(self.span(decl)),
            group: snake(name),
            secured: headers.is_some(),
            endpoints: shapes.into_iter().map(|s| s.plan).collect(),
        });
    }

    fn http_api_companion(&mut self, ty: &str, member: &str, scheme: Type) {
        let companion = super::companion_name(ty, member);
        self.env.declare(Namespace::Value, &companion);
        let entity = self.env.entity(Namespace::Value, &companion);
        self.env.set_value(entity, scheme);
    }

    /// `HttpApi.Derive.Handler` of an endpoint with the services `r`.
    fn handler(&self, shape: &Shape, services: Type) -> Type {
        let [path, query, headers, payload] = shape.parts.clone();
        let errors = Type::Row(Row::closed(shape.errors.clone()));
        let args = [
            path,
            query,
            headers,
            payload,
            services,
            errors,
            shape.success.clone(),
        ];
        self.alias("Handler", &args)
    }

    /// The body of the alias `name` of `HttpApi.Derive` at `args`.
    fn alias(&self, name: &str, args: &[Type]) -> Type {
        match self.env.type_def(HTTP_API_DERIVE, name).map(|d| &d.form) {
            Some(Form::Alias { params, body, .. }) => {
                let subst: Vec<(String, Type)> =
                    params.iter().cloned().zip(args.iter().cloned()).collect();
                body.substitute(&subst)
            }
            _ => Type::Error,
        }
    }

    /// The endpoint the record field `field` declares.
    fn endpoint(&mut self, field: &Node) -> Result<Shape, String> {
        let label = field
            .child_tokens()
            .find(|t| t.kind == K::Ident)
            .map(|t| t.text(self.text).to_owned())
            .unwrap_or_default();
        let written = field.nodes().next().ok_or("an endpoint has a type")?;
        let (mut head, mut args) = self.head(written).ok_or("")?;
        let mut auth = Auth::None;
        if head.0 == HTTP_API_DERIVE && matches!(head.1.as_str(), "Authed" | "MaybeAuthed") {
            auth = if head.1 == "Authed" {
                Auth::Required
            } else {
                Auth::Optional
            };
            let inner = *args.first().ok_or("`Authed` takes the endpoint")?;
            (head, args) = self.head(inner).ok_or("")?;
        }
        let methods = ["Get", "Post", "Put", "Patch", "Delete"];
        let (true, true, [path, request, success, errors]) = (
            head.0 == HTTP_API_DERIVE,
            methods.contains(&head.1.as_str()),
            args.as_slice(),
        ) else {
            return Err(format!(
                "endpoint `{label}` is `Get`, `Post`, `Put`, `Patch` or `Delete` of `HttpApi.Derive` with a path, the request, the success and the errors"
            ));
        };
        let path = path
            .child_tokens()
            .find(|t| t.kind == K::String)
            .map(|t| string_value(t.text(self.text)))
            .ok_or_else(|| format!("endpoint `{label}`: the path is a string, `\"/users/:id\"`"))?;
        let (parts, codecs) = self.request_parts(&label, request)?;
        self.path_params(&label, &path, &parts[0])?;
        let (status, success) = match self.head(success) {
            Some(((m, n), inner)) if m == HTTP_API_DERIVE && n == "Created" && inner.len() == 1 => {
                (201, inner[0])
            }
            _ => (200, *success),
        };
        let success_ty = self.annotation(success);
        let success_codec = if success_ty.is_prim("Unit") {
            None
        } else {
            Some((status, self.codec(success)?))
        };
        let mut rows = Vec::new();
        let mut dicts = Vec::new();
        for error in errors.nodes() {
            let ty = self.annotation(error);
            let problem = Type::app(Type::con(HTTP_API_DERIVE, "Problem"), ty.clone());
            let Outcome::Solved(dict) = self.entails(&problem, &[]) else {
                return Err(format!(
                    "endpoint `{label}`: the error `{}` has no `HttpApi.Derive.Problem` instance, its status and body",
                    self.show(&ty)
                ));
            };
            rows.extend(error_label(&ty).map(|l| (l, ty)));
            dicts.push(dict);
        }
        let method = Constructor {
            module: HTTP_API.to_owned(),
            ty: "Method".to_owned(),
            name: head.1.clone(),
            arity: 0,
            rep: self.env.constructor_rep(HTTP_API, "Method", &head.1),
        };
        Ok(Shape {
            plan: EndpointPlan {
                label,
                method,
                path,
                parts: codecs,
                success: success_codec,
                errors: dicts,
                auth,
            },
            parts,
            success: success_ty,
            errors: rows,
        })
    }

    /// The types of the request's parts, `Unit` for a missing one, and the
    /// codec of each present one.
    #[allow(clippy::type_complexity, reason = "the parts and their codecs")]
    fn request_parts(
        &mut self,
        label: &str,
        request: &Node,
    ) -> Result<([Type; 4], Vec<(&'static str, Codec)>), String> {
        let unit = Type::prim("Unit");
        let mut parts = [unit.clone(), unit.clone(), unit.clone(), unit];
        let mut codecs = Vec::new();
        let fields: Vec<&Node> = match request.kind {
            K::RecordType => request
                .nodes()
                .filter(|n| n.kind == K::RecordTypeField)
                .collect(),
            K::RowType if request.nodes().next().is_none() => Vec::new(),
            _ => {
                return Err(format!(
                    "endpoint `{label}`: the request is a record of its parts, `{{ path : P; payload : B }}`, or `{{}}`"
                ));
            }
        };
        for field in fields {
            let part = field
                .child_tokens()
                .find(|t| t.kind == K::Ident)
                .map_or("", |t| t.text(self.text));
            let Some(i) = PARTS.iter().position(|p| *p == part) else {
                return Err(format!(
                    "endpoint `{label}`: `{part}` is not a part of a request: `path`, `query`, `headers` or `payload`"
                ));
            };
            let ty = field.nodes().next().ok_or("a part has a type")?;
            parts[i] = self.annotation(ty);
            codecs.push((PARTS[i], self.codec(ty)?));
        }
        Ok((parts, codecs))
    }

    /// Each `:name` of the path is a field of the path part, and back.
    fn path_params(&mut self, label: &str, path: &str, part: &Type) -> Result<(), String> {
        let mut params: Vec<&str> = path
            .split('/')
            .filter_map(|s| s.strip_prefix(':'))
            .collect();
        params.sort_unstable();
        let mut fields: Vec<String> = match self.whnf(part) {
            Type::Record(row) => row.entries.into_iter().map(|(l, _)| l).collect(),
            _ => Vec::new(),
        };
        fields.sort();
        if params == fields {
            return Ok(());
        }
        Err(format!(
            "endpoint `{label}`: the path `{path}` has the parameters {}, and its `path` part the fields {}",
            listed(&params),
            listed(&fields)
        ))
    }

    /// The codec of the type written at `node`: the value named after it in
    /// its module, else the JSON codec of its schema.
    fn codec(&mut self, node: &Node) -> Result<Codec, String> {
        let names: Vec<&str> = node
            .child_tokens()
            .filter(|t| t.kind == K::Con)
            .map(|t| t.text(self.text))
            .collect();
        let shown = names.join(".");
        let no_codec = || {
            format!(
                "`{shown}` has no codec: name a `PureCodec Json` of it after it in its module, or derive `Schema`"
            )
        };
        let (true, Some((name, qualifier))) = (node.kind == K::NameType, names.split_last()) else {
            return Err(no_codec());
        };
        let (module, _) = self
            .find_type(node, &qualifier.join("."), name)
            .ok_or_else(no_codec)?;
        // the value named after the type, a refined type's `T$json`, and a
        // built-in type's in its module: `String.json`
        let mut candidates = vec![
            (module.clone(), snake(name)),
            (module.clone(), super::companion_name(name, "json")),
        ];
        if module == PRELUDE {
            candidates.push(((*name).to_owned(), "json".to_owned()));
        }
        let found = candidates.into_iter().find(|(module, name)| {
            let entity = Entity {
                module: module.clone(),
                namespace: Namespace::Value,
                name: name.clone(),
            };
            self.env.value(&entity).is_some()
        });
        if let Some((module, name)) = found {
            return Ok(Codec::Value { module, name });
        }
        let schema = Entity {
            module: module.clone(),
            namespace: Namespace::Value,
            name: super::companion_name(name, "schema"),
        };
        if self.env.value(&schema).is_some() {
            return Ok(Codec::Schema {
                module,
                ty: (*name).to_owned(),
            });
        }
        Err(no_codec())
    }

    /// The type a node names and its arguments: `Get "/" {} S {}` →
    /// `(HttpApi.Derive, Get)` and four nodes.
    fn head<'n>(&mut self, node: &'n Node) -> Option<((String, String), Vec<&'n Node>)> {
        match node.kind {
            K::ParenType => self.head(node.nodes().next()?),
            K::AppType => {
                let mut nodes = node.nodes();
                let (head, _) = self.head(nodes.next()?)?;
                Some((head, nodes.collect()))
            }
            K::NameType => {
                let names: Vec<&str> = node
                    .child_tokens()
                    .filter(|t| t.kind == K::Con)
                    .map(|t| t.text(self.text))
                    .collect();
                let (name, qualifier) = names.split_last()?;
                let (module, _) = self.find_type(node, &qualifier.join("."), name)?;
                Some(((module, (*name).to_owned()), Vec::new()))
            }
            _ => None,
        }
    }
}

fn listed(names: &[impl AsRef<str>]) -> String {
    if names.is_empty() {
        return "none".to_owned();
    }
    names
        .iter()
        .map(|n| format!("`{}`", n.as_ref()))
        .collect::<Vec<_>>()
        .join(", ")
}
