//! Strict JSON Schema 2020-12 exporter over counted `Codec.$Meta` AST values.

use flick_core::ir::Lit;

use super::json_pointer_ops::indent_json;
use super::json_schema_jval::{
    JVal, closed_object, jval_compact, merge_keywords, opt_val, pair_of, tagged_fields, transforms,
    value_to_jval,
};
use super::list_ops::slice_reps;
use super::{Error, Fuel, Heap, Value, charge};
use crate::tier0::Data;

const INT32_MIN: f64 = -2_147_483_648.0;
const INT32_MAX: f64 = 2_147_483_647.0;

fn int_schema(min: f64, max: f64) -> JVal {
    JVal::obj(vec![
        ("type", JVal::Str("integer".into())),
        ("minimum", JVal::Num(min)),
        ("maximum", JVal::Num(max)),
    ])
}

struct Exporter {
    defs: Vec<(String, Option<JVal>)>,
}

enum ExportErr {
    Refused(String),
    Vm(Error),
}

impl From<Error> for ExportErr {
    fn from(err: Error) -> Self {
        Self::Vm(err)
    }
}

fn refuse(path: &str, reason: &str) -> ExportErr {
    ExportErr::Refused(format!("{path} -> JSON Schema: {reason}"))
}

fn at_path(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_owned()
    } else {
        format!("{path}.{key}")
    }
}

