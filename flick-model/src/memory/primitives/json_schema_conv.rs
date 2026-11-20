//! JSON Schema conversion kernel and Draft-07 / `OpenAPI` 3.0 -> Draft 2020-12 adapters.

use std::collections::BTreeMap;

use super::Error;
use super::json_pointer_ops::{format_uri_fragment, parse_uri_fragment};
use super::json_schema_dialect::{
    JNode, META_DRAFT_07, META_DRAFT_2020_12, creates_resource, is_meta_uri, is_valid_anchor,
    reject_collisions, rewrite_openapi_ref, unsupported,
};

const ROOT_RESOURCE_URI: &str = "https://effect.invalid/.json-schema/";
const PRE_2020_COLLISIONS: &[&str] = &[
    "$anchor",
    "$defs",
    "$dynamicAnchor",
    "$dynamicRef",
    "$vocabulary",
    "contentSchema",
    "dependentRequired",
    "dependentSchemas",
    "maxContains",
    "minContains",
    "prefixItems",
    "unevaluatedItems",
    "unevaluatedProperties",
];
const OPENAPI_30_EXTRA: &[&str] = &[
    "$comment",
    "$id",
    "$schema",
    "const",
    "contains",
    "contentEncoding",
    "contentMediaType",
    "else",
    "examples",
    "if",
    "patternProperties",
    "propertyNames",
    "then",
];
pub(super) const SCHEMA_MAPS: &[&str] = &["properties", "patternProperties"];
const SCHEMA_ARRAYS: &[&str] = &["allOf", "anyOf", "oneOf"];
pub(super) const JSON_SCHEMA_SINGLES: &[&str] = &[
    "not",
    "additionalProperties",
    "propertyNames",
    "contains",
    "if",
    "then",
    "else",
    "contentSchema",
];
const OPENAPI30_SINGLES: &[&str] = &["not", "items", "additionalProperties"];

#[derive(Clone, Copy)]
pub(super) enum Adapter {
    Draft07To2020,
    OpenApi30To2020,
    ToDraft07,
    ToDraft04,
}

#[derive(Clone)]
pub(super) struct Scope {
    pub(super) source_root: Vec<String>,
    pub(super) target_root: Vec<String>,
    pub(super) uri: String,
}

pub(super) struct Converter {
    adapter: Adapter,
    track_ids: bool,
    ignore_ref_siblings: bool,
    bool_to_obj: bool,
    locations: BTreeMap<(String, Vec<String>), Vec<String>>,
    root_uri: String,
}

fn resolve_uri(reference: &str, base: &str) -> Option<(String, String)> {
    let trimmed = reference.trim();
    let (without_frag, frag) = match trimmed.split_once('#') {
        Some((p, f)) => (p, Some(f)),
        None => (trimmed, None),
    };
    let resolved_base = if without_frag.contains("://") {
        without_frag.to_owned()
    } else if without_frag.is_empty() {
        base.split('#').next().unwrap_or(base).to_owned()
    } else if let Some(rest) = without_frag.strip_prefix("//") {
        let scheme = base.split("://").next()?;
        format!("{scheme}://{rest}")
    } else if without_frag.starts_with('/') {
        let (scheme, after) = base.split_once("://")?;
        let authority = after.split('/').next().unwrap_or(after);
        format!("{scheme}://{authority}{without_frag}")
    } else {
        let b = base.split('#').next().unwrap_or(base);
        let prefix = match b.rfind('/') {
            Some(i) => &b[..=i],
            None => b,
        };
        format!("{prefix}{without_frag}")
    };
    let hash = frag
        .filter(|f| !f.is_empty())
        .map_or(String::new(), |f| format!("#{f}"));
    Some((resolved_base, hash))
}

fn relocate_ref(reference: &str, target: &[String]) -> String {
    match reference.find('#') {
        None if target.is_empty() => reference.to_owned(),
        Some(i) => format!("{}{}", &reference[..i], format_uri_fragment(target)),
        None => format!("{reference}{}", format_uri_fragment(target)),
    }
}

impl Converter {
    pub(super) fn new(
        adapter: Adapter,
        track_ids: bool,
        ignore_ref_siblings: bool,
        bool_to_obj: bool,
    ) -> Self {
        Self {
            adapter,
            track_ids,
            ignore_ref_siblings,
            bool_to_obj,
            locations: BTreeMap::new(),
            root_uri: ROOT_RESOURCE_URI.into(),
        }
    }

