//! the bridge to Effect in compiled code. A foreign binding the kernel
//! has no export for is one of Effect, and what crosses it is converted by its
//! shape, from the signature. Emitted only for Flick's own runtime.

use crate::effects::bindings;
use flick_ir::Shape;

use super::Gen;
use super::doc::{After, Before, Head, Js, array, call, object};
use crate::ir::{Foreign, ForeignMode};
use flick_pretty::docvec;

/// which way a value crosses the bridge to Effect.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Toward {
    /// From Flick to the Effect side.
    Host,
    /// From the Effect side to Flick.
    Flick,
}

impl Toward {
    fn back(self) -> Self {
        match self {
            Toward::Host => Toward::Flick,
            Toward::Flick => Toward::Host,
        }
    }
}

impl<'d> Gen<'_, 'd> {
    fn bridge(&mut self, name: &str) -> Js<'d> {
        let alias = format!("$bridge_{name}");
        if !self
            .foreign
            .iter()
            .any(|(source, export, _)| source == bindings::ADAPTER_BRIDGE && export == name)
        {
            self.foreign.push((
                bindings::ADAPTER_BRIDGE.to_owned(),
                name.to_owned(),
                alias.clone(),
            ));
        }
        self.leaf(&alias)
    }
    pub(super) fn bridged_body(
        &mut self,
        f: &Foreign,
        runtime: bool,
        called: Js<'d>,
        copy_result: bool,
        catch: Option<Js<'d>>,
    ) -> Option<Js<'d>> {
        let a = self.a;
        let shape = &f.adapter.result_shape;
        Some(match (f.mode, runtime) {
            (ForeignMode::Pure, _) | (_, false) => {
                let context = self.bridge("inSyncContext");
                let called = call(a, context, &[self.thunk(called)]);
                let value = self.foreign_result(f, copy_result, called);
                self.crossed(Toward::Flick, shape, value)
            }
            (ForeignMode::Effect, true) => {
                let from = self.bridge("fromEffect");
                let ran = call(a, from, &[called]);
                let ran = match catch {
                    Some(c) => {
                        let foreign = self.rt("foreignEffect");
                        call(a, foreign, &[self.thunk(ran), c])
                    }
                    None => ran,
                };
                let value = self.foreign_result(f, copy_result, self.leaf("$v"));
                let value = self.crossed(Toward::Flick, shape, value);
                self.mapped(ran, "$v", value)
            }
            (ForeignMode::Sync, true) => {
                let context = self.bridge("inSyncContext");
                let called = call(a, context, &[self.thunk(called)]);
                let value = self.foreign_result(f, copy_result, called);
                let value = self.crossed(Toward::Flick, shape, value);
                match catch {
                    Some(c) => {
                        let foreign = self.rt("foreignSync");
                        call(a, foreign, &[self.thunk(value), c])
                    }
                    None => value,
                }
            }
            (ForeignMode::Async, true) => return None,
        })
    }
    pub(super) fn crossed(&mut self, toward: Toward, shape: &Shape, value: Js<'d>) -> Js<'d> {
        let a = self.a;
        match shape {
            Shape::Data => {
                let convert = self.bridge(match toward {
                    Toward::Host => "dataToEffect",
                    Toward::Flick => "dataFromEffect",
                });
                call(a, convert, &[value])
            }
            Shape::Fun {
                args,
                result,
                runtime,
                sync,
            } => self.crossed_fun(toward, args, result, (*runtime, *sync), value),
            Shape::Eff(inner) => self.crossed_eff(toward, inner, value),
            Shape::List(inner) => {
                let depth = self.bridge_depth();
                let param = format!("$e{depth}");
                let item = self.crossed(toward, inner, self.leaf(&param));
                let map = self.member(value.in_parens(), "map");
                call(a, map, &[self.lambda_of(&param, item)])
            }
            Shape::Option(inner) => {
                let depth = self.bridge_depth();
                let param = format!("$e{depth}");
                let item = self.crossed(toward, inner, self.leaf(&param));
                let convert = self.bridge(match toward {
                    Toward::Host => "optionToEffect",
                    Toward::Flick => "optionFromEffect",
                });
                call(a, convert, &[value, self.lambda_of(&param, item)])
            }
            Shape::Result(error, success) => {
                let depth = self.bridge_depth();
                let ep = format!("$e{depth}");
                let ap = format!("$a{depth}");
                let error = self.crossed(toward, error, self.leaf(&ep));
                let success = self.crossed(toward, success, self.leaf(&ap));
                let convert = self.bridge(match toward {
                    Toward::Host => "resultToEffect",
                    Toward::Flick => "resultFromEffect",
                });
                call(
                    a,
                    convert,
                    &[
                        value,
                        self.lambda_of(&ap, success),
                        self.lambda_of(&ep, error),
                    ],
                )
            }
            Shape::Tuple(items) => {
                let depth = self.bridge_depth();
                let param = format!("$t{depth}");
                let parts: Vec<_> = items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| {
                        self.crossed(toward, item, self.leaf(&format!("{param}[{i}]")))
                    })
                    .collect();
                call(
                    a,
                    self.lambda_of(&param, array(a, &parts)).in_parens(),
                    &[value],
                )
            }
            Shape::Record(fields) => self.crossed_record(toward, fields, value),
            _ => self.crossed_opaque(toward, shape, value),
        }
    }
    fn crossed_record(
        &mut self,
        toward: Toward,
        fields: &[(String, Shape)],
        value: Js<'d>,
    ) -> Js<'d> {
        let a = self.a;
        let depth = self.bridge_depth();
        let param = format!("$t{depth}");
        let mut entries = Vec::new();
        let mut parts = Vec::new();
        for (name, field) in fields.iter().filter(|(_, field)| !field.is_data()) {
            let name = self.string(name);
            let accessed = Js::code(
                docvec![a, a.alloc_str(&param), "[", name.doc, "]"],
                Head::Ident,
                &[],
            );
            let field = self.crossed(toward, field, accessed);
            entries.push(docvec![
                a,
                "[",
                name.doc,
                "]: ",
                field.at(a, Before::Delimiter, After::Delimiter)
            ]);
            parts.push(field);
        }
        let data = self.crossed(toward, &Shape::Data, self.leaf(&param));
        entries.insert(
            0,
            docvec![a, "...", data.at(a, Before::Other, After::Delimiter)],
        );
        parts.push(data);
        let body = object(a, &entries, &parts).in_parens();
        call(a, self.lambda_of(&param, body).in_parens(), &[value])
    }

    fn crossed_opaque(&mut self, toward: Toward, shape: &Shape, value: Js<'d>) -> Js<'d> {
        let convert = match (toward, shape) {
            (Toward::Host, Shape::Layer) => self.bridge("layerToEffect"),
            (Toward::Host, Shape::Schema) => self.bridge("schemaToEffect"),
            (Toward::Host, Shape::Codec) => self.bridge("codecToEffect"),
            (Toward::Host, Shape::Fiber) => self.bridge("fiberToEffect"),
            (Toward::Host, Shape::Duration) => self.bridge("durationToEffect"),
            (Toward::Host, Shape::Schedule) => self.bridge("scheduleToEffect"),
            (Toward::Host, Shape::Chunk) => self.bridge("chunkToEffect"),
            (Toward::Host, Shape::HashMap) => self.bridge("hashMapToEffect"),
            (Toward::Host, Shape::HashSet) => self.bridge("hashSetToEffect"),
            (Toward::Host, Shape::Handlers) => self.bridge("handlersToEffect"),
            (Toward::Host, Shape::Exit) => self.bridge("exitToEffect"),
            (Toward::Host, _) => self.bridge("failureToEffect"),
            (Toward::Flick, Shape::Exit) => self.bridge("exitFromEffect"),
            (Toward::Flick, Shape::Failure) => self.bridge("failureFromEffect"),
            (Toward::Flick, Shape::Layer) => self.bridge("layerFromEffect"),
            (Toward::Flick, Shape::Schema) => self.bridge("schemaFromEffect"),
            (Toward::Flick, Shape::Codec) => self.bridge("codecFromEffect"),
            (Toward::Flick, Shape::Fiber) => self.bridge("fiberFromEffect"),
            (Toward::Flick, Shape::Duration) => self.bridge("durationFromEffect"),
            (Toward::Flick, Shape::Schedule) => self.bridge("scheduleFromEffect"),
            (Toward::Flick, Shape::Chunk) => self.bridge("chunkFromEffect"),
            (Toward::Flick, Shape::HashMap) => self.bridge("hashMapFromEffect"),
            (Toward::Flick, Shape::HashSet) => self.bridge("hashSetFromEffect"),
            // a record of handlers crosses unchanged
            (Toward::Flick, _) => return value,
        };
        call(self.a, convert, &[value])
    }
    fn crossed_fun(
        &mut self,
        toward: Toward,
        args: &[Shape],
        result: &Shape,
        (runtime, sync): (bool, bool),
        function: Js<'d>,
    ) -> Js<'d> {
        let a = self.a;
        let depth = self.bridge_depth();
        let prefix = match toward {
            Toward::Host => "h",
            Toward::Flick => "g",
        };
        let params: Vec<_> = (0..args.len())
            .map(|k| format!("${prefix}{depth}_{k}"))
            .collect();
        let mut called = function;
        for (param, arg) in params.iter().zip(args) {
            let arg = self.crossed(toward.back(), arg, self.leaf(param));
            called = call(a, called, &[arg]);
        }
        let at = format!("$r{depth}");
        let body = match (toward, sync, runtime) {
            (Toward::Host, true, _) => {
                let value = self.crossed(toward, result, self.leaf(&at));
                let bridge = self.bridge("callSync");
                let ran = call(a, bridge, &[self.thunk(called)]);
                if value.atom() == Some(at.as_str()) {
                    ran
                } else {
                    call(a, self.lambda_of(&at, value).in_parens(), &[ran])
                }
            }
            (Toward::Host, false, true) => {
                let value = self.crossed(toward, result, self.leaf(&at));
                let ran = self.mapped(called, &at, value);
                let bridge = self.bridge("callEffect");
                call(a, bridge, &[self.thunk(ran)])
            }
            (Toward::Flick, true, _) => {
                let bridge = self.bridge("inSyncContext");
                let ran = call(a, bridge, &[self.thunk(called)]);
                self.crossed(toward, result, ran)
            }
            (Toward::Flick, false, true) => {
                let value = self.crossed(toward, result, self.leaf(&at));
                let bridge = self.bridge("fromEffect");
                let ran = call(a, bridge, &[called]);
                self.mapped(ran, &at, value)
            }
            (_, false, false) => self.crossed(toward, result, called),
        };
        params
            .iter()
            .rev()
            .fold(body, |body, param| self.lambda_of(param, body))
    }
    fn crossed_eff(&mut self, toward: Toward, inner: &Shape, eff: Js<'d>) -> Js<'d> {
        let a = self.a;
        let depth = self.bridge_depth();
        let at = format!("$r{depth}");
        let value = self.crossed(toward, inner, self.leaf(&at));
        match toward {
            Toward::Host => {
                let ran = self.mapped(call(a, eff, &[]), &at, value);
                let bridge = self.bridge("callEffect");
                call(a, bridge, &[self.thunk(ran)])
            }
            Toward::Flick => {
                let bridge = self.bridge("fromEffect");
                let ran = call(a, bridge, &[eff]);
                let mapped = self.mapped(ran, &at, value);
                self.thunk(mapped)
            }
        }
    }
    fn mapped(&mut self, ran: Js<'d>, at: &str, value: Js<'d>) -> Js<'d> {
        if value.atom() == Some(at) {
            ran
        } else {
            let mapped = self.rt("mapped");
            call(self.a, mapped, &[ran, self.lambda_of(at, value)])
        }
    }
    fn bridge_depth(&mut self) -> String {
        let temp = self.temp();
        temp.trim_start_matches('$').to_owned()
    }
}
