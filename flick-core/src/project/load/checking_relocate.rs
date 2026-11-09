/// Keep an inline namespace's type checking path, while placing its runtime
/// definitions in the parent file. This also keeps eager initialization in
/// the ordinary item dependency order of that file.
fn relocate_ir(module: &mut ir::Module, child: &str, parent: &str, local: &str) {
    let mut value = serde_json::to_value(&*module).expect("IR serializes");
    relocate_value(&mut value, child, parent, local);
    *module = serde_json::from_value(value).expect("relocated IR keeps its shape");
}

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

/// The reference model checks the same flattened runtime names as the IR.
/// This view is not published as the source module's `.iface`.
fn relocate_declarations(
    interface: &Interface,
    child: &str,
    parent: &str,
    local: &str,
) -> Interface {
    let mut own = interface.clone();
    own.instances.retain(|i| i.module == child);
    let mut value = serde_json::to_value(&own).expect("declarations serialize");
    relocate_decl_value(&mut value, child, parent, local);
    let mut declarations: Interface =
        serde_json::from_value(value).expect("declarations keep their shape");
    // Model method lookup uses the method's field name, while the runtime
    // value in the merged file has a prefixed symbol.
    let method_names: HashSet<String> = own
        .exports
        .iter()
        .filter_map(|e| e.definition.as_ref())
        .filter_map(|d| match &d.form {
            types::Form::Class { methods, .. } => Some(methods),
            _ => None,
        })
        .flat_map(|methods| methods.iter().cloned())
        .collect();
    let methods: Vec<_> = declarations
        .exports
        .iter()
        .filter(|e| e.entity.namespace == Namespace::Value)
        .cloned()
        .filter_map(|mut e| {
            let prefix = format!("{local}$");
            if !e.entity.name.starts_with(&prefix) {
                return None;
            }
            e.entity.name.replace_range(..prefix.len(), "");
            method_names.contains(&e.entity.name).then_some(e)
        })
        .collect();
    declarations.exports.extend(methods);
    declarations
}

fn relocate_decl_value(value: &mut serde_json::Value, child: &str, parent: &str, local: &str) {
    match value {
        serde_json::Value::Object(fields) => {
            relocate_name(fields, child, parent, local, false);
            if let Some(serde_json::Value::String(name)) = fields.get_mut("parent") {
                *name = format!("{local}${name}");
            }
            if let Some(serde_json::Value::Array(constructors)) = fields.get_mut("constructors") {
                for constructor in constructors {
                    if let Some(name) = constructor.as_str() {
                        *constructor = format!("{local}${name}").into();
                    }
                }
            }
            if let Some(serde_json::Value::Array(entries)) = fields.get_mut("entries") {
                for entry in entries {
                    if let serde_json::Value::Array(pair) = entry
                        && let Some(serde_json::Value::String(label)) = pair.first_mut()
                        && let Some(member) =
                            label.strip_prefix(child).and_then(|s| s.strip_prefix('.'))
                    {
                        *label = format!("{parent}.{local}${member}");
                    }
                }
            }
            for field in fields.values_mut() {
                relocate_decl_value(field, child, parent, local);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                relocate_decl_value(item, child, parent, local);
            }
        }
        _ => {}
    }
}
