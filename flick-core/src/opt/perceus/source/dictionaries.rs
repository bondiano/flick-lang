//! Instance entries own values and cold getters, never their unfinished self.
use flick_ir::default_method;

use super::{
    Code, Dict, Env, Expr, ExprKind, InstanceInfo, Local, Lower, LowerError, Operation, Param, Pat,
    Span, Stage, abstract_params, dictionary, given, memory,
};

impl Lower<'_> {
    pub(super) fn instance(
        &mut self,
        info: &InstanceInfo,
        methods: &[(String, Expr)],
        arguments: &[Local],
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        if arguments.len() < info.context.len() {
            let code = self.reserve(span)?;
            let mut function = Code::new(vec![0, 1]);
            let mut inside = (0..arguments.len())
                .map(|field| function.emit(Operation::Project { object: 0, field }, span))
                .collect::<Result<Vec<_>, _>>()?;
            inside.push(1);
            let result = self.instance(info, methods, &inside, &mut function, span)?;
            self.finish(code, &function.finish(result))?;
            return block.emit(
                Operation::Closure {
                    code,
                    captures: arguments.to_vec(),
                },
                span,
            );
        }
        let env: Env = info
            .context
            .iter()
            .map(|name| given(name))
            .zip(arguments.iter().copied())
            .collect();
        let generated;
        let methods = if methods.is_empty()
            && let Some((fields, constructors)) = &info.derived
        {
            generated =
                super::derived::methods(&info.class.1, fields, constructors.as_deref(), span)?;
            &generated
        } else {
            methods
        };
        let mut entries = Vec::new();
        for (name, value) in methods {
            entries.push((
                name.clone(),
                memory::DictionaryEntry::Value(self.expr(value, &env, block)?),
            ));
        }
        for (name, own, extra) in &info.defaults {
            let body = default_body(info, name, *own, extra, span);
            let getter = self.lambda(&[self_param()], &body, &env, None, block, span)?;
            entries.push((name.clone(), memory::DictionaryEntry::Getter(getter)));
        }
        for (class, value) in &info.supers {
            let body = dictionary(value, span)?;
            let getter = self.lambda(&[self_param()], &body, &env, None, block, span)?;
            entries.push((format!("${class}"), memory::DictionaryEntry::Getter(getter)));
        }
        block.emit(Operation::Dictionary { entries }, span)
    }
}

const SELF: &str = "$memory$self";

fn self_param() -> Param {
    Param {
        pat: Pat::Bind(SELF.into()),
        stage: Stage::Pure,
        ty: None,
        binder_types: Vec::new(),
    }
}

fn default_body(info: &InstanceInfo, name: &str, own: usize, extra: &[Dict], span: Span) -> Expr {
    let value = Expr::new(
        ExprKind::Global {
            module: info.class.0.clone(),
            name: default_method(&info.class.1, name),
            type_args: Vec::new(),
        },
        span,
    );
    let own: Vec<_> = (0..own)
        .map(|index| format!("$memory$own{index}"))
        .collect();
    let dictionaries = [Dict::Field(SELF.into())]
        .into_iter()
        .chain(extra.iter().cloned())
        .chain(own.iter().cloned().map(Dict::Given))
        .collect();
    let body = Expr::new(ExprKind::Dicts(Box::new(value), dictionaries), span);
    if own.is_empty() {
        body
    } else {
        // Abstract dictionary arguments only. A value default remains a
        // value, rather than being eta-expanded over an ordinary argument.
        Expr::new(
            ExprKind::Lambda {
                params: abstract_params(&own),
                body: Box::new(body),
            },
            span,
        )
    }
}
