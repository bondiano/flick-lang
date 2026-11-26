//! Calls of the host's global functions: the scalar slice of the value ABI.
//!
//! A `foreign pure … from global "a.b"` of pure stages is called directly
//! when each value it passes a Lua value holds exactly as Flick holds it:
//!
//! - to the host, a string, a boolean, an integer of at most 32 bits, a
//!   float, or a function of one pure stage, which the host calls with one
//!   argument and which gives it one result;
//! - from the host, a string, a boolean or a float, or nothing: a result
//!   of `Unit` or `Never` drops what the host gives, and the call is a
//!   statement. The host's first result is the value; a host may give more.
//!
//! The host is trusted to give what the declaration says, as on every
//! target. Everything else that crosses (`Option`, records, lists,
//! multiple results, methods, modules, `Sync` and effect modes) needs the
//! Lua value ABI and is refused.

use std::iter;

use flick_ir::{
    Binding, Expr as IrExpr, ExprKind as Ir, Foreign, ForeignMode, IrType, ItemKind, Lang, Span,
    Stage,
};

use super::super::ast::{Block, Expr, ExprKind, Function, LastKind, Name, Results, StmtKind};
use super::super::names::{KERNEL, MODULE, NAMESPACE, is_identifier};
use super::Lowering;
use super::ops::kernel_call;

/// What a value crossing to or from the host is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Host {
    Str,
    Bool,
    Int,
    Float,
    Unit,
    Never,
    /// A function of one pure stage.
    Callback,
}

/// The specifier of the JS globals of the kernel in `kernel.txt`.
const GLOBAL: &str = "global";

/// How a host function is called.
pub(super) struct HostFn<'f> {
    /// The global path, `string.gsub`.
    path: Vec<&'f str>,
    /// The arguments it takes; a function of `()` takes none.
    arity: usize,
    /// The Flick parameters a call takes: `()` for none.
    taken: usize,
    /// Whether its result is dropped.
    statement: bool,
    /// Whether its result is a string, which comes from the host.
    text_result: bool,
    /// The arguments that are callbacks given a string by the host.
    text_callbacks: Vec<usize>,
    /// The dictionaries a call takes before its arguments, of a kernel
    /// function.
    dicts: usize,
}