impl Exporter {
    #[expect(
        clippy::too_many_lines,
        reason = "strict JSON Schema 2020-12 AST walker"
    )]
    fn export(
        &mut self,
        heap: &Heap,
        meta: &Value,
        path: &str,
        fuel: Fuel<'_>,
    ) -> Result<JVal, ExportErr> {
        charge(fuel, 1)?;
        let (tag, fields) = tagged_fields(heap, meta)?;
        match tag.as_str() {
            "Codec.$Meta.$Number" => Ok(JVal::obj(vec![("type", JVal::Str("number".into()))])),
            "Codec.$Meta.$Int" => Ok(int_schema(INT32_MIN, INT32_MAX)),
            "Codec.$Meta.$Int8" => Ok(int_schema(-128.0, 127.0)),
            "Codec.$Meta.$Int16" => Ok(int_schema(-32_768.0, 32_767.0)),
            "Codec.$Meta.$UInt8" => Ok(int_schema(0.0, 255.0)),
            "Codec.$Meta.$UInt16" => Ok(int_schema(0.0, 65_535.0)),
            "Codec.$Meta.$UInt32" => Ok(int_schema(0.0, 4_294_967_295.0)),
            "Codec.$Meta.$String" => Ok(JVal::obj(vec![("type", JVal::Str("string".into()))])),
            "Codec.$Meta.$Bool" => Ok(JVal::obj(vec![("type", JVal::Str("boolean".into()))])),
            "Codec.$Meta.$BigInt" => Ok(JVal::obj(vec![
                ("type", JVal::Str("string".into())),
                ("pattern", JVal::Str("^-?[0-9]+$".into())),
            ])),
            "Codec.$Meta.$Unit" => Ok(JVal::obj(vec![("type", JVal::Str("null".into()))])),
            "Codec.$Meta.$Never" => Ok(JVal::Bool(false)),
            "Codec.$Meta.$Unknown" => Err(refuse(
                path,
                "an Unknown value accepts anything, which strict export does not write",
            )),
            "Codec.$Meta.$Redacted" => Err(refuse(
                path,
                "a Redacted value has no JSON form without an explicit policy",
            )),
            "Codec.$Meta.$MaskedRedacted" => {
                Ok(JVal::obj(vec![("const", JVal::Str("<redacted>".into()))]))
            }
            "Codec.$Meta.$Json" => Err(refuse(path, "JSON value has no JSON form")),
            "Codec.$Meta.$LosslessNumber" => Ok(JVal::obj(vec![(
                "anyOf",
                JVal::Arr(vec![
                    JVal::obj(vec![("type", JVal::Str("number".into()))]),
                    JVal::obj(vec![(
                        "enum",
                        JVal::Arr(vec![
                            JVal::Str("NaN".into()),
                            JVal::Str("Infinity".into()),
                            JVal::Str("-Infinity".into()),
                            JVal::Str("-0".into()),
                        ]),
                    )]),
                ]),
            )])),
            "Codec.$Meta.$Refused" => Err(refuse(path, &heap.string_lossy(&fields[0])?)),
            "Codec.$Meta.$Literal" => Ok(JVal::obj(vec![(
                "const",
                value_to_jval(heap, &fields[0], fuel)?,
            )])),
            "Codec.$Meta.$Enum" => {
                let reps = slice_reps(heap, &fields[0])?;
                let mut vals = Vec::with_capacity(reps.len());
                for r in &reps {
                    vals.push(value_to_jval(heap, r, fuel)?);
                }
                Ok(JVal::obj(vec![("enum", JVal::Arr(vals))]))
            }
            "Codec.$Meta.$Stage" => self.export(heap, &fields[0], path, fuel),
            "Codec.$Meta.$List" => Ok(JVal::obj(vec![
                ("type", JVal::Str("array".into())),
                (
                    "items",
                    self.export(heap, &fields[0], &format!("{path}[]"), fuel)?,
                ),
            ])),
            "Codec.$Meta.$NonEmptyList" => {
                let first = self.export(heap, &fields[0], &format!("{path}[0]"), fuel)?;
                let items = self.export(heap, &fields[0], &format!("{path}[]"), fuel)?;
                Ok(JVal::obj(vec![
                    ("type", JVal::Str("array".into())),
                    ("prefixItems", JVal::Arr(vec![first])),
                    ("items", items),
                    ("minItems", JVal::Num(1.0)),
                ]))
            }
            "Codec.$Meta.$ListEnsure" => {
                let single = self.export(heap, &fields[0], path, fuel)?;
                let arr = JVal::obj(vec![
                    ("type", JVal::Str("array".into())),
                    (
                        "items",
                        self.export(heap, &fields[0], &format!("{path}[]"), fuel)?,
                    ),
                ]);
                Ok(JVal::obj(vec![("anyOf", JVal::Arr(vec![single, arr]))]))
            }
            "Codec.$Meta.$Tuple" => {
                let reps = slice_reps(heap, &fields[0])?;
                let mut items = Vec::with_capacity(reps.len());
                for (i, r) in reps.iter().enumerate() {
                    items.push(self.export(heap, r, &format!("{path}[{i}]"), fuel)?);
                }
                Ok(JVal::obj(vec![
                    ("type", JVal::Str("array".into())),
                    ("prefixItems", JVal::Arr(items)),
                    ("items", JVal::Bool(false)),
                ]))
            }
            "Codec.$Meta.$Option" => {
                let inner = self.export(heap, &fields[0], path, fuel)?;
                Ok(JVal::obj(vec![(
                    "anyOf",
                    JVal::Arr(vec![
                        inner,
                        JVal::obj(vec![("type", JVal::Str("null".into()))]),
                    ]),
                )]))
            }
            "Codec.$Meta.$TaggedOption" => {
                let none_obj = closed_object(
                    vec![(
                        "_tag".into(),
                        JVal::obj(vec![("const", JVal::Str("None".into()))]),
                    )],
                    true,
                );
                let val_schema = self.export(heap, &fields[0], &at_path(path, "value"), fuel)?;
                let some_obj = closed_object(
                    vec![
                        (
                            "_tag".into(),
                            JVal::obj(vec![("const", JVal::Str("Some".into()))]),
                        ),
                        ("value".into(), val_schema),
                    ],
                    true,
                );
                Ok(JVal::obj(vec![(
                    "oneOf",
                    JVal::Arr(vec![none_obj, some_obj]),
                )]))
            }
            "Codec.$Meta.$Result" => {
                let ok_s = self.export(heap, &fields[1], &at_path(path, "ok"), fuel)?;
                let err_s = self.export(heap, &fields[0], &at_path(path, "err"), fuel)?;
                Ok(JVal::obj(vec![(
                    "oneOf",
                    JVal::Arr(vec![
                        closed_object(vec![("ok".into(), ok_s)], true),
                        closed_object(vec![("err".into(), err_s)], true),
                    ]),
                )]))
            }
            "Codec.$Meta.$Record" => {
                let reps = slice_reps(heap, &fields[0])?;
                let mut pairs = Vec::with_capacity(reps.len());
                for r in &reps {
                    let (kv, mv) = pair_of(heap, r)?;
                    pairs.push((heap.string_lossy(&kv)?, mv));
                }
                pairs.sort_by(|a, b| a.0.cmp(&b.0));
                let mut entries = Vec::with_capacity(pairs.len());
                for (k, mv) in &pairs {
                    entries.push((k.clone(), self.export(heap, mv, &at_path(path, k), fuel)?));
                }
                Ok(closed_object(entries, true))
            }
            "Codec.$Meta.$Block" => {
                let reps = slice_reps(heap, &fields[0])?;
                let ignore_extra = matches!(fields[1], Value::Bool(true));
                let mut entries = Vec::with_capacity(reps.len());
                for r in &reps {
                    let (kv, mv) = pair_of(heap, r)?;
                    let k = heap.string_lossy(&kv)?;
                    entries.push((k.clone(), self.export(heap, &mv, &at_path(path, &k), fuel)?));
                }
                Ok(closed_object(entries, !ignore_extra))
            }
            "Codec.$Meta.$Entries" => {
                let val_s = self.export(heap, &fields[0], &format!("{path}.*"), fuel)?;
                let mut out = JVal::obj(vec![
                    ("type", JVal::Str("object".into())),
                    ("additionalProperties", val_s),
                ]);
                for chk in &slice_reps(heap, &fields[1])? {
                    let kw = Self::check_keywords(heap, chk, "object", path, false, fuel)?;
                    out = merge_keywords(out, kw);
                }
                Ok(out)
            }
            "Codec.$Meta.$Adt" => {
                let reps = slice_reps(heap, &fields[0])?;
                if reps.is_empty() {
                    return Ok(JVal::obj(vec![("enum", JVal::Arr(vec![]))]));
                }
                let mut members = Vec::with_capacity(reps.len());
                for r in &reps {
                    let (nv, fv) = if let Value::Object(tok) = r
                        && let Data::Tagged { tag, fields, .. } = &heap.object(tok.id)?.data
                        && tag == "$tuple"
                        && fields.len() == 3
                    {
                        (fields[1].representation(), fields[2].representation())
                    } else {
                        pair_of(heap, r)?
                    };
                    let vname = heap.string_lossy(&nv)?;
                    let mut entries = vec![(
                        "_tag".to_owned(),
                        JVal::obj(vec![("const", JVal::Str(vname.clone()))]),
                    )];
                    for (i, fm) in slice_reps(heap, &fv)?.iter().enumerate() {
                        let key = format!("_{i}");
                        let fp = at_path(&at_path(path, &vname), &key);
                        entries.push((key, self.export(heap, fm, &fp, fuel)?));
                    }
                    members.push(closed_object(entries, true));
                }
                Ok(JVal::obj(vec![("oneOf", JVal::Arr(members))]))
            }
            "Codec.$Meta.$Refined" => {
                let name = heap.string_lossy(&fields[0])?;
                let known = opt_val(heap, &fields[1])?
                    .map(|v| heap.string_lossy(&v))
                    .transpose()?;
                let kw = match known.as_deref() {
                    Some("email") => vec![("format".to_owned(), JVal::Str("email".into()))],
                    Some("int") => vec![
                        ("type".to_owned(), JVal::Str("integer".into())),
                        ("minimum".to_owned(), JVal::Num(INT32_MIN)),
                        ("maximum".to_owned(), JVal::Num(INT32_MAX)),
                    ],
                    Some("non_empty") => vec![("minLength".to_owned(), JVal::Num(1.0))],
                    Some("positive") => vec![("exclusiveMinimum".to_owned(), JVal::Num(0.0))],
                    _ => {
                        return Err(refuse(
                            path,
                            &format!("the check of {name} has no JSON Schema form"),
                        ));
                    }
                };
                let mut base = self.export(heap, &fields[2], path, fuel)?;
                for (k, v) in kw {
                    base.set(k, v);
                }
                Ok(base)
            }
            "Codec.$Meta.$Checked" => {
                let mut out = self.export(heap, &fields[0], path, fuel)?;
                let is_obj = matches!(out, JVal::Obj(_));
                let typ = out.get_str("type").unwrap_or("").to_owned();
                let does_transform = transforms(heap, &fields[0])?;
                let mut acc = Vec::new();
                for chk in &slice_reps(heap, &fields[1])? {
                    acc.extend(Self::check_keywords(
                        heap,
                        chk,
                        &typ,
                        path,
                        does_transform,
                        fuel,
                    )?);
                }
                if !is_obj {
                    return Ok(out);
                }
                if let Some(t) = opt_val(heap, &fields[2])? {
                    out.set("title".into(), JVal::Str(heap.string_lossy(&t)?));
                }
                if let Some(d) = opt_val(heap, &fields[3])? {
                    out.set("description".into(), JVal::Str(heap.string_lossy(&d)?));
                }
                Ok(merge_keywords(out, acc))
            }
            "Codec.$Meta.$ResolvedLazy" => {
                let name = heap.string_lossy(&fields[0])?;
                if !self.defs.iter().any(|(k, _)| k == &name)
                    && let Some(inner) = opt_val(heap, &fields[1])?
                {
                    self.defs.push((name.clone(), None));
                    let resolved = self.export(heap, &inner, path, fuel)?;
                    if let Some((_, slot)) = self.defs.iter_mut().find(|(k, _)| k == &name) {
                        *slot = Some(resolved);
                    }
                }
                Ok(JVal::obj(vec![(
                    "$ref",
                    JVal::Str(format!("#/$defs/{name}")),
                )]))
            }
            _ => Err(refuse(path, "this codec has no JSON Schema form")),
        }
    }

    fn check_keywords(
        heap: &Heap,
        chk_meta: &Value,
        typ: &str,
        path: &str,
        does_transform: bool,
        fuel: Fuel<'_>,
    ) -> Result<Vec<(String, JVal)>, ExportErr> {
        let (tag, fields) = tagged_fields(heap, chk_meta)?;
        let desc = heap.string_lossy(&fields[0])?;
        if tag == "Check.$Meta.$None" {
            return Err(refuse(
                path,
                &format!("the check \"{desc}\" has no JSON Schema form"),
            ));
        }
        if does_transform {
            return Err(refuse(
                path,
                &format!("the check \"{desc}\" applies after a transformation"),
            ));
        }
        match tag.as_str() {
            "Check.$Meta.$Approx" => Err(refuse(
                path,
                &format!("the check \"{desc}\" has no exact JSON Schema form"),
            )),
            "Check.$Meta.$Exact" => Ok(match value_to_jval(heap, &fields[1], fuel)? {
                JVal::Obj(kws) => kws,
                _ => vec![],
            }),
            "Check.$Meta.$Props" => {
                if typ != "object" {
                    return Err(refuse(
                        path,
                        &format!("the check \"{desc}\" has no exact JSON Schema form"),
                    ));
                }
                Ok(match value_to_jval(heap, &fields[1], fuel)? {
                    JVal::Obj(kws) => kws,
                    _ => vec![],
                })
            }
            "Check.$Meta.$Length" => {
                let min_v = opt_val(heap, &fields[1])?.and_then(|v| match v {
                    Value::Int(i) => Some(i),
                    _ => None,
                });
                let max_v = opt_val(heap, &fields[2])?.and_then(|v| match v {
                    Value::Int(i) => Some(i),
                    _ => None,
                });
                match (typ, min_v, max_v) {
                    ("array", Some(lo), Some(hi)) => Ok(vec![
                        ("minItems".into(), JVal::Num(f64::from(lo))),
                        ("maxItems".into(), JVal::Num(f64::from(hi))),
                    ]),
                    ("array", Some(lo), None) => {
                        Ok(vec![("minItems".into(), JVal::Num(f64::from(lo)))])
                    }
                    ("array", None, Some(hi)) => {
                        Ok(vec![("maxItems".into(), JVal::Num(f64::from(hi)))])
                    }
                    ("string", Some(lo), None) if lo <= 1 => Ok(vec![(
                        "minLength".into(),
                        JVal::Num(f64::from((lo + 1) / 2)),
                    )]),
                    ("string", None, Some(0)) => Ok(vec![("maxLength".into(), JVal::Num(0.0))]),
                    ("string", Some(lo), Some(hi)) if hi == 0 || (lo + 1) / 2 > hi => Ok(vec![
                        ("minLength".into(), JVal::Num(f64::from((lo + 1) / 2))),
                        ("maxLength".into(), JVal::Num(f64::from(hi))),
                    ]),
                    _ => Err(refuse(
                        path,
                        &format!("the check \"{desc}\" has no exact JSON Schema form"),
                    )),
                }
            }
            _ => Err(refuse(
                path,
                &format!("the check \"{desc}\" has no JSON Schema form"),
            )),
        }
    }
}

