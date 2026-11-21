//! Counted `JsonSchema` dialect conversion entrypoints and `OpenAPI` 3.1 helpers.

use std::collections::{BTreeMap, BTreeSet};

use flick_core::ir::Lit;
use flick_core::ir::memory::JsonSchemaIntrinsic;

use super::json_pointer_ops::{format_uri_fragment, parse_uri_fragment};
use super::json_schema_conv::{Adapter, Converter};
use super::json_schema_to::convert_document;
use super::list_ops::slice_reps;
use super::record_ops::put_js_key;
use super::{Data, Error, Heap, Value, charge};

pub(super) const META_DRAFT_04: &str = "http://json-schema.org/draft-04/schema#";
pub(super) const META_DRAFT_07: &str = "http://json-schema.org/draft-07/schema#";
pub(super) const META_DRAFT_2020_12: &str = "https://json-schema.org/draft/2020-12/schema";
const META_OPENAPI_3_1: &str = "https://spec.openapis.org/oas/3.1/dialect/base";
const OPENAPI_31_COLLISIONS: &[&str] = &["example", "discriminator", "xml", "externalDocs"];

#[derive(Clone)]
pub(super) enum JNode {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<JNode>),
    Obj(Vec<(String, JNode)>),
}

impl JNode {
    pub(super) fn obj() -> Self {
        Self::Obj(Vec::new())
    }
    pub(super) fn get(&self, key: &str) -> Option<&JNode> {
        let Self::Obj(fs) = self else { return None };
        fs.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
    pub(super) fn has(&self, key: &str) -> bool {
        self.get(key).is_some()
    }
    pub(super) fn put(&mut self, key: impl Into<String>, val: JNode) {
        let k = key.into();
        if let Self::Obj(fs) = self {
            if let Some((_, slot)) = fs.iter_mut().find(|(ek, _)| ek == &k) {
                *slot = val;
            } else {
                fs.push((k, val));
            }
        }
    }
    pub(super) fn remove(&mut self, key: &str) {
        if let Self::Obj(fs) = self
            && let Some(i) = fs.iter().position(|(k, _)| k == key)
        {
            fs.remove(i);
        }
    }
    pub(super) fn as_str(&self) -> Option<&str> {
        if let Self::Str(s) = self {
            Some(s)
        } else {
            None
        }
    }
    pub(super) fn as_obj(&self) -> Option<&[(String, JNode)]> {
        if let Self::Obj(fs) = self {
            Some(fs)
        } else {
            None
        }
    }
}

pub(super) fn run(
    heap: &mut Heap,
    op: JsonSchemaIntrinsic,
    args: &[Value],
    fuel: &mut u64,
) -> Result<Value, Error> {
    charge(fuel, 1)?;
    let input = read_node(heap, &args[0], fuel)?;
    let out = match op {
        JsonSchemaIntrinsic::FromDraft202012 => from_draft_2020_12(input),
        JsonSchemaIntrinsic::FromDraft07 => {
            let mut conv = Converter::new(Adapter::Draft07To2020, true, true, false);
            from_draft_2020_12(conv.convert_root(input)?)
        }
        JsonSchemaIntrinsic::FromOpenApi30 => {
            let mut conv = Converter::new(Adapter::OpenApi30To2020, false, false, false);
            from_draft_2020_12(conv.convert_root(input)?)
        }
        JsonSchemaIntrinsic::FromOpenApi31 => from_openapi_3_1(input),
        JsonSchemaIntrinsic::ToDraft07 => {
            convert_document(&input, "draft-07", Adapter::ToDraft07, false)?
        }
        JsonSchemaIntrinsic::ToDraft04 => {
            convert_document(&input, "draft-04", Adapter::ToDraft04, true)?
        }
        JsonSchemaIntrinsic::ToOpenApi31 => to_openapi_3_1(&input)?,
    };
    write_node(heap, &out, fuel)
}

fn read_node(heap: &Heap, val: &Value, fuel: &mut u64) -> Result<JNode, Error> {
    charge(fuel, 1)?;
    match val {
        Value::Bool(b) => Ok(JNode::Bool(*b)),
        Value::Int(i) => Ok(JNode::Num(f64::from(*i))),
        Value::Float(f) => Ok(JNode::Num(*f)),
        Value::Object(tok) => match &heap.object(tok.id)?.data {
            Data::String(s) => Ok(JNode::Str(s.clone())),
            Data::Utf16(u) => Ok(JNode::Str(String::from_utf16_lossy(u))),
            Data::Tagged { tag, fields, .. } if tag == "$list" => {
                let mut out = Vec::with_capacity(fields.len());
                for f in fields {
                    out.push(read_node(heap, f, fuel)?);
                }
                Ok(JNode::Arr(out))
            }
            Data::ListSlice { .. } => {
                let reps = slice_reps(heap, val)?;
                let mut out = Vec::with_capacity(reps.len());
                for r in &reps {
                    out.push(read_node(heap, r, fuel)?);
                }
                Ok(JNode::Arr(out))
            }
            Data::Record(fields) => {
                let mut out = Vec::with_capacity(fields.len());
                for (k, v) in fields {
                    out.push((k.clone(), read_node(heap, v, fuel)?));
                }
                Ok(JNode::Obj(out))
            }
            _ => Ok(JNode::Null),
        },
        _ => Ok(JNode::Null),
    }
}

fn write_node(heap: &mut Heap, node: &JNode, fuel: &mut u64) -> Result<Value, Error> {
    charge(fuel, 1)?;
    match node {
        JNode::Null => Ok(heap.allocate(Data::JsonNull)),
        JNode::Bool(b) => Ok(Value::Bool(*b)),
        JNode::Num(f) => Ok(Value::Float(*f)),
        JNode::Str(s) => heap.literal(&Lit::String(s.clone())),
        JNode::Arr(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match write_node(heap, item, fuel) {
                    Ok(v) => out.push(v),
                    Err(err) => {
                        heap.drop_values(out)?;
                        return Err(err);
                    }
                }
            }
            heap.construct("$list", out)
        }
        JNode::Obj(entries) => {
            let mut fields: Vec<(String, Value)> = Vec::with_capacity(entries.len());
            for (k, v) in entries {
                match write_node(heap, v, fuel) {
                    Ok(val) => {
                        if let Some(old) = put_js_key(&mut fields, k.clone(), val) {
                            heap.drop_value(old)?;
                        }
                    }
                    Err(err) => {
                        heap.drop_values(fields.into_iter().map(|(_, x)| x).collect())?;
                        return Err(err);
                    }
                }
            }
            heap.record(fields)
        }
    }
}

