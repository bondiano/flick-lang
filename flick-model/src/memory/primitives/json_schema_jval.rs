//! JSON value tree and keyword merging helpers for strict JSON Schema 2020-12 export.

use flick_core::opt::{js_number, json_string};

use super::json_ops::stringify_value;
use super::list_ops::slice_reps;
use super::{Data, Error, Heap, Value, charge};

#[derive(Clone)]
pub(super) enum JVal {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<JVal>),
    Obj(Vec<(String, JVal)>),
}

impl JVal {
    pub(super) fn obj(pairs: Vec<(&str, JVal)>) -> Self {
        Self::Obj(pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
    }

    pub(super) fn get_str(&self, key: &str) -> Option<&str> {
        let Self::Obj(entries) = self else {
            return None;
        };
        entries.iter().find(|(k, _)| k == key).and_then(|(_, v)| {
            if let Self::Str(s) = v {
                Some(s.as_str())
            } else {
                None
            }
        })
    }

    pub(super) fn set(&mut self, key: String, val: JVal) {
        if let Self::Obj(entries) = self {
            if let Some((_, slot)) = entries.iter_mut().find(|(k, _)| k == &key) {
                *slot = val;
            } else {
                entries.push((key, val));
            }
        }
    }

    pub(super) fn write_compact(&self, out: &mut String) {
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Self::Num(f) => {
                if !f.is_finite() {
                    out.push_str("null");
                } else if *f == 0.0 {
                    out.push('0');
                } else {
                    out.push_str(&js_number(*f));
                }
            }
            Self::Str(s) => out.push_str(&json_string(s)),
            Self::Arr(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.write_compact(out);
                }
                out.push(']');
            }
            Self::Obj(entries) => {
                out.push('{');
                for (i, (k, v)) in entries.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&json_string(k));
                    out.push(':');
                    v.write_compact(out);
                }
                out.push('}');
            }
        }
    }
}

pub(super) fn jval_compact(v: &JVal) -> String {
    let mut s = String::new();
    v.write_compact(&mut s);
    s
}

pub(super) fn closed_object(entries: Vec<(String, JVal)>, closed: bool) -> JVal {
    let required: Vec<JVal> = entries.iter().map(|(k, _)| JVal::Str(k.clone())).collect();
    let mut fields = vec![
        ("type".to_owned(), JVal::Str("object".into())),
        ("properties".to_owned(), JVal::Obj(entries)),
        ("required".to_owned(), JVal::Arr(required)),
    ];
    if closed {
        fields.push(("additionalProperties".to_owned(), JVal::Bool(false)));
    }
    JVal::Obj(fields)
}

fn is_lower_bound(k: &str) -> bool {
    matches!(
        k,
        "minimum" | "exclusiveMinimum" | "minLength" | "minItems" | "minProperties"
    )
}

fn is_upper_bound(k: &str) -> bool {
    matches!(
        k,
        "maximum" | "exclusiveMaximum" | "maxLength" | "maxItems" | "maxProperties"
    )
}

pub(super) fn merge_keywords(mut out: JVal, kw: Vec<(String, JVal)>) -> JVal {
    let JVal::Obj(ref mut entries) = out else {
        return out;
    };
    for (k, v) in kw {
        if let Some((_, old)) = entries.iter_mut().find(|(ek, _)| ek == &k) {
            if k == "type"
                && matches!(old, JVal::Str(s) if s == "number")
                && matches!(&v, JVal::Str(s) if s == "integer")
            {
                *old = v;
            } else if let (JVal::Num(a), JVal::Num(b)) = (&*old, &v) {
                if is_lower_bound(&k) {
                    *old = JVal::Num(a.max(*b));
                } else if is_upper_bound(&k) {
                    *old = JVal::Num(a.min(*b));
                } else if jval_compact(old) != jval_compact(&v) {
                    push_all_of(entries, JVal::Obj(vec![(k, v)]));
                }
            } else if jval_compact(old) != jval_compact(&v) {
                push_all_of(entries, JVal::Obj(vec![(k, v)]));
            }
        } else {
            entries.push((k, v));
        }
    }
    out
}

