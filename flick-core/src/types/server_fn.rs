//! `server fn name x = body`: a function its callers reach over the
//! network. The signature right above it is what the server runs,
//! `P -> … ->{r; e} S`. The name is the call a client makes,
//! `P -> … ->{; Rpc.ClientError | e} S`, and `server name` is the implementation
//! `Server.route` serves, `Server.Function {Server, r | t}`: it needs
//! `Server`, so a client entry cannot reach it (`FLICK-E009`). The payload
//! (the parameter, or a tuple of several), the success and each error cross
//! the network as JSON, so each needs a Schema witness, and the errors are
//! listed in the row.

use super::Checker;
use super::kinds::Kind;
use super::schemas::companion_name;
use super::ty::{Binder, Effects, Row, Type, error_label};
use super::typing::{Ref, SchemaItem, ServerFn, range};
use crate::diagnostics::codes;
use crate::iface::{Entity, Namespace};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

/// The member of the implementation's companion, `name$server`.
pub const IMPLEMENTATION: &str = "server";

/// What crosses the network, as the companions `name$payload`,
/// `name$success` and `name$error<i>` name their witnesses.
pub const PAYLOAD: &str = "payload";
pub const SUCCESS: &str = "success";

/// The row label of the error of a call `Server.authenticated` turns away.
const UNAUTHORIZED: &str = "Server.Unauthorized";

/// Whether a server function failing with `errors` is guarded.
fn guarded(errors: &[(String, Type)]) -> bool {
    errors.iter().any(|(l, _)| l == UNAUTHORIZED)
}

/// The witness member of the `i`-th error of a server function.
pub fn error_member(i: usize) -> String {
    format!("error{i}")
}

/// Whether `decl` is a `server fn`.
pub fn is_server_fn(decl: &Node) -> bool {
    decl.kind == K::LetDecl && decl.has_token(K::ServerKw)
}

/// The parts of a server function's signature: parameters, payload,
/// services, errors (each with its row label) and success.
struct Parts {
    params: Vec<Type>,
    payload: Type,
    services: Row,
    errors: Vec<(String, Type)>,
    success: Type,
}

fn parts(scheme: &Type) -> Result<Parts, &'static str> {
    if matches!(scheme, Type::Forall { .. }) {
        return Err("a server fn is not generic: its types cross the network");
    }
    let mut params = Vec::new();
    let mut ty = scheme;
    let (effects, result) = loop {
        let Type::Fun {
            param,
            effects,
            result,
        } = ty
        else {
            return Err("a server fn is a function: `P ->{r; e} S`");
        };
        params.push((**param).clone());
        match (effects, &**result) {
            (None, Type::Fun { .. }) => ty = result,
            (Some(_), Type::Fun { .. }) => {
                return Err("only the last arrow of a server fn runs: `A -> B ->{r; e} S`");
            }
            (None, _) => return Err("a server fn runs on the server: its arrow is `->{r; e}`"),
            (Some(effects), result) => break (effects, result),
        }
    };
    let (Type::Row(services), Type::Row(errors)) = (&effects.services, &effects.errors) else {
        return Err("a server fn lists its services and errors");
    };
    if errors.tail.is_some() {
        return Err("a server fn lists its errors: each crosses the network with its codec");
    }
    let payload = match params.as_slice() {
        [one] => one.clone(),
        _ => Type::Tuple(params.clone()),
    };
    Ok(Parts {
        params,
        payload,
        services: services.clone(),
        errors: errors.entries.clone(),
        success: result.clone(),
    })
}

/// The type of the client's call of a server function whose implementation
/// is `scheme`, and the type of `server name`.
fn call_types(scheme: &Type) -> Result<(Type, Type), &'static str> {
    let parts = parts(scheme)?;
    let client_error = Type::con("Rpc", "ClientError");
    let kind = if guarded(&parts.errors) {
        "Guarded"
    } else {
        "Function"
    };
    let mut errors = parts.errors;
    errors.extend(error_label(&client_error).map(|l| (l, client_error)));
    let mut params = parts.params.into_iter().rev();
    let last = params.next().unwrap_or(Type::Error);
    let ran = Type::Fun {
        param: Box::new(last),
        effects: Some(Box::new(Effects {
            services: Type::Row(Row::closed(Vec::new())),
            errors: Type::Row(Row::closed(errors)),
            sync: false,
        })),
        result: Box::new(parts.success),
    };
    let call = params.fold(ran, |result, param| Type::Fun {
        param: Box::new(param),
        effects: None,
        result: Box::new(result),
    });
    // open, so a list of them unifies to everything they need
    let mut services = parts.services.entries;
    services.retain(|(l, _)| l != "Server.Server");
    services.push(("Server.Server".to_owned(), Type::con("Server", "Server")));
    let mut row = Row::closed(services);
    row.tail = Some(Box::new(Type::Var("t".to_owned())));
    let implementation = Type::Forall {
        vars: vec![Binder {
            name: "t".to_owned(),
            kind: Kind::ServiceRow,
        }],
        constraints: Vec::new(),
        body: Box::new(Type::app(Type::con("Server", kind), Type::Row(row))),
    };
    Ok((call, implementation))
}