pub(super) fn unsupported(keyword: &str, dialect: &str, details: &str) -> Error {
    Error::RangeError(format!(
        "Error: Cannot convert JSON Schema keyword \"{keyword}\" to {dialect}: {details}"
    ))
}

pub(super) fn reject_collisions(
    src: &JNode,
    keys: &[&str],
    target: &str,
    from: &str,
) -> Result<(), Error> {
    for &k in keys {
        if src.has(k) {
            return Err(unsupported(
                k,
                target,
                &format!("it is not active in {from} but would become active in the target"),
            ));
        }
    }
    Ok(())
}

pub(super) fn is_meta_uri(v: &JNode, uri: &str) -> bool {
    let Some(s) = v.as_str() else { return false };
    if let Some(stripped) = uri.strip_suffix('#') {
        s == uri || s == stripped
    } else {
        s == uri || s == format!("{uri}#")
    }
}

pub(super) fn creates_resource(v: Option<&JNode>) -> bool {
    v.and_then(JNode::as_str)
        .is_some_and(|s| !s.is_empty() && !s.starts_with('#'))
}

pub(super) fn is_valid_anchor(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes
        .first()
        .is_some_and(|&b| b.is_ascii_alphabetic() || b == b'_')
        && bytes[1..]
            .iter()
            .all(|&b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_'))
}

pub(super) fn rewrite_openapi_ref(r: &str) -> String {
    if !r.starts_with('#') {
        return r.to_owned();
    }
    if let Some(path) = parse_uri_fragment(r)
        && path.len() >= 2
        && path[0] == "components"
        && path[1] == "schemas"
    {
        let mut next = vec!["$defs".to_owned()];
        next.extend_from_slice(&path[2..]);
        return format_uri_fragment(&next);
    }
    r.to_owned()
}

pub(super) fn make_doc(dialect: &str, root: JNode, defs: JNode) -> JNode {
    let mut doc = JNode::obj();
    doc.put("definitions", defs);
    doc.put("dialect", JNode::Str(dialect.into()));
    doc.put("root", root);
    doc
}

fn from_draft_2020_12(js: JNode) -> JNode {
    let (mut schema, mut defs) = (JNode::obj(), JNode::obj());
    if let JNode::Obj(entries) = js {
        let mut kept = Vec::new();
        for (k, v) in entries {
            if k == "$defs" {
                if matches!(v, JNode::Obj(_)) {
                    defs = v;
                }
            } else {
                kept.push((k, v));
            }
        }
        schema = JNode::Obj(kept);
    }
    make_doc("draft-2020-12", schema, defs)
}

fn transform_schema(
    node: JNode,
    f: &mut impl FnMut(&mut JNode, bool) -> Result<(), Error>,
) -> Result<JNode, Error> {
    fn walk(
        node: JNode,
        inherited: bool,
        is_root: bool,
        f: &mut impl FnMut(&mut JNode, bool) -> Result<(), Error>,
    ) -> Result<JNode, Error> {
        let JNode::Obj(entries) = node else {
            return Ok(node);
        };
        let has_id = entries
            .iter()
            .find(|(k, _)| k == "$id")
            .is_some_and(|(_, v)| creates_resource(Some(v)));
        let in_embedded = inherited || (!is_root && has_id);
        let mut out = Vec::with_capacity(entries.len());
        for (k, v) in entries {
            let next = match k.as_str() {
                "$defs" | "properties" | "patternProperties" | "dependentSchemas" => {
                    if let JNode::Obj(m) = v {
                        let mut nm = Vec::with_capacity(m.len());
                        for (mk, mv) in m {
                            nm.push((mk, walk(mv, in_embedded, false, f)?));
                        }
                        JNode::Obj(nm)
                    } else {
                        v
                    }
                }
                "allOf" | "anyOf" | "oneOf" | "prefixItems" => {
                    if let JNode::Arr(xs) = v {
                        let mut nxs = Vec::with_capacity(xs.len());
                        for x in xs {
                            nxs.push(walk(x, in_embedded, false, f)?);
                        }
                        JNode::Arr(nxs)
                    } else {
                        v
                    }
                }
                "not"
                | "additionalProperties"
                | "propertyNames"
                | "unevaluatedProperties"
                | "items"
                | "contains"
                | "unevaluatedItems"
                | "if"
                | "then"
                | "else"
                | "contentSchema" => walk(v, in_embedded, false, f)?,
                _ => v,
            };
            out.push((k, next));
        }
        let mut res = JNode::Obj(out);
        f(&mut res, in_embedded)?;
        Ok(res)
    }
    walk(node, false, true, f)
}

fn from_openapi_3_1(js: JNode) -> JNode {
    let root_id = creates_resource(js.get("$id"));
    let transformed = transform_schema(js, &mut |s, in_embedded| {
        if !root_id && !in_embedded {
            for kw in ["$ref", "$dynamicRef"] {
                if let Some(r) = s.get(kw).and_then(JNode::as_str).map(ToOwned::to_owned) {
                    s.put(kw, JNode::Str(rewrite_openapi_ref(&r)));
                }
            }
        }
        if s.get("$schema")
            .is_some_and(|v| is_meta_uri(v, META_OPENAPI_3_1))
        {
            s.put("$schema", JNode::Str(META_DRAFT_2020_12.into()));
        }
        if let Some(ex) = s.get("example").cloned() {
            match s.get("examples").cloned() {
                None => {
                    s.put("examples", JNode::Arr(vec![ex]));
                    s.remove("example");
                }
                Some(JNode::Arr(mut xs)) => {
                    xs.insert(0, ex);
                    s.put("examples", JNode::Arr(xs));
                    s.remove("example");
                }
                _ => {}
            }
        }
        Ok(())
    })
    .unwrap_or_else(|_| JNode::obj());
    from_draft_2020_12(transformed)
}

fn is_openapi_component_key(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}

fn sanitize_openapi_key(s: &str) -> String {
    if s.is_empty() {
        return "_".into();
    }
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[expect(clippy::too_many_lines, reason = "OpenAPI 3.1 dialect rewrite")]
fn to_openapi_3_1(multi: &JNode) -> Result<JNode, Error> {
    let shared_entries = multi
        .get("definitions")
        .and_then(JNode::as_obj)
        .unwrap_or(&[])
        .to_vec();
    let shared_keys: BTreeSet<String> = shared_entries.iter().map(|(k, _)| k.clone()).collect();
    let mut used = BTreeSet::new();
    let mut invalid = Vec::new();
    for (k, _) in &shared_entries {
        if is_openapi_component_key(k) {
            used.insert(k.clone());
        } else {
            invalid.push(k.clone());
        }
    }
    invalid.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    let mut key_map = BTreeMap::new();
    for k in &invalid {
        let base = sanitize_openapi_key(k);
        if !used.contains(&base) {
            used.insert(base.clone());
            key_map.insert(k.clone(), base);
        }
    }
    for k in &invalid {
        if key_map.contains_key(k) {
            continue;
        }
        let base = sanitize_openapi_key(k);
        let mut suffix = 1_usize;
        let cand = loop {
            let c = format!("{base}_{suffix}");
            if !used.contains(&c) {
                break c;
            }
            suffix += 1;
        };
        used.insert(cand.clone());
        key_map.insert(k.clone(), cand);
    }
    let component = |k: &str| key_map.get(k).cloned().unwrap_or_else(|| k.to_owned());
    let rewrite = |schema: JNode, reject_shared: bool| -> Result<JNode, Error> {
        let is_root = creates_resource(schema.get("$id"));
        let local_keys: BTreeSet<String> = schema
            .get("$defs")
            .and_then(JNode::as_obj)
            .map(|e| e.iter().map(|(k, _)| k.clone()).collect())
            .unwrap_or_default();
        transform_schema(schema, &mut |s, in_embedded| {
            reject_collisions(s, OPENAPI_31_COLLISIONS, "OpenAPI 3.1", "Draft 2020-12")?;
            for kw in ["$ref", "$dynamicRef"] {
                let Some(r) = s.get(kw).and_then(JNode::as_str).map(ToOwned::to_owned) else {
                    continue;
                };
                if !r.starts_with('#') {
                    continue;
                }
                let Some(path) = parse_uri_fragment(&r) else {
                    continue;
                };
                if path.len() < 2 || path[0] != "$defs" {
                    continue;
                }
                let key = &path[1];
                if is_root {
                    if reject_shared
                        && !in_embedded
                        && shared_keys.contains(key)
                        && !local_keys.contains(key)
                    {
                        return Err(unsupported(
                            kw,
                            "OpenAPI 3.1",
                            "a schema resource cannot reference the shared definitions pool",
                        ));
                    }
                    continue;
                }
                if in_embedded {
                    continue;
                }
                let mut next = vec!["components".into(), "schemas".into(), component(key)];
                next.extend_from_slice(&path[2..]);
                s.put(kw, JNode::Str(format_uri_fragment(&next)));
            }
            Ok(())
        })
    };
    let schemas_in = match multi.get("schemas") {
        Some(JNode::Arr(xs)) => xs.clone(),
        _ => Vec::new(),
    };
    let mut schemas_out = Vec::with_capacity(schemas_in.len());
    for s in schemas_in {
        schemas_out.push(rewrite(s, true)?);
    }
    let mut defs_out = JNode::obj();
    for (k, v) in shared_entries {
        defs_out.put(component(&k), rewrite(v, false)?);
    }
    let mut res = JNode::obj();
    res.put("definitions", defs_out);
    res.put("dialect", JNode::Str("openapi-3.1".into()));
    res.put("schemas", JNode::Arr(schemas_out));
    Ok(res)
}

fn is_valid_legacy_fragment(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes.first().is_some_and(u8::is_ascii_alphabetic)
        && bytes[1..]
            .iter()
            .all(|&b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b':'))
}

pub(super) fn convert_meta_kw(
    out: &mut JNode,
    v: &JNode,
    is_root: bool,
    target: &str,
    dialect: &str,
) -> Result<(), Error> {
    if is_root {
        let next = if is_meta_uri(v, META_DRAFT_2020_12) {
            JNode::Str(target.into())
        } else {
            v.clone()
        };
        out.put("$schema", next);
    } else if !is_meta_uri(v, META_DRAFT_2020_12) {
        return Err(unsupported(
            "$schema",
            dialect,
            "an embedded resource cannot declare a different dialect",
        ));
    }
    Ok(())
}

#[expect(clippy::float_cmp, reason = "exact Draft-07/04 minContains == 1 check")]
pub(super) fn check_contains_card(node: &JNode, dialect: &str) -> Result<(), Error> {
    let min_bad = node
        .get("minContains")
        .is_some_and(|v| !matches!(v, JNode::Num(n) if *n == 1.0));
    if min_bad || node.has("maxContains") {
        return Err(unsupported(
            "minContains/maxContains",
            dialect,
            "contains cardinality cannot be represented",
        ));
    }
    Ok(())
}

pub(super) fn copy_contains(src: &JNode, out: &mut JNode) {
    for k in ["minContains", "maxContains"] {
        if let Some(v) = src.get(k) {
            out.put(k, v.clone());
        }
    }
}

pub(super) fn convert_legacy_id(
    src: &JNode,
    out: &mut JNode,
    key: &str,
    dialect: &str,
) -> Result<(), Error> {
    let (id, anchor) = (src.get("$id"), src.get("$anchor"));
    let Some(a) = anchor else {
        if let Some(i) = id {
            out.put(key, i.clone());
        }
        return Ok(());
    };
    let Some(s) = a.as_str().filter(|x| is_valid_anchor(x)) else {
        return Err(unsupported("$anchor", dialect, "the anchor is not valid"));
    };
    if !is_valid_legacy_fragment(s) {
        return Err(unsupported(
            "$anchor",
            dialect,
            "the anchor cannot be represented as a plain-name fragment identifier",
        ));
    }
    if id.is_some() {
        return Err(unsupported(
            "$anchor",
            dialect,
            "it cannot be combined with the schema $id",
        ));
    }
    out.put(key, JNode::Str(format!("#{s}")));
    Ok(())
}
