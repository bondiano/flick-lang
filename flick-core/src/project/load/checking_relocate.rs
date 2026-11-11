/// Keep an inline namespace's type checking path, while placing its runtime
/// definitions in the parent file. This also keeps eager initialization in
/// the ordinary item dependency order of that file.
fn relocate_ir(module: &mut ir::Module, child: &str, parent: &str, local: &str) {
    #[cfg(test)]
    let expected = relocated_as_json(module, child, parent, local);
    let to = Relocation { child, parent, local };
    for item in &mut module.items {
        match &mut item.kind {
            ir::ItemKind::Value { body, .. } | ir::ItemKind::Refined { check: body, .. } => {
                to.expr(body);
            }
            ir::ItemKind::Destructure { pattern, body } => {
                to.pattern(pattern);
                to.expr(body);
            }
            ir::ItemKind::Foreign(foreign) => {
                if let Some(catch) = &mut foreign.catch {
                    to.expr(catch);
                }
            }
            ir::ItemKind::Constructor(constructor) => to.constructor(constructor),
            ir::ItemKind::Instance { info, methods } => {
                to.instance(info);
                for (_, method) in methods {
                    to.expr(method);
                }
            }
            ir::ItemKind::Service { .. } => {}
        }
    }
    #[cfg(test)]
    assert_eq!(*module, expected, "the walk moves what the JSON form of it moved");
}

/// The names of the module `child` as the file of `parent` holds them,
/// under the prefix `local$`.
struct Relocation<'a> {
    child: &'a str,
    parent: &'a str,
    local: &'a str,
}

impl Relocation<'_> {
    /// Moves a top-level name of the child; whether it was one.
    fn name(&self, module: &mut String, name: &mut String) -> bool {
        let moved = module == self.child;
        if moved {
            self.parent.clone_into(module);
            *name = runtime_name(self.local, name);
        }
        moved
    }

    /// Moves `Child.member`, the key of an effect or of a service.
    fn key(&self, path: &mut String) {
        if let Some(member) = path.strip_prefix(self.child).and_then(|s| s.strip_prefix('.')) {
            *path = format!("{}.{}${member}", self.parent, self.local);
        }
    }

    fn constructor(&self, constructor: &mut types::Constructor) {
        if self.name(&mut constructor.module, &mut constructor.name) {
            constructor.ty = format!("{}${}", self.local, constructor.ty);
        }
    }

    fn dicts(&self, dicts: &mut [types::Dict]) {
        for dict in dicts {
            match dict {
                types::Dict::Instance { module, name, args } => {
                    self.name(module, name);
                    self.dicts(args);
                }
                types::Dict::Value { module, name } => {
                    self.name(module, name);
                }
                types::Dict::Super { dict, .. } => self.dicts(slice::from_mut(dict)),
                types::Dict::Structural { fields, .. } => self.dicts(fields),
                types::Dict::Given(_)
                | types::Dict::Opened { .. }
                | types::Dict::Field(_)
                | types::Dict::Primitive { .. }
                | types::Dict::Site { .. }
                | types::Dict::Error => {}
            }
        }
    }

    fn instance(&self, info: &mut types::InstanceInfo) {
        self.name(&mut info.module, &mut info.name);
        if info.class.0 == self.child {
            self.parent.clone_into(&mut info.class.0);
            info.class.1 = format!("{}${}", self.local, info.class.1);
        }
        for (_, dict) in &mut info.supers {
            self.dicts(slice::from_mut(dict));
        }
        if let Some((dicts, constructors)) = &mut info.derived {
            self.dicts(dicts);
            for constructor in constructors.iter_mut().flatten() {
                self.constructor(constructor);
            }
        }
        for (_, _, dicts) in &mut info.defaults {
            self.dicts(dicts);
        }
        if let Some(generic) = &mut info.generic {
            let types::GenericInfo { cases, inl, inr, con, field } = generic;
            let cases = cases.iter_mut().map(|(constructor, _)| constructor);
            for constructor in cases.chain([inl, inr, con, field]) {
                self.constructor(constructor);
            }
        }
    }

    fn pattern(&self, pattern: &mut ir::Pat) {
        match pattern {
            ir::Pat::Con(constructor, items) => {
                self.constructor(constructor);
                self.patterns(items);
            }
            ir::Pat::Tuple(items) | ir::Pat::Or(items) => self.patterns(items),
            ir::Pat::List(items, rest) => {
                self.patterns(items);
                if let Some(rest) = rest {
                    self.pattern(rest);
                }
            }
            ir::Pat::Record(fields) => {
                for (_, field) in fields {
                    self.pattern(field);
                }
            }
            ir::Pat::Wild | ir::Pat::Bind(_) | ir::Pat::Lit(_) => {}
        }
    }

    fn patterns(&self, patterns: &mut [ir::Pat]) {
        for pattern in patterns {
            self.pattern(pattern);
        }
    }

    fn arms(&self, arms: &mut [ir::Arm]) {
        for arm in arms {
            self.pattern(&mut arm.pat);
        }
    }

    fn expr(&self, expr: &mut ir::Expr) {
        match &mut expr.kind {
            ir::ExprKind::Global { module, name } => {
                self.name(module, name);
            }
            ir::ExprKind::Method { dict, .. } => self.dicts(slice::from_mut(dict)),
            ir::ExprKind::Con(constructor) => self.constructor(constructor),
            ir::ExprKind::Perform { effect: key, .. } | ir::ExprKind::ServiceTag(key) => {
                self.key(key);
            }
            ir::ExprKind::Dicts(_, dicts) => self.dicts(dicts),
            ir::ExprKind::Each { pat, .. } => self.pattern(pat),
            ir::ExprKind::Lambda { params, .. } => {
                for param in params {
                    self.pattern(&mut param.pat);
                }
            }
            ir::ExprKind::Block(statements, _) => {
                for statement in statements {
                    if let ir::Stmt::Let(pattern, _) = statement {
                        self.pattern(pattern);
                    }
                }
            }
            ir::ExprKind::Match(_, arms) => self.arms(arms),
            ir::ExprKind::Handle(handler) => {
                for handled in &mut handler.effects {
                    self.key(&mut handled.effect);
                }
                self.arms(&mut handler.errors);
            }
            _ => {}
        }
        expr.each_child_mut(&mut |child| self.expr(child));
    }
}