    fn resource_id<'a>(&self, src: &'a JNode) -> Option<&'a JNode> {
        if self.ignore_ref_siblings && src.get("$ref").and_then(JNode::as_str).is_some() {
            return None;
        }
        src.get("$id")
    }

    pub(super) fn convert_root(&mut self, root: JNode) -> Result<JNode, Error> {
        let out = self.convert_at(root, &[], &[])?;
        Ok(self.relocate_tree(out))
    }

    pub(super) fn convert_at(
        &mut self,
        root: JNode,
        src: &[String],
        dst: &[String],
    ) -> Result<JNode, Error> {
        if src.is_empty() && self.track_ids {
            self.root_uri = ROOT_RESOURCE_URI.into();
            if let Some(id) = self.resource_id(&root).and_then(JNode::as_str)
                && let Some((uri, _)) = resolve_uri(id, ROOT_RESOURCE_URI)
            {
                self.root_uri = uri;
            }
        }
        let scopes = vec![Scope {
            source_root: vec![],
            target_root: vec![],
            uri: self.root_uri.clone(),
        }];
        self.visit(root, src, dst, &scopes)
    }

    fn record(&mut self, src: &[String], dst: &[String], scopes: &[Scope]) {
        for s in scopes {
            self.locations.insert(
                (s.uri.clone(), src[s.source_root.len()..].to_vec()),
                dst[s.target_root.len()..].to_vec(),
            );
        }
    }

    fn visit(
        &mut self,
        node: JNode,
        src: &[String],
        dst: &[String],
        scopes: &[Scope],
    ) -> Result<JNode, Error> {
        if let JNode::Bool(b) = node {
            self.record(src, dst, scopes);
            return Ok(if self.bool_to_obj {
                if b {
                    JNode::obj()
                } else {
                    let mut n = JNode::obj();
                    n.put("not", JNode::obj());
                    n
                }
            } else {
                JNode::Bool(b)
            });
        }
        if !matches!(node, JNode::Obj(_)) {
            return Ok(node);
        }
        let mut scopes = scopes.to_vec();
        if !src.is_empty()
            && self.track_ids
            && let Some(id) = self.resource_id(&node)
            && creates_resource(Some(id))
            && let Some((uri, _)) = resolve_uri(
                id.as_str().unwrap_or(""),
                &scopes.last().expect("scope").uri,
            )
        {
            scopes.push(Scope {
                source_root: src.to_vec(),
                target_root: dst.to_vec(),
                uri,
            });
        }
        self.record(src, dst, &scopes);
        match self.adapter {
            Adapter::Draft07To2020 => self.draft07_to_2020(&node, src, dst, &scopes),
            Adapter::OpenApi30To2020 => self.openapi30_to_2020(&node, src, dst, &scopes),
            Adapter::ToDraft07 => self.convert_to_draft07(&node, src, dst, &scopes),
            Adapter::ToDraft04 => self.convert_to_draft04(&node, src, dst, &scopes),
        }
    }

    pub(super) fn sub(
        &mut self,
        v: JNode,
        src: &[String],
        dst: &[String],
        sk: &str,
        dk: &str,
        scopes: &[Scope],
    ) -> Result<JNode, Error> {
        let mut s = src.to_vec();
        s.push(sk.to_owned());
        let mut d = dst.to_vec();
        d.push(dk.to_owned());
        self.visit(v, &s, &d, scopes)
    }

    pub(super) fn sub_at(
        &mut self,
        v: JNode,
        src: &[String],
        dst: &[String],
        sp: &[&str],
        dp: &[&str],
        scopes: &[Scope],
    ) -> Result<JNode, Error> {
        let mut s = src.to_vec();
        s.extend(sp.iter().map(|&x| x.to_owned()));
        let mut d = dst.to_vec();
        d.extend(dp.iter().map(|&x| x.to_owned()));
        self.visit(v, &s, &d, scopes)
    }

    pub(super) fn sub_arr(
        &mut self,
        v: JNode,
        src: &[String],
        dst: &[String],
        sk: &str,
        dk: &str,
        scopes: &[Scope],
    ) -> Result<JNode, Error> {
        let JNode::Arr(xs) = v else { return Ok(v) };
        let mut out = Vec::with_capacity(xs.len());
        for (i, x) in xs.into_iter().enumerate() {
            let idx = i.to_string();
            out.push(self.sub_at(x, src, dst, &[sk, &idx], &[dk, &idx], scopes)?);
        }
        Ok(JNode::Arr(out))
    }

    pub(super) fn sub_map(
        &mut self,
        v: JNode,
        src: &[String],
        dst: &[String],
        sk: &str,
        dk: &str,
        scopes: &[Scope],
    ) -> Result<JNode, Error> {
        let JNode::Obj(entries) = v else { return Ok(v) };
        let mut out = Vec::with_capacity(entries.len());
        for (k, val) in entries {
            let conv = self.sub_at(val, src, dst, &[sk, &k], &[dk, &k], scopes)?;
            out.push((k, conv));
        }
        Ok(JNode::Obj(out))
    }

    pub(super) fn put_ref(out: &mut JNode, val: JNode, scopes: &[Scope]) {
        if let JNode::Str(r) = val {
            let uri = &scopes.last().expect("scope").uri;
            out.put("$ref", JNode::Str(format!("\x00{uri}\x00{r}")));
        } else {
            out.put("$ref", val);
        }
    }

    pub(super) fn relocate_tree(&self, node: JNode) -> JNode {
        match node {
            JNode::Arr(xs) => JNode::Arr(xs.into_iter().map(|x| self.relocate_tree(x)).collect()),
            JNode::Obj(entries) => JNode::Obj(
                entries
                    .into_iter()
                    .map(|(k, v)| {
                        if k == "$ref"
                            && let JNode::Str(ref s) = v
                            && let Some(rest) = s.strip_prefix('\x00')
                            && let Some((uri, raw)) = rest.split_once('\x00')
                        {
                            let mut next = raw.to_owned();
                            if let Some((base, hash)) = resolve_uri(raw, uri)
                                && let Some(ptr) = parse_uri_fragment(&hash)
                                && let Some(target) = self.locations.get(&(base, ptr))
                            {
                                next = relocate_ref(raw, target);
                            }
                            return (k, JNode::Str(next));
                        }
                        (k, self.relocate_tree(v))
                    })
                    .collect(),
            ),
            other => other,
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "shared subschema keyword dispatcher"
    )]
    pub(super) fn try_subschema(
        &mut self,
        out: &mut JNode,
        key: &str,
        val: &JNode,
        src: &[String],
        dst: &[String],
        scopes: &[Scope],
        singles: &[&str],
        maps: &[&str],
    ) -> Result<bool, Error> {
        if maps.contains(&key) {
            out.put(key, self.sub_map(val.clone(), src, dst, key, key, scopes)?);
            Ok(true)
        } else if SCHEMA_ARRAYS.contains(&key) {
            out.put(key, self.sub_arr(val.clone(), src, dst, key, key, scopes)?);
            Ok(true)
        } else if singles.contains(&key) {
            out.put(key, self.sub(val.clone(), src, dst, key, key, scopes)?);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Draft-07 -> Draft 2020-12 keyword translation"
    )]
    fn draft07_to_2020(
        &mut self,
        node: &JNode,
        src: &[String],
        dst: &[String],
        scopes: &[Scope],
    ) -> Result<JNode, Error> {
        let mut out = JNode::obj();
        if let Some(r) = node.get("$ref").filter(|v| v.as_str().is_some()) {
            Self::put_ref(&mut out, r.clone(), scopes);
            if let Some(d) = node.get("definitions") {
                out.put(
                    "$defs",
                    self.sub_map(d.clone(), src, dst, "definitions", "$defs", scopes)?,
                );
            }
            return Ok(out);
        }
        let mut collisions = PRE_2020_COLLISIONS.to_vec();
        collisions.push("deprecated");
        reject_collisions(node, &collisions, "Draft 2020-12", "Draft-07")?;
        let (mut items, mut additional_items) = (None, None);
        for (k, v) in node.as_obj().unwrap_or(&[]) {
            if self.try_subschema(
                &mut out,
                k,
                v,
                src,
                dst,
                scopes,
                JSON_SCHEMA_SINGLES,
                SCHEMA_MAPS,
            )? {
                continue;
            }
            match k.as_str() {
                "$schema" => {
                    let next = if is_meta_uri(v, META_DRAFT_07) {
                        JNode::Str(META_DRAFT_2020_12.into())
                    } else {
                        v.clone()
                    };
                    out.put("$schema", next);
                }
                "$id" => {
                    if let Some(s) = v.as_str()
                        && let Some((id, anchor)) = s.split_once('#')
                    {
                        if !anchor.is_empty() && !is_valid_anchor(anchor) {
                            return Err(unsupported(
                                "$id",
                                "Draft 2020-12",
                                &format!("fragment \"#{anchor}\" is not a valid $anchor"),
                            ));
                        }
                        if !id.is_empty() {
                            out.put("$id", JNode::Str(id.to_owned()));
                        }
                        if !anchor.is_empty() {
                            out.put("$anchor", JNode::Str(anchor.to_owned()));
                        }
                    } else {
                        out.put("$id", v.clone());
                    }
                }
                "definitions" => {
                    out.put(
                        "$defs",
                        self.sub_map(v.clone(), src, dst, k, "$defs", scopes)?,
                    );
                }
                "dependencies" => {
                    if let Some(deps) = v.as_obj() {
                        let (mut req, mut sch) = (JNode::obj(), JNode::obj());
                        for (dk, dv) in deps {
                            if matches!(dv, JNode::Arr(_)) {
                                req.put(dk.clone(), dv.clone());
                            } else {
                                sch.put(
                                    dk.clone(),
                                    self.sub_at(
                                        dv.clone(),
                                        src,
                                        dst,
                                        &["dependencies", dk],
                                        &["dependentSchemas", dk],
                                        scopes,
                                    )?,
                                );
                            }
                        }
                        if req.as_obj().is_some_and(|e| !e.is_empty()) {
                            out.put("dependentRequired", req);
                        }
                        if sch.as_obj().is_some_and(|e| !e.is_empty()) {
                            out.put("dependentSchemas", sch);
                        }
                    } else {
                        out.put(k.clone(), v.clone());
                    }
                }
                "items" => items = Some(v.clone()),
                "additionalItems" => additional_items = Some(v.clone()),
                _ => out.put(k.clone(), v.clone()),
            }
        }
        if let Some(JNode::Arr(xs)) = items {
            out.put(
                "prefixItems",
                self.sub_arr(JNode::Arr(xs), src, dst, "items", "prefixItems", scopes)?,
            );
            if let Some(ai) = additional_items {
                out.put(
                    "items",
                    self.sub(ai, src, dst, "additionalItems", "items", scopes)?,
                );
            }
        } else if let Some(it) = items {
            out.put("items", self.sub(it, src, dst, "items", "items", scopes)?);
        }
        Ok(out)
    }

    fn openapi30_to_2020(
        &mut self,
        node: &JNode,
        src: &[String],
        dst: &[String],
        scopes: &[Scope],
    ) -> Result<JNode, Error> {
        let mut out = JNode::obj();
        if let Some(r) = node.get("$ref").and_then(JNode::as_str) {
            Self::put_ref(&mut out, JNode::Str(rewrite_openapi_ref(r)), scopes);
            return Ok(out);
        }
        let mut collisions = PRE_2020_COLLISIONS.to_vec();
        collisions.extend_from_slice(OPENAPI_30_EXTRA);
        reject_collisions(node, &collisions, "Draft 2020-12", "OpenAPI 3.0")?;
        for (k, v) in node.as_obj().unwrap_or(&[]) {
            if self.try_subschema(
                &mut out,
                k,
                v,
                src,
                dst,
                scopes,
                OPENAPI30_SINGLES,
                &["properties"],
            )? {
                continue;
            }
            match k.as_str() {
                "example" => out.put("examples", JNode::Arr(vec![v.clone()])),
                "nullable" | "exclusiveMinimum" | "exclusiveMaximum" => {}
                _ => out.put(k.clone(), v.clone()),
            }
        }
        for (bound, ex_key) in [
            ("minimum", "exclusiveMinimum"),
            ("maximum", "exclusiveMaximum"),
        ] {
            match node.get(ex_key) {
                Some(JNode::Bool(true)) => {
                    if let Some(JNode::Num(n)) = node.get(bound) {
                        out.put(ex_key, JNode::Num(*n));
                        out.remove(bound);
                    }
                }
                Some(JNode::Bool(false)) | None => {}
                Some(other) => out.put(ex_key, other.clone()),
            }
        }
        if let Some(t) = node.get("type").and_then(JNode::as_str)
            && matches!(node.get("nullable"), Some(JNode::Bool(true)))
        {
            out.put(
                "type",
                JNode::Arr(vec![JNode::Str(t.into()), JNode::Str("null".into())]),
            );
        }
        Ok(out)
    }
}
