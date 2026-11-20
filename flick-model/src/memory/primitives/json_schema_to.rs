//! JSON Schema Draft 2020-12 -> Draft-07 and Draft-04 writers.

use super::Error;
use super::json_schema_conv::{Adapter, Converter, JSON_SCHEMA_SINGLES, SCHEMA_MAPS, Scope};
use super::json_schema_dialect::{
    JNode, META_DRAFT_04, META_DRAFT_07, check_contains_card, convert_legacy_id, convert_meta_kw,
    copy_contains, make_doc, reject_collisions, unsupported,
};

const DRAFT07_COLLISIONS: &[&str] = &["additionalItems", "definitions", "dependencies"];
const DRAFT04_COLLISIONS: &[&str] = &["additionalItems", "definitions", "dependencies", "id"];
const DRAFT04_SINGLES: &[&str] = &["not", "additionalProperties", "contentSchema"];

pub(super) fn convert_document(
    doc: &JNode,
    dialect: &str,
    adapter: Adapter,
    bool_to_obj: bool,
) -> Result<JNode, Error> {
    let from = doc.get("dialect").and_then(JNode::as_str).unwrap_or("");
    if from != "draft-2020-12" {
        return Err(Error::RangeError(format!(
            "RangeError: expected a draft-2020-12 document, got {from}"
        )));
    }
    let mut conv = Converter::new(adapter, true, false, bool_to_obj);
    let root_in = doc.get("root").cloned().unwrap_or_else(JNode::obj);
    let schema = conv.convert_at(root_in, &[], &[])?;
    let defs_in = doc.get("definitions").cloned().unwrap_or_else(JNode::obj);
    let defs = if let JNode::Obj(entries) = defs_in {
        let mut out = Vec::with_capacity(entries.len());
        for (k, v) in entries {
            let cv = conv.convert_at(
                v,
                &["$defs".into(), k.clone()],
                &["definitions".into(), k.clone()],
            )?;
            out.push((k, cv));
        }
        JNode::Obj(out)
    } else {
        defs_in
    };
    Ok(make_doc(
        dialect,
        conv.relocate_tree(schema),
        conv.relocate_tree(defs),
    ))
}