/// What `relocate_ir` gives, found the way it was first written: by the
/// shape of the IR's JSON. It holds the walk to every place of the IR that
/// names a module, one added later too.
#[cfg(test)]
fn relocated_as_json(module: &ir::Module, child: &str, parent: &str, local: &str) -> ir::Module {
    let mut value = serde_json::to_value(module).expect("IR serializes");
    relocate_value(&mut value, child, parent, local);
    serde_json::from_value(value).expect("relocated IR keeps its shape")
}

#[cfg(test)]
fn relocate_value(value: &mut serde_json::Value, child: &str, parent: &str, local: &str) {
    match value {
        serde_json::Value::Object(fields) => {
            let constructor = fields.contains_key("arity")
                && fields.contains_key("rep")
                && fields.contains_key("ty");
            let dictionary = fields.contains_key("args")
                && fields.contains_key("module")
                && fields.contains_key("name");
            let instance = fields.contains_key("context")
                && fields.contains_key("supers")
                && fields.contains_key("module");
            if constructor || dictionary || instance {
                relocate_name(fields, child, parent, local, constructor);
                if instance
                    && let Some(serde_json::Value::Array(class)) = fields.get_mut("class")
                    && class.first().and_then(serde_json::Value::as_str) == Some(child)
                    && let [module, name] = &mut class[..]
                {
                    *module = parent.into();
                    if let Some(raw) = name.as_str() {
                        *name = format!("{local}${raw}").into();
                    }
                }
            }
            for (key, field) in fields.iter_mut() {
                if matches!(key.as_str(), "effect" | "ServiceTag")
                    && let Some(path) = field.as_str()
                    && let Some(member) = path.strip_prefix(child).and_then(|s| s.strip_prefix('.'))
                {
                    *field = format!("{parent}.{local}${member}").into();
                }
                if matches!(key.as_str(), "Global" | "Value" | "Instance")
                    && let serde_json::Value::Object(reference) = field
                {
                    relocate_name(reference, child, parent, local, false);
                }
                relocate_value(field, child, parent, local);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                relocate_value(item, child, parent, local);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
fn relocate_name(
    fields: &mut serde_json::Map<String, serde_json::Value>,
    child: &str,
    parent: &str,
    local: &str,
    constructor: bool,
) {
    if fields.get("module").and_then(serde_json::Value::as_str) != Some(child) {
        return;
    }
    fields.insert("module".into(), parent.into());
    if let Some(name) = fields.get("name").and_then(serde_json::Value::as_str) {
        fields.insert("name".into(), runtime_name(local, name).into());
    }
    if constructor && let Some(ty) = fields.get("ty").and_then(serde_json::Value::as_str) {
        fields.insert("ty".into(), format!("{local}${ty}").into());
    }
}

fn rename_item(item: &mut ir::Item, local: &str) {
    match &mut item.kind {
        ir::ItemKind::Value { name, .. } | ir::ItemKind::Refined { name, .. } => {
            *name = runtime_name(local, name);
        }
        ir::ItemKind::Foreign(foreign) => {
            foreign.name = format!("{local}${}", foreign.name);
        }
        ir::ItemKind::Service { name, .. } => {
            *name = format!("{local}${name}");
        }
        ir::ItemKind::Destructure { pattern, .. } => rename_pattern(pattern, local),
        ir::ItemKind::Constructor(_) | ir::ItemKind::Instance { .. } => {}
    }
}

fn runtime_name(local: &str, name: &str) -> String {
    match name.strip_prefix("$default_") {
        Some(rest) => format!("$default_{local}${rest}"),
        None => format!("{local}${name}"),
    }
}

fn rename_pattern(pattern: &mut ir::Pat, local: &str) {
    match pattern {
        ir::Pat::Bind(name) => *name = format!("{local}${name}"),
        ir::Pat::Tuple(items) | ir::Pat::Or(items) | ir::Pat::Con(_, items) => {
            for item in items {
                rename_pattern(item, local);
            }
        }
        ir::Pat::List(items, rest) => {
            for item in items {
                rename_pattern(item, local);
            }
            if let Some(rest) = rest {
                rename_pattern(rest, local);
            }
        }
        ir::Pat::Record(fields) => {
            for (_, item) in fields {
                rename_pattern(item, local);
            }
        }
        ir::Pat::Wild | ir::Pat::Lit(_) => {}
    }
}