pub(super) fn export_json_schema(
    heap: &mut Heap,
    root_val: &Value,
    meta_val: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let root = heap.string_lossy(root_val)?;
    let mut exporter = Exporter { defs: Vec::new() };
    let top = match exporter.export(heap, meta_val, &root, fuel) {
        Ok(JVal::Obj(entries)) => entries,
        Ok(_) => {
            let msg = heap.literal(&Lit::String(format!(
                "{root} -> JSON Schema: no value has this type"
            )))?;
            let err = heap.construct("Codec.ExportError.ExportError", vec![msg])?;
            return heap.construct("Prelude.Result.Err", vec![err]);
        }
        Err(ExportErr::Refused(reason)) => {
            let msg = heap.literal(&Lit::String(reason))?;
            let err = heap.construct("Codec.ExportError.ExportError", vec![msg])?;
            return heap.construct("Prelude.Result.Err", vec![err]);
        }
        Err(ExportErr::Vm(err)) => return Err(err),
    };
    let mut out = Vec::with_capacity(top.len() + 2);
    out.push((
        "$schema".to_owned(),
        JVal::Str("https://json-schema.org/draft/2020-12/schema".into()),
    ));
    out.extend(top);
    let defs: Vec<(String, JVal)> = exporter
        .defs
        .into_iter()
        .filter_map(|(k, v)| v.map(|j| (k, j)))
        .collect();
    if !defs.is_empty() {
        out.push(("$defs".to_owned(), JVal::Obj(defs)));
    }
    let indented = indent_json(&jval_compact(&JVal::Obj(out)), 2);
    let text = heap.literal(&Lit::String(indented))?;
    heap.construct("Prelude.Result.Ok", vec![text])
}
