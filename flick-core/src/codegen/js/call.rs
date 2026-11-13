//! Applications and the stages they run at: a pure call is a JS call, a
//! Sync one in a generator runs with the fiber's services, a runtime one
//! is `yield*` of its effect.

use flick_pretty::docvec;

use super::doc::{After, Before, Form, Head, Js, MULTILINE, YIELD_TEXT, YIELDS, call, object};
use super::{Gen, names};
use crate::ir::{Expr, ExprKind, Stage};
use flick_ir::{Dict, Rep};

impl<'d> Gen<'_, 'd> {
    pub(super) fn app(
        &mut self,
        e: &Expr,
        func: &Expr,
        arg: &Expr,
        stage: Stage,
        arg_first: bool,
    ) -> Js<'d> {
        let a = self.a;
        if let Some(value) = self.constructed(e) {
            return value;
        }
        if let Some((worker, args)) = self.worker_call(e) {
            let owner = self.owner.take();
            let js = self.ordered(&args);
            let js = self.before_sync(stage, owner.is_some(), js);
            let applied = call(a, worker, &js).marked_by(a, mark!(self, e.span));
            return self.staged(stage, owner, applied);
        }
        let owner = self.owner.take();
        let (f, x) = if arg_first {
            let x = self.expr(arg);
            let x = self.spill(x);
            (self.expr(func), x)
        } else {
            let js = self.ordered(&[func, arg]);
            (js[0], js[1])
        };
        let [f, x] = <[Js<'d>; 2]>::try_from(self.before_sync(stage, owner.is_some(), vec![f, x]))
            .expect("two operands");
        // `(x) => (y)` ends with `)` too, yet needs parentheses to be called
        let f = if f.atom().is_some() || f.form == Form::Call {
            f
        } else {
            let doc = docvec![a, "(", f.at(a, Before::Delimiter, After::Delimiter), ")"];
            Js::code(doc, f.after_paren(), &[f])
        };
        let applied = call(a, f, &[x]).call().marked_by(a, mark!(self, e.span));
        self.staged(stage, owner, applied)
    }

    /// The operands of a Sync call in a generator as constants when one
    /// yields: `$rt.sync` wraps the call in an arrow, where `yield*` is
    /// not allowed.
    fn before_sync(&mut self, stage: Stage, within: bool, js: Vec<Js<'d>>) -> Vec<Js<'d>> {
        let wrapped = if self.kernel {
            within
        } else {
            stage == Stage::Sync
        };
        if !wrapped || !self.generator || !js.iter().any(|j| j.flags & YIELD_TEXT != 0) {
            return js;
        }
        js.into_iter().map(|j| self.spill(j)).collect()
    }

    /// The JS call `applied` of `stage` made at the current stage: a Sync
    /// one in a generator runs with the fiber's services.
    fn staged(&mut self, stage: Stage, owner: Option<String>, applied: Js<'d>) -> Js<'d> {
        if self.kernel {
            return if stage == Stage::Pure {
                applied
            } else {
                self.run(owner, applied)
            };
        }
        match stage {
            Stage::Sync if self.generator => {
                let sync = self.rt("sync");
                let effect = call(self.a, sync, &[self.thunk(applied)]);
                self.yield_star(effect)
            }
            Stage::Pure | Stage::Sync => applied,
            Stage::Runtime => self.run(owner, applied),
        }
    }

    /// `(yield* effect)`.
    pub(super) fn yield_star(&self, effect: Js<'d>) -> Js<'d> {
        let a = self.a;
        let doc = docvec![a, "yield* ", effect.at(a, Before::Other, After::Delimiter)];
        // release runs a lone one without a generator
        let alone = effect.flags & (YIELDS | MULTILINE) == 0;
        Js {
            effect: alone.then(|| (effect.at(a, Before::Arrow, After::Delimiter), effect.flags)),
            ..Js::code(doc, Head::Ident, &[effect])
                .with(YIELD_TEXT | YIELDS)
                .in_parens()
        }
    }

    /// Runs the effect `js`, in `owner` when a `Within` gave one: in a
    /// generator with `yield*`, in a Sync stage at once with the services
    /// of the context.
    pub(super) fn run(&mut self, owner: Option<String>, js: Js<'d>) -> Js<'d> {
        if self.kernel {
            return self.kernel_run(owner, js);
        }
        let js = match owner {
            Some(owner) => {
                let within = self.rt("within");
                call(self.a, within, &[self.leaf(&owner), js])
            }
            None => js,
        };
        if self.generator {
            self.yield_star(js)
        } else {
            assert!(
                self.sync,
                "a runtime call runs inside a runtime or Sync stage"
            );
            let run_in = self.rt("runIn");
            call(self.a, run_in, &[js])
        }
    }

    /// The constant of `method` of `dict`, a canonical instance of this
    /// module whose method is emitted on its own.
    pub(super) fn direct_method(&mut self, dict: &Dict, method: &str) -> Option<Js<'d>> {
        let Dict::Instance { module, name, args } = dict else {
            return None;
        };
        if !args.is_empty() {
            return None;
        }
        if module != self.module {
            let constant = format!("{}${method}", names::instance(name));
            return self
                .options
                .is_direct(module, name, method)
                .then(|| self.global(module, &constant));
        }
        let constant = self.direct.get(&(name.clone(), method.to_owned()))?;
        Some(self.leaf(constant))
    }

    /// A known full application of a function of the module with a worker,
    /// its stages before the last pure and none evaluating its argument first.
    fn worker_call<'e>(&mut self, e: &'e Expr) -> Option<(Js<'d>, Vec<&'e Expr>)> {
        let mut args = Vec::new();
        let mut head = e;
        let mut last = true;
        while let ExprKind::App {
            func,
            arg,
            stage,
            arg_first: false,
        } = &head.kind
        {
            if *stage == Stage::Runtime && !last {
                return None;
            }
            last = false;
            args.push(&**arg);
            head = func;
        }
        let ExprKind::Global { module, name } = &head.kind else {
            return None;
        };
        args.reverse();
        if module != self.module {
            let arity = self
                .options
                .worker(module, name)
                .filter(|&n| n == args.len())?;
            let worker = self.global(module, &format!("{}${arity}", names::ident(name)));
            return Some((worker, args));
        }
        if let Some((arity, alias)) = self.direct_foreigns.get(name) {
            return (args.len() == *arity).then(|| (self.leaf(alias), args));
        }
        let (arity, _) = *self.workers.get(name)?;
        (args.len() == arity).then(|| {
            let worker = self.leaf(&format!("{}${arity}", names::ident(name)));
            (worker, args)
        })
    }

    /// Release: a tagged constructor applied to all its fields, as the
    /// object it builds, `{ _tag: "Rect#…", _0: 3, _1: 4 }`.
    fn constructed(&mut self, e: &Expr) -> Option<Js<'d>> {
        if !self.options.release {
            return None;
        }
        let mut args = Vec::new();
        let mut head = e;
        while let ExprKind::App {
            func,
            arg,
            stage: Stage::Pure,
            arg_first: false,
        } = &head.kind
        {
            args.push(&**arg);
            head = func;
        }
        let ExprKind::Con(c) = &head.kind else {
            return None;
        };
        if c.rep != Rep::Tagged
            || names::native(self.options.lang, c).is_some()
            || args.len() != c.arity
        {
            return None;
        }
        args.reverse();
        let js = self.ordered(&args);
        let package = self.options.package(&c.module);
        let tag = self.string(&names::tag(c, package));
        let a = self.a;
        let mut entries = vec![docvec![a, "_tag: ", tag.doc]];
        entries.extend(js.iter().enumerate().map(|(i, v)| {
            docvec![
                a,
                "_",
                i,
                ": ",
                v.at(a, Before::Delimiter, After::Delimiter)
            ]
        }));
        let object = object(a, &entries, &js);
        let object = if self.options.typescript() {
            Js::code(
                docvec![
                    a,
                    object.at(a, Before::Delimiter, After::Delimiter),
                    " as any"
                ],
                Head::Brace,
                &[object],
            )
            .in_parens()
        } else {
            object
        };
        Some(object.marked_by(a, mark!(self, e.span)))
    }
}