fn push_all_of(entries: &mut Vec<(String, JVal)>, item: JVal) {
    if let Some((_, JVal::Arr(list))) = entries.iter_mut().find(|(k, _)| k == "allOf") {
        list.push(item);
    } else {
        entries.push(("allOf".to_owned(), JVal::Arr(vec![item])));
    }
}

pub(super) fn value_to_jval(heap: &Heap, val: &Value, fuel: &mut u64) -> Result<JVal, Error> {
    charge(fuel, 1)?;
    match val {
        Value::Bool(b) => Ok(JVal::Bool(*b)),
        Value::Int(i) => Ok(JVal::Num(f64::from(*i))),
        Value::Float(f) => Ok(JVal::Num(*f)),
        Value::Object(tok) => match &heap.object(tok.id)?.data {
            Data::JsonNull => Ok(JVal::Null),
            Data::String(s) => Ok(JVal::Str(s.clone())),
            Data::Utf16(u) => Ok(JVal::Str(String::from_utf16_lossy(u))),
            Data::Tagged { tag, fields, .. } if tag == "$list" => {
                let mut out = Vec::with_capacity(fields.len());
                for f in fields {
                    out.push(value_to_jval(heap, f, fuel)?);
                }
                Ok(JVal::Arr(out))
            }
            Data::ListSlice { .. } => {
                let reps = slice_reps(heap, val)?;
                let mut out = Vec::with_capacity(reps.len());
                for r in &reps {
                    out.push(value_to_jval(heap, r, fuel)?);
                }
                Ok(JVal::Arr(out))
            }
            Data::Record(fields) => {
                let mut out = Vec::with_capacity(fields.len());
                for (k, v) in fields {
                    out.push((k.clone(), value_to_jval(heap, v, fuel)?));
                }
                Ok(JVal::Obj(out))
            }
            _ => Ok(JVal::Str(stringify_value(heap, val, fuel)?)),
        },
        _ => Ok(JVal::Null),
    }
}

pub(super) fn tagged_fields(heap: &Heap, val: &Value) -> Result<(String, Vec<Value>), Error> {
    let Value::Object(tok) = val else {
        return Err(Error::PatternKind("Codec.$Meta"));
    };
    let Data::Tagged { tag, fields, .. } = &heap.object(tok.id)?.data else {
        return Err(Error::PatternKind("Codec.$Meta"));
    };
    Ok((
        tag.clone(),
        fields.iter().map(Value::representation).collect(),
    ))
}

pub(super) fn opt_val(heap: &Heap, val: &Value) -> Result<Option<Value>, Error> {
    let (tag, fields) = tagged_fields(heap, val)?;
    if tag == "Prelude.Option.Some" && fields.len() == 1 {
        Ok(Some(fields[0].representation()))
    } else {
        Ok(None)
    }
}

pub(super) fn pair_of(heap: &Heap, val: &Value) -> Result<(Value, Value), Error> {
    let (_, fields) = tagged_fields(heap, val)?;
    Ok((fields[0].representation(), fields[1].representation()))
}

pub(super) fn transforms(heap: &Heap, meta: &Value) -> Result<bool, Error> {
    let (tag, fields) = tagged_fields(heap, meta)?;
    Ok(match tag.as_str() {
        "Codec.$Meta.$Number"
        | "Codec.$Meta.$Int"
        | "Codec.$Meta.$String"
        | "Codec.$Meta.$Bool"
        | "Codec.$Meta.$List"
        | "Codec.$Meta.$Tuple"
        | "Codec.$Meta.$Record"
        | "Codec.$Meta.$Never"
        | "Codec.$Meta.$Unknown"
        | "Codec.$Meta.$Literal"
        | "Codec.$Meta.$Enum"
        | "Codec.$Meta.$Json" => false,
        "Codec.$Meta.$Checked" => transforms(heap, &fields[0])?,
        "Codec.$Meta.$Refined" => transforms(heap, &fields[2])?,
        _ => true,
    })
}