impl Checker<'_> {
    /// The types of the server function `name` declared with `scheme`, its
    /// signature: the call under `name`, the implementation under
    /// `name$server`.
    pub(super) fn declare_server_fn(&mut self, decl: &Node, name: &str, scheme: Option<&Type>) {
        let entity = self.env.entity(Namespace::Value, name);
        if !cfg!(feature = "server-stdlib") {
            self.error(
                codes::E012,
                self.span(decl),
                "`server fn` requires a compiler built with the `server-stdlib` feature",
            );
            self.env.set_value(entity, Type::Error);
            return;
        }
        let Some(scheme) = scheme else {
            self.env.set_value(entity, Type::Error);
            // a function without one has `FLICK-T004` already
            if !super::let_parts(decl, self.text).params.is_empty() {
                return;
            }
            self.error(
                codes::E010,
                self.span(decl),
                format!(
                    "server fn `{name}` needs a signature right above it: what the server runs"
                ),
            );
            return;
        };
        match call_types(scheme) {
            Ok((call, implementation)) => {
                self.env.set_value(entity, call);
                let companion = companion_name(name, IMPLEMENTATION);
                self.env.declare(Namespace::Value, &companion);
                let entity = self.env.entity(Namespace::Value, &companion);
                self.env.set_value(entity, implementation);
            }
            Err(why) => {
                self.error(
                    codes::E010,
                    self.span(decl),
                    format!("server fn `{name}`: {why}"),
                );
                self.env.set_value(entity, Type::Error);
            }
        }
    }

    /// The witnesses of what each server function sends, for lowering: a
    /// type without one is reported at the function.
    pub(super) fn server_fns(&mut self, decls: &[&Node]) {
        if !cfg!(feature = "server-stdlib") {
            return;
        }
        for decl in decls.iter().filter(|d| is_server_fn(d)) {
            let named = super::let_parts(decl, self.text);
            let Some(name) = named.name else {
                continue;
            };
            let Some(scheme) = self.signed_type(&name) else {
                continue;
            };
            let Ok(parts) = parts(&scheme) else {
                continue;
            };
            let params = parts.params.len();
            let is_guarded = guarded(&parts.errors);
            if named.params.len() != params {
                // its implementation takes the parameters as one payload
                self.error(
                    codes::E010,
                    self.span(decl),
                    format!("server fn `{name}` names each of its {params} parameters"),
                );
                continue;
            }
            let mut sent = vec![
                (PAYLOAD.to_owned(), "payload".to_owned(), parts.payload),
                (SUCCESS.to_owned(), "result".to_owned(), parts.success),
            ];
            sent.extend(parts.errors.into_iter().enumerate().map(|(i, (_, t))| {
                let what = format!("error `{}`", self.show(&t));
                (error_member(i), what, t)
            }));
            let errors = sent.len() - 2;
            let mut complete = true;
            for (member, what, ty) in sent {
                match self.plan_of(&ty, &member) {
                    Ok(plan) => self.typing.schemas.push(SchemaItem {
                        ty: companion_name(&name, &member),
                        decl: range(self.span(decl)),
                        witnesses: 0,
                        plan,
                        url: None,
                        form: None,
                    }),
                    Err((_, message)) => {
                        complete = false;
                        self.error(
                            codes::E010,
                            self.span(decl),
                            format!("server fn `{name}` sends its {what} as JSON, but {message}"),
                        );
                    }
                }
            }
            if complete {
                self.typing.server_fns.push(ServerFn {
                    decl: range(self.span(decl)),
                    name: name.clone(),
                    params,
                    errors,
                    guarded: is_guarded,
                });
            }
            // the implementation needs `Server`; the call does not
            let companion = companion_name(&name, IMPLEMENTATION);
            self.typing.servers.insert(companion, Vec::new());
        }
    }

    /// `server name`: the implementation of the server function `name`.
    pub fn server_expr(&mut self, node: &Node) -> Type {
        let span = self.span(node);
        let Some(name) = node.nodes().next() else {
            return Type::Error;
        };
        self.infer(name);
        let named = match self.typing.names.get(&name.range()) {
            Some(Ref::Value { module, name }) => Some((module.clone(), name.clone())),
            _ => None,
        };
        let Some((module, value)) = named else {
            return Type::Error;
        };
        let entity = Entity {
            module: module.clone(),
            namespace: Namespace::Value,
            name: companion_name(&value, IMPLEMENTATION),
        };
        let Some(scheme) = self.env.value(&entity).cloned() else {
            self.error(
                codes::E010,
                span,
                format!(
                    "`{value}` is not a server fn: only `server fn` has an implementation to serve"
                ),
            );
            return Type::Error;
        };
        self.record_name(
            span,
            Ref::Value {
                module,
                name: entity.name,
            },
        );
        self.instantiate(&scheme, span)
    }
}