impl Converter {
    pub(super) fn convert_to_draft07(
        &mut self,
        node: &JNode,
        src: &[String],
        dst: &[String],
        scopes: &[Scope],
    ) -> Result<JNode, Error> {
        reject_collisions(node, DRAFT07_COLLISIONS, "Draft-07", "Draft 2020-12")?;
        let mut out = JNode::obj();
        let (mut reference, mut prefix_items, mut items) = (None, None, None);
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
                "$ref" => reference = Some(v.clone()),
                "$schema" => {
                    convert_meta_kw(&mut out, v, src.is_empty(), META_DRAFT_07, "Draft-07")?;
                }
                "$id" | "$anchor" | "dependentRequired" | "dependentSchemas" | "minContains"
                | "maxContains" => {}
                "$defs" => {
                    out.put(
                        "definitions",
                        self.sub_map(v.clone(), src, dst, k, "definitions", scopes)?,
                    );
                }
                "prefixItems" => prefix_items = Some(v.clone()),
                "items" => items = Some(v.clone()),
                "$dynamicRef"
                | "$dynamicAnchor"
                | "$vocabulary"
                | "unevaluatedProperties"
                | "unevaluatedItems" => {
                    return Err(unsupported(
                        k,
                        "Draft-07",
                        "the target dialect has no equivalent",
                    ));
                }
                "required" if matches!(v, JNode::Arr(xs) if xs.is_empty()) => {}
                _ => out.put(k.clone(), v.clone()),
            }
        }
        self.convert_tuple(&mut out, prefix_items, items, src, dst, scopes)?;
        if node.has("contains") {
            check_contains_card(node, "Draft-07")?;
            if let Some(mc) = node.get("minContains") {
                out.put("minContains", mc.clone());
            }
        } else {
            copy_contains(node, &mut out);
        }
        self.convert_deps(node, &mut out, src, dst, scopes, false)?;
        convert_legacy_id(node, &mut out, "$id", "Draft-07")?;
        Self::convert_ref(&mut out, reference, scopes);
        Ok(out)
    }

    pub(super) fn convert_to_draft04(
        &mut self,
        node: &JNode,
        src: &[String],
        dst: &[String],
        scopes: &[Scope],
    ) -> Result<JNode, Error> {
        reject_collisions(node, DRAFT04_COLLISIONS, "Draft-04", "Draft 2020-12")?;
        let mut out = JNode::obj();
        let (mut reference, mut prefix_items, mut items, mut const_val) = (None, None, None, None);
        for (k, v) in node.as_obj().unwrap_or(&[]) {
            if self.try_subschema(
                &mut out,
                k,
                v,
                src,
                dst,
                scopes,
                DRAFT04_SINGLES,
                SCHEMA_MAPS,
            )? {
                continue;
            }
            match k.as_str() {
                "$ref" => reference = Some(v.clone()),
                "$schema" => {
                    convert_meta_kw(&mut out, v, src.is_empty(), META_DRAFT_04, "Draft-04")?;
                }
                "$id" | "$anchor" | "dependentRequired" | "dependentSchemas" | "contains"
                | "minContains" | "maxContains" | "if" | "then" | "else" | "minimum"
                | "maximum" | "exclusiveMinimum" | "exclusiveMaximum" => {}
                "$defs" => {
                    out.put(
                        "definitions",
                        self.sub_map(v.clone(), src, dst, k, "definitions", scopes)?,
                    );
                }
                "prefixItems" => prefix_items = Some(v.clone()),
                "items" => items = Some(v.clone()),
                "$dynamicRef"
                | "$dynamicAnchor"
                | "$vocabulary"
                | "unevaluatedProperties"
                | "unevaluatedItems"
                | "propertyNames" => {
                    return Err(unsupported(
                        k,
                        "Draft-04",
                        "the target dialect has no equivalent",
                    ));
                }
                "const" => const_val = Some(v.clone()),
                "required" if matches!(v, JNode::Arr(xs) if xs.is_empty()) => {}
                _ => out.put(k.clone(), v.clone()),
            }
        }
        self.convert_tuple(&mut out, prefix_items, items, src, dst, scopes)?;
        draft04_bound(node, &mut out, "minimum", "exclusiveMinimum");
        draft04_bound(node, &mut out, "maximum", "exclusiveMaximum");
        self.convert_deps(node, &mut out, src, dst, scopes, true)?;
        convert_legacy_id(node, &mut out, "id", "Draft-04")?;
        self.draft04_conditionals(node, &mut out, src, dst, scopes)?;
        self.draft04_contains(node, &mut out, src, dst, scopes)?;
        if let Some(cv) = const_val {
            if node.has("enum") {
                let mut e = JNode::obj();
                e.put("enum", JNode::Arr(vec![cv]));
                append_all_of(&mut out, e);
            } else {
                out.put("enum", JNode::Arr(vec![cv]));
            }
        }
        Self::convert_ref(&mut out, reference, scopes);
        Ok(out)
    }

    fn convert_tuple(
        &mut self,
        out: &mut JNode,
        prefix: Option<JNode>,
        items: Option<JNode>,
        src: &[String],
        dst: &[String],
        scopes: &[Scope],
    ) -> Result<(), Error> {
        let Some(p) = prefix else {
            if let Some(it) = items {
                out.put("items", self.sub(it, src, dst, "items", "items", scopes)?);
            }
            return Ok(());
        };
        out.put(
            "items",
            self.sub_arr(p, src, dst, "prefixItems", "items", scopes)?,
        );
        if let Some(it) = items {
            out.put(
                "additionalItems",
                self.sub(it, src, dst, "items", "additionalItems", scopes)?,
            );
        }
        Ok(())
    }

    fn convert_ref(out: &mut JNode, reference: Option<JNode>, scopes: &[Scope]) {
        let Some(r) = reference else { return };
        if r.as_str().is_some() && out.as_obj().is_some_and(|e| !e.is_empty()) {
            let mut rf = JNode::obj();
            Self::put_ref(&mut rf, r, scopes);
            append_all_of(out, rf);
        } else {
            Self::put_ref(out, r, scopes);
        }
    }

    fn convert_deps(
        &mut self,
        node: &JNode,
        out: &mut JNode,
        src: &[String],
        dst: &[String],
        scopes: &[Scope],
        draft04: bool,
    ) -> Result<(), Error> {
        let req = node.get("dependentRequired").and_then(JNode::as_obj);
        let sch = node.get("dependentSchemas").and_then(JNode::as_obj);
        if req.is_none() && sch.is_none() {
            return Ok(());
        }
        let mut keys = Vec::new();
        for list in [req, sch].into_iter().flatten() {
            for (k, _) in list {
                if !keys.contains(k) {
                    keys.push(k.clone());
                }
            }
        }
        let mut deps = JNode::obj();
        for key in keys {
            let rv = req.and_then(|m| m.iter().find(|(k, _)| k == &key).map(|(_, v)| v.clone()));
            let sv = sch.and_then(|m| m.iter().find(|(k, _)| k == &key).map(|(_, v)| v.clone()));
            let omit = draft04 && matches!(&rv, Some(JNode::Arr(xs)) if xs.is_empty());
            match (sv, rv) {
                (None, Some(r)) if !omit => deps.put(key, r),
                (Some(s), r) if r.is_none() || omit => {
                    deps.put(
                        key.clone(),
                        self.sub_at(
                            s,
                            src,
                            dst,
                            &["dependentSchemas", &key],
                            &["dependencies", &key],
                            scopes,
                        )?,
                    );
                }
                (Some(s), Some(r)) => {
                    let cs = self.sub_at(
                        s,
                        src,
                        dst,
                        &["dependentSchemas", &key],
                        &["dependencies", &key, "allOf", "0"],
                        scopes,
                    )?;
                    let mut req_obj = JNode::obj();
                    req_obj.put("required", r);
                    let mut comb = JNode::obj();
                    comb.put("allOf", JNode::Arr(vec![cs, req_obj]));
                    deps.put(key, comb);
                }
                _ => {}
            }
        }
        if deps.as_obj().is_some_and(|e| !e.is_empty()) {
            out.put("dependencies", deps);
        }
        Ok(())
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Draft-04 if/then/else boolean encoding"
    )]
    fn draft04_conditionals(
        &mut self,
        node: &JNode,
        out: &mut JNode,
        src: &[String],
        dst: &[String],
        scopes: &[Scope],
    ) -> Result<(), Error> {
        let (hi, ht, he) = (node.has("if"), node.has("then"), node.has("else"));
        if !hi || (!ht && !he) {
            for k in ["if", "then", "else"] {
                if let Some(v) = node.get(k) {
                    out.put(k, self.sub(v.clone(), src, dst, k, k, scopes)?);
                }
            }
            return Ok(());
        }
        let idx = all_of_len(out);
        let cond = match (ht, he) {
            (true, true) => {
                if has_schema_id(node.get("if").expect("if")) {
                    return Err(unsupported(
                        "if",
                        "Draft-04",
                        "lowering both branches would duplicate a schema identifier",
                    ));
                }
                let i0 = self.sub_at(
                    node.get("if").expect("if").clone(),
                    src,
                    dst,
                    &["if"],
                    &["allOf", &idx, "anyOf", "0", "allOf", "0"],
                    scopes,
                )?;
                let t0 = self.sub_at(
                    node.get("then").expect("then").clone(),
                    src,
                    dst,
                    &["then"],
                    &["allOf", &idx, "anyOf", "0", "allOf", "1"],
                    scopes,
                )?;
                let i1 = self.sub_at(
                    node.get("if").expect("if").clone(),
                    src,
                    dst,
                    &["if"],
                    &["allOf", &idx, "anyOf", "1", "allOf", "0", "not"],
                    scopes,
                )?;
                let e1 = self.sub_at(
                    node.get("else").expect("else").clone(),
                    src,
                    dst,
                    &["else"],
                    &["allOf", &idx, "anyOf", "1", "allOf", "1"],
                    scopes,
                )?;
                let mut b0 = JNode::obj();
                b0.put("allOf", JNode::Arr(vec![i0, t0]));
                let mut not_i = JNode::obj();
                not_i.put("not", i1);
                let mut b1 = JNode::obj();
                b1.put("allOf", JNode::Arr(vec![not_i, e1]));
                let mut a = JNode::obj();
                a.put("anyOf", JNode::Arr(vec![b0, b1]));
                a
            }
            (true, false) => {
                let i0 = self.sub_at(
                    node.get("if").expect("if").clone(),
                    src,
                    dst,
                    &["if"],
                    &["allOf", &idx, "anyOf", "0", "not"],
                    scopes,
                )?;
                let t1 = self.sub_at(
                    node.get("then").expect("then").clone(),
                    src,
                    dst,
                    &["then"],
                    &["allOf", &idx, "anyOf", "1"],
                    scopes,
                )?;
                let mut not_i = JNode::obj();
                not_i.put("not", i0);
                let mut a = JNode::obj();
                a.put("anyOf", JNode::Arr(vec![not_i, t1]));
                a
            }
            (false, true) => {
                let i0 = self.sub_at(
                    node.get("if").expect("if").clone(),
                    src,
                    dst,
                    &["if"],
                    &["allOf", &idx, "anyOf", "0"],
                    scopes,
                )?;
                let e1 = self.sub_at(
                    node.get("else").expect("else").clone(),
                    src,
                    dst,
                    &["else"],
                    &["allOf", &idx, "anyOf", "1"],
                    scopes,
                )?;
                let mut a = JNode::obj();
                a.put("anyOf", JNode::Arr(vec![i0, e1]));
                a
            }
            _ => unreachable!(),
        };
        append_all_of(out, cond);
        Ok(())
    }

    fn draft04_contains(
        &mut self,
        node: &JNode,
        out: &mut JNode,
        src: &[String],
        dst: &[String],
        scopes: &[Scope],
    ) -> Result<(), Error> {
        let Some(c) = node.get("contains") else {
            copy_contains(node, out);
            return Ok(());
        };
        check_contains_card(node, "Draft-04")?;
        let idx = all_of_len(out);
        let conv = self.sub_at(
            c.clone(),
            src,
            dst,
            &["contains"],
            &["allOf", &idx, "anyOf", "1", "not", "items", "not"],
            scopes,
        )?;
        let mut arr_t = JNode::obj();
        arr_t.put("type", JNode::Str("array".into()));
        let mut not_arr = JNode::obj();
        not_arr.put("not", arr_t);
        let mut not_c = JNode::obj();
        not_c.put("not", conv);
        let mut items_obj = JNode::obj();
        items_obj.put("items", not_c);
        let mut not_items = JNode::obj();
        not_items.put("not", items_obj);
        let mut any = JNode::obj();
        any.put("anyOf", JNode::Arr(vec![not_arr, not_items]));
        append_all_of(out, any);
        Ok(())
    }
}