impl Lowering<'_> {
    /// What a scalar of type `ty` is.
    fn host_type(&self, ty: &IrType) -> Option<Host> {
        if let IrType::Fun { stage, .. } = ty {
            return (*stage == Stage::Pure).then_some(Host::Callback);
        }
        let (module, name) = ty.as_bare_con()?;
        let is = |lang| self.known.is(lang, module, name);
        let int = [
            Lang::Int,
            Lang::Int8,
            Lang::Int16,
            Lang::UInt8,
            Lang::UInt16,
            Lang::UInt32,
        ];
        if is(Lang::String) {
            Some(Host::Str)
        } else if is(Lang::Bool) {
            Some(Host::Bool)
        } else if int.into_iter().any(is) {
            Some(Host::Int)
        } else if is(Lang::Float) {
            Some(Host::Float)
        } else if is(Lang::Unit) {
            Some(Host::Unit)
        } else if is(Lang::Never) {
            Some(Host::Never)
        } else {
            None
        }
    }

    /// Whether a callback of type `ty` is one the host calls exactly: a
    /// value it gives in, a value it takes back.
    fn callback(&self, ty: &IrType) -> bool {
        let IrType::Fun { param, result, .. } = ty else {
            return false;
        };
        let given = self.host_type(param);
        let taken = self.host_type(result);
        matches!(
            given,
            Some(Host::Str | Host::Bool | Host::Float | Host::Unit)
        ) && matches!(
            taken,
            Some(Host::Str | Host::Bool | Host::Int | Host::Float | Host::Unit)
        )
    }

    /// How the foreign function `f` is called, or why it cannot be.
    pub(super) fn host_function<'f>(
        &self,
        module: &str,
        f: &'f Foreign,
    ) -> Result<HostFn<'f>, &'static str> {
        // a JS global of the kernel a built-in module binds is the Lua kernel's
        if let Binding::Global { name } = &f.binding
            && self.builtin.contains(module)
            && self.kernel.row(GLOBAL, name).is_some()
        {
            return self.kernel_function(f, GLOBAL, name);
        }
        let Binding::Global { name } = &f.binding else {
            return match &f.binding {
                Binding::Module { from, name } if from.starts_with("flick:") => {
                    self.kernel_function(f, from, name)
                }
                _ => Err(
                    "a foreign function of a module or of inline code needs the Lua value \
                          ABI, which is not implemented",
                ),
            };
        };
        if f.mode != ForeignMode::Pure {
            return Err(
                "a foreign function that is not `pure` runs in the Lua runtime, which is \
                        not implemented",
            );
        }
        let a = &f.adapter;
        let adapted = !a.optional.is_empty()
            || !a.owned.is_empty()
            || !a.words.is_empty()
            || a.result.nullable
            || a.callbacks.iter().any(|c| c.is_some_and(|n| n != 1));
        if f.dicts > 0 || f.catch.is_some() || adapted || f.stages.iter().any(|&r| r) {
            return Err(
                "its adapter, dictionaries or `catch` need the Lua value ABI, which is \
                        not implemented",
            );
        }
        let path: Vec<&str> = name.split('.').collect();
        if !path.iter().all(|s| is_identifier(s))
            || [MODULE, NAMESPACE, KERNEL, "_"].contains(&path[0])
        {
            return Err("its global path is not one generated Lua can name");
        }
        let Some(ty) = f.ty.as_deref() else {
            return Err("its type is not known, and the Lua value ABI is not implemented");
        };
        let (params, result) = ty.fun_stages();
        let taken = f.taken();
        if taken == 0 {
            return Err(
                "a foreign value, not a function, needs the Lua value ABI, which is not \
                        implemented",
            );
        }
        if params.len() != taken || params.iter().any(|(_, s)| *s != Stage::Pure) {
            return Err("its stages need the Lua runtime, which is not implemented");
        }
        let mut text_callbacks = Vec::new();
        for (i, (param, _)) in params[..f.arity].iter().enumerate() {
            match self.host_type(param) {
                Some(Host::Str | Host::Bool | Host::Int | Host::Float) => {}
                Some(Host::Callback) if self.callback(param) => {
                    if let IrType::Fun { param, .. } = &***param
                        && self.host_type(param) == Some(Host::Str)
                    {
                        text_callbacks.push(i);
                    }
                }
                _ => return Err(ABI_ARGS),
            }
        }
        let host = self.host_type(result);
        let statement = match host {
            Some(Host::Unit | Host::Never) => true,
            Some(Host::Str | Host::Bool | Host::Float) => false,
            _ => return Err(ABI_RESULT),
        };
        Ok(HostFn {
            path,
            arity: f.arity,
            taken,
            statement,
            text_result: host == Some(Host::Str),
            text_callbacks,
            dicts: 0,
        })
    }

    /// How a binding of Flick's own kernel is called: `K.<export>` with
    /// every Flick argument at once, in the values generated code holds, as
    /// the table `kernel.txt` implements it.
    fn kernel_function<'f>(
        &self,
        f: &'f Foreign,
        from: &str,
        name: &'f str,
    ) -> Result<HostFn<'f>, &'static str> {
        if !self.kernel.row(from, name).is_some_and(|r| r.implemented) {
            return Err("the Lua kernel does not implement it");
        }
        // every argument at once, whatever JS's `arity` splits them into
        let params = (f.ty.as_deref()).map(|ty| ty.fun_stages().0);
        let taken = params.as_ref().map_or(0, Vec::len);
        let pure = params.is_some_and(|ps| ps.iter().all(|(_, s)| *s == Stage::Pure));
        if f.mode != ForeignMode::Pure || taken == 0 || !pure {
            return Err("the Lua kernel implements it only as a pure function of its arguments");
        }
        Ok(HostFn {
            path: iter::once(KERNEL).chain(name.split('.')).collect(),
            arity: f.dicts + taken,
            taken,
            statement: false,
            text_result: false,
            text_callbacks: Vec::new(),
            dicts: f.dicts,
        })
    }

    /// The call of `host` with `args`, the first `arity` of the Flick ones;
    /// a dropped result is a statement, and the value is Unit. A string the
    /// host gives, as a result or to a callback, is made WTF-8 first: its
    /// ill-formed UTF-8 as U+FFFD.
    fn host_call(&mut self, host: &HostFn<'_>, args: Vec<Expr>, span: Span) -> Expr {
        let mut callee = Expr::name(&Name::new(host.path[0].to_owned()), span);
        for segment in &host.path[1..] {
            callee = callee.field(segment);
        }
        let mut args: Vec<Expr> = args.into_iter().take(host.arity).collect();
        for &i in &host.text_callbacks {
            let callback = self.local("callback", args[i].clone());
            let text = self.names.fresh("text");
            let given = kernel_call("from_host", vec![Expr::name(&text, span)], span);
            let mut body = Block::default();
            body.end(
                LastKind::Return(Expr::name(&callback, span).call(vec![given])),
                span,
            );
            let wrapped = Function {
                params: vec![text],
                body,
                span,
            };
            args[i] = Expr::new(ExprKind::Function(Box::new(wrapped)), span);
        }
        let call = Expr::new(ExprKind::Call(Box::new(callee), args, Results::Any), span);
        if host.statement {
            self.push(StmtKind::Call(call), span);
            return Self::unit(span);
        }
        if host.text_result {
            return kernel_call("from_host", vec![call], span);
        }
        call
    }

    /// `f a b` of a foreign function given all its arguments: the host's
    /// function called at once.
    pub(super) fn host_application(&mut self, e: &IrExpr) -> Option<Expr> {
        let mut args = Vec::new();
        let mut head = e;
        while let Ir::App {
            func,
            arg,
            stage: Stage::Pure,
            arg_first: false,
        } = &head.kind
        {
            args.push(&**arg);
            head = func;
        }
        // a kernel function's dictionaries come first
        let (head, dicts) = match &head.kind {
            Ir::Dicts(inner, dicts) => (&**inner, dicts.as_slice()),
            _ => (head, &[][..]),
        };
        let Ir::Global { module, name, .. } = &head.kind else {
            return None;
        };
        let item = self.items.get(&(module.as_str(), name.as_str())).copied();
        let Some(ItemKind::Foreign(f)) = item else {
            return None;
        };
        let host = self.host_function(module, f).ok()?;
        if args.len() != host.taken || dicts.len() != host.dicts {
            return None;
        }
        args.reverse();
        let mut values: Vec<Expr> = dicts.iter().map(|d| self.dict(d, e.span)).collect();
        values.extend(self.ordered(&args));
        Some(self.host_call(&host, values, e.span))
    }

    /// A foreign function as a value: a curried function calling the host,
    /// after a function of a kernel function's dictionaries.
    pub(super) fn host_value(&mut self, host: &HostFn<'_>, span: Span) -> Expr {
        let dicts: Vec<Name> = (0..host.dicts).map(|_| self.names.fresh("dict")).collect();
        let params: Vec<Name> = (0..host.taken).map(|_| self.names.fresh("x")).collect();
        let args = (dicts.iter().chain(&params))
            .map(|p| Expr::name(p, span))
            .collect();
        let body = self.function_body(|g| {
            let value = g.host_call(host, args, span);
            g.block.end(LastKind::Return(value), span);
        });
        let mut f = Function {
            params: vec![params[params.len() - 1].clone()],
            body,
            span,
        };
        for param in params[..params.len() - 1].iter().rev() {
            let mut outer = Block::default();
            outer.end(
                LastKind::Return(Expr::new(ExprKind::Function(Box::new(f)), span)),
                span,
            );
            f = Function {
                params: vec![param.clone()],
                body: outer,
                span,
            };
        }
        if !dicts.is_empty() {
            let mut outer = Block::default();
            outer.end(
                LastKind::Return(Expr::new(ExprKind::Function(Box::new(f)), span)),
                span,
            );
            f = Function {
                params: dicts,
                body: outer,
                span,
            };
        }
        Expr::new(ExprKind::Function(Box::new(f)), span)
    }
}

const ABI_ARGS: &str = "an argument other than a string, a boolean, a number or a callback of \
                        those needs the Lua value ABI, which is not implemented";
const ABI_RESULT: &str = "a result other than a string, a boolean, a float or `Unit` needs the \
                          Lua value ABI, which is not implemented";