fn draft04_bound(src: &JNode, out: &mut JNode, bound: &str, ex_key: &str) {
    let b = src.get(bound);
    if let Some(JNode::Num(ex)) = src.get(ex_key) {
        let keep_b = matches!(b, Some(JNode::Num(n)) if (bound == "minimum" && n > ex) || (bound == "maximum" && n < ex));
        if keep_b {
            out.put(bound, b.expect("b").clone());
        } else {
            out.put(bound, JNode::Num(*ex));
            out.put(ex_key, JNode::Bool(true));
        }
    } else if let Some(bv) = b {
        out.put(bound, bv.clone());
    }
}

fn append_all_of(out: &mut JNode, schema: JNode) {
    if let Some(JNode::Arr(mut xs)) = out.get("allOf").cloned() {
        xs.push(schema);
        out.put("allOf", JNode::Arr(xs));
    } else {
        out.put("allOf", JNode::Arr(vec![schema]));
    }
}

fn all_of_len(out: &JNode) -> String {
    match out.get("allOf") {
        Some(JNode::Arr(xs)) => xs.len().to_string(),
        _ => "0".into(),
    }
}

fn has_schema_id(node: &JNode) -> bool {
    let Some(entries) = node.as_obj() else {
        return false;
    };
    if node.has("$id") || node.has("$anchor") {
        return true;
    }
    for (k, v) in entries {
        match k.as_str() {
            "$defs" | "properties" | "patternProperties" | "dependentSchemas"
                if v.as_obj()
                    .is_some_and(|m| m.iter().any(|(_, x)| has_schema_id(x))) =>
            {
                return true;
            }
            "allOf" | "anyOf" | "oneOf" | "prefixItems" if matches!(v, JNode::Arr(xs) if xs.iter().any(has_schema_id)) =>
            {
                return true;
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
            | "contentSchema"
                if has_schema_id(v) =>
            {
                return true;
            }
            _ => {}
        }
    }
    false
}
